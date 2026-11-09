//! Native port of gui/sections/hotkeys.rs and hotkeys.css.
use crate::settings_ui::{
    HotkeyModalRequest, SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, gray, lerp, mix_pct},
    controls::{
        self, ButtonKind, RangeOptions, SegmentedToggleOption, SelectOption, SelectOptions,
    },
    live, style,
};
use gpui::{prelude::*, *};
use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

const DEFAULT_TARGET_LABEL: &str = "Default";
const ALL_MICROPHONES_LABEL: &str = "All microphones";

#[derive(Default)]
pub(crate) struct State {
    draft: Option<crate::HotkeyBinding>,
    editing: bool,
    gamepad: bool,
    keyboard_draft: Option<crate::Shortcut>,
    gamepad_draft: Option<crate::GamepadShortcut>,
    recording: bool,
    held: HashSet<u32>,
    modifier_order: Vec<u32>,
    preview: Option<crate::Shortcut>,
    gamepad_preview: Option<crate::GamepadShortcut>,
    hold: Option<Instant>,
    progress: f32,
    entered: Option<Instant>,
    closing: Option<Instant>,
    picker: bool,
    picker_entered: Option<Instant>,
    picker_closing: Option<Instant>,
    pending_exit: Option<Instant>,
    poll: Option<Task<()>>,
    focus: Option<FocusHandle>,
}

impl Drop for State {
    fn drop(&mut self) {
        if self.draft.is_some() {
            crate::set_settings_hotkey_recording(false);
            crate::set_settings_gamepad_recording(false);
        }
    }
}

fn stop_recording(app: &mut SettingsApp) {
    let s = &mut app.sections.hotkeys;
    s.recording = false;
    s.hold = None;
    s.progress = 0.;
    s.pending_exit = Some(Instant::now());
    crate::set_settings_hotkey_recording(false);
    crate::set_settings_gamepad_recording(false);
}

fn open(
    app: &mut SettingsApp,
    request: HotkeyModalRequest,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    stop_recording(app);
    let (mut binding, editing) = match request {
        HotkeyModalRequest::Edit { binding } => (binding, true),
        HotkeyModalRequest::Add { preset_action } => {
            let action = preset_action.unwrap_or_default();
            let target = default_target_for_action(
                action,
                &app.snapshot.devices,
                &app.snapshot.output_devices,
                &app.snapshot.config.hotkeys,
            );
            (
                crate::HotkeyBinding {
                    action,
                    target: draft_target_for(action, target),
                    ..Default::default()
                },
                false,
            )
        }
    };
    binding.ignore_modifiers &= binding.gamepad.is_none();
    let s = &mut app.sections.hotkeys;
    s.gamepad = binding.gamepad.is_some();
    s.keyboard_draft = editing.then(|| binding.shortcut.clone());
    s.gamepad_draft = binding.gamepad.clone();
    s.draft = Some(binding);
    s.editing = editing;
    s.entered = Some(Instant::now());
    s.focus = Some(cx.focus_handle());
    s.closing = None;
    s.picker = false;
    s.picker_closing = None;
    s.preview = None;
    s.gamepad_preview = None;
    s.pending_exit = None;
    app.open_select = None;
    app.sections
        .hotkeys
        .focus
        .as_ref()
        .unwrap()
        .focus(window, cx);
    app.update_config(cx, |c| c.hotkeys_paused = true);
    app.sections.hotkeys.poll = Some(cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(16))
                .await;
            if !matches!(
                this.update(cx, |app, cx| {
                    if app.sections.hotkeys.draft.is_none() {
                        return false;
                    }
                    poll(app, cx);
                    true
                }),
                Ok(true)
            ) {
                break;
            }
        }
    }));
}

fn close(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.sections.hotkeys.closing.is_some() {
        return;
    }
    stop_recording(app);
    app.open_select = None;
    app.sections.hotkeys.closing = Some(Instant::now());
    app.update_config(cx, |c| c.hotkeys_paused = false);
}

fn persist(app: &mut SettingsApp, create: bool, cx: &mut Context<SettingsApp>) {
    let s = &app.sections.hotkeys;
    if !create && !s.editing {
        cx.notify();
        return;
    }
    let Some(mut draft) = s.draft.clone() else {
        return;
    };
    draft.shortcut = s.keyboard_draft.clone().unwrap_or_default();
    draft.gamepad = if s.gamepad {
        s.gamepad_draft.clone()
    } else {
        None
    };
    draft.ignore_modifiers &= !s.gamepad;
    let old = app
        .snapshot
        .config
        .hotkeys
        .iter()
        .find(|b| b.id == draft.id);
    let names = &app.snapshot.config.advanced.audio_device_name_display;
    draft.target_name = draft_target_name_for(
        draft.action,
        draft.target.as_deref().unwrap_or_default(),
        old.and_then(|b| b.target.as_deref()),
        old.and_then(|b| b.target_name.clone()),
        &app.snapshot.devices,
        &app.snapshot.output_devices,
        names,
    );
    draft.target_2_name = draft_target_name_for(
        draft.action,
        draft.target_2.as_deref().unwrap_or_default(),
        old.and_then(|b| b.target_2.as_deref()),
        old.and_then(|b| b.target_2_name.clone()),
        &app.snapshot.devices,
        &app.snapshot.output_devices,
        names,
    );
    app.sections.hotkeys.draft = Some(draft.clone());
    app.update_config(cx, move |c| {
        if create {
            c.hotkeys.push(draft);
        } else if let Some(b) = c.hotkeys.iter_mut().find(|b| b.id == draft.id) {
            *b = draft;
        }
        sync_legacy_shortcut(c);
    });
}

fn begin_recording(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) {
    if app.sections.hotkeys.recording {
        stop_recording(app);
        cx.notify();
        return;
    }
    for _ in live::keys().try_iter() {}
    let _ = crate::take_settings_mouse_pressed_shortcut();
    let _ = crate::take_settings_alt_space_recorded();
    let s = &mut app.sections.hotkeys;
    s.recording = true;
    s.preview = None;
    s.gamepad_preview = None;
    s.hold = None;
    s.progress = 0.;
    s.pending_exit = None;
    s.held.clear();
    s.modifier_order.clear();
    for vk in [
        0xA0,
        0xA1,
        0xA2,
        0xA3,
        0xA4,
        0xA5,
        crate::VK_LWIN,
        crate::VK_RWIN,
    ] {
        if crate::key_down(vk) {
            s.held.insert(vk);
        }
    }
    crate::set_settings_hotkey_recording(true);
    crate::set_settings_gamepad_recording(s.gamepad);
    app.sections
        .hotkeys
        .focus
        .as_ref()
        .unwrap()
        .focus(window, cx);
    cx.notify();
}

fn capture(
    app: &mut SettingsApp,
    shortcut: Option<crate::Shortcut>,
    gamepad: Option<crate::GamepadShortcut>,
    cx: &mut Context<SettingsApp>,
) {
    let s = &mut app.sections.hotkeys;
    if let Some(shortcut) = shortcut {
        s.keyboard_draft = Some(shortcut);
    }
    if let Some(gamepad) = gamepad {
        s.gamepad_draft = Some(gamepad);
    }
    stop_recording(app);
    persist(app, false, cx);
}

fn poll(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app
        .sections
        .hotkeys
        .closing
        .is_some_and(|t| t.elapsed() >= Duration::from_millis(190))
    {
        app.sections.hotkeys.draft = None;
        cx.notify();
        return;
    }
    if app
        .sections
        .hotkeys
        .picker_closing
        .is_some_and(|t| t.elapsed() >= Duration::from_millis(180))
    {
        app.sections.hotkeys.picker = false;
        app.sections.hotkeys.picker_closing = None;
        cx.notify();
    }
    if !app.sections.hotkeys.recording {
        return;
    }
    if app.sections.hotkeys.gamepad {
        for _ in live::keys().try_iter() {}
        let mut inputs = crate::settings_gamepad_held_inputs();
        inputs.retain(|i| matches!(i, crate::GamepadInput::Button { .. }));
        inputs.truncate(2);
        let next = (!inputs.is_empty()).then_some(crate::GamepadShortcut { inputs });
        let s = &mut app.sections.hotkeys;
        if next != s.gamepad_preview {
            s.hold = next.as_ref().map(|_| Instant::now());
            s.progress = 0.;
        }
        s.gamepad_preview = next.clone();
        s.progress = s
            .hold
            .map(|t| t.elapsed().as_secs_f32().min(1.))
            .unwrap_or(0.);
        if next.as_ref().is_some_and(|g| g.inputs.len() == 2) || s.progress >= 1. {
            capture(app, None, next, cx);
        }
    } else {
        for key in live::keys().try_iter() {
            let vk = match key.vk {
                crate::VK_CONTROL => {
                    if key.extended {
                        0xA3
                    } else {
                        0xA2
                    }
                }
                crate::VK_MENU => {
                    if key.extended {
                        0xA5
                    } else {
                        0xA4
                    }
                }
                v => v,
            };
            let s = &mut app.sections.hotkeys;
            if key.down {
                s.held.insert(vk);
            } else {
                s.held.remove(&vk);
            }
            let modifiers = crate::ModifierState {
                ctrl: s.held.contains(&0xA2) || s.held.contains(&0xA3),
                alt: s.held.contains(&0xA4) || s.held.contains(&0xA5),
                shift: s.held.contains(&crate::VK_SHIFT)
                    || s.held.contains(&0xA0)
                    || s.held.contains(&0xA1),
                win: s.held.contains(&crate::VK_LWIN) || s.held.contains(&crate::VK_RWIN),
            };
            for (vk, down) in [
                (crate::VK_CONTROL, modifiers.ctrl),
                (crate::VK_MENU, modifiers.alt),
                (crate::VK_SHIFT, modifiers.shift),
                (crate::VK_LWIN, modifiers.win),
            ] {
                if !down {
                    s.modifier_order.retain(|v| *v != vk);
                } else if !s.modifier_order.contains(&vk) {
                    s.modifier_order.push(vk);
                }
            }
            if key.down {
                if let Some(shortcut) =
                    crate::shortcut_from_raw_vk(key.vk, key.extended, &modifiers)
                {
                    if shortcut.vk != 0 {
                        capture(app, Some(shortcut), None, cx);
                        break;
                    }
                }
            }
        }
        if app.sections.hotkeys.recording {
            if crate::take_settings_alt_space_recorded() {
                capture(
                    app,
                    Some(crate::Shortcut {
                        alt: true,
                        vk: 0x20,
                        ..Default::default()
                    }),
                    None,
                    cx,
                );
            } else if let Some(shortcut) = crate::take_settings_mouse_pressed_shortcut() {
                capture(app, Some(shortcut), None, cx);
            } else {
                let next = current_mouse_shortcut().or_else(current_modifier_shortcut);
                let s = &mut app.sections.hotkeys;
                if next != s.preview {
                    s.hold = next.as_ref().map(|_| Instant::now());
                    s.progress = 0.;
                }
                s.preview = next.clone();
                s.progress = s
                    .hold
                    .map(|t| t.elapsed().as_secs_f32().min(1.))
                    .unwrap_or(0.);
                if s.progress >= 1.
                    || next
                        .as_ref()
                        .is_some_and(|v| !v.mouse_buttons.is_empty() && shortcut_has_modifier(v))
                {
                    capture(app, next, None, cx);
                }
            }
        }
    }
    cx.notify();
}

fn change_action(
    app: &mut SettingsApp,
    action: crate::HotkeyAction,
    cx: &mut Context<SettingsApp>,
) {
    let Some(d) = app.sections.hotkeys.draft.as_mut() else {
        return;
    };
    let old = d.action;
    d.action = action;
    let target = d.target.clone().unwrap_or_default();
    if (action_uses_volume_target(action) && old != action)
        || !action.needs_target()
        || !target_is_valid_for_action(
            action,
            &target,
            &app.snapshot.devices,
            &app.snapshot.output_devices,
        )
    {
        d.target = draft_target_for(
            action,
            default_target_for_action(
                action,
                &app.snapshot.devices,
                &app.snapshot.output_devices,
                &app.snapshot.config.hotkeys,
            ),
        );
    }
    if !action.needs_second_target()
        || !target_is_valid_for_action(
            action,
            d.target_2.as_deref().unwrap_or_default(),
            &app.snapshot.devices,
            &app.snapshot.output_devices,
        )
    {
        d.target_2 = draft_second_target_for(
            action,
            default_second_target_for_action(
                action,
                d.target.as_deref().unwrap_or_default(),
                &app.snapshot.devices,
                &app.snapshot.output_devices,
            ),
        );
    }
    app.sections.hotkeys.picker_closing = Some(Instant::now());
    persist(app, false, cx);
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    // Keep cached device names current, including disconnected-device fallback.
    let mut updates = Vec::new();
    for b in &app.snapshot.config.hotkeys {
        for (second, target, saved) in [
            (false, &b.target, &b.target_name),
            (true, &b.target_2, &b.target_2_name),
        ] {
            if let Some(name) = target.as_deref().and_then(|t| {
                device_name_for_action(
                    b.action,
                    t,
                    &app.snapshot.devices,
                    &app.snapshot.output_devices,
                    &app.snapshot.config.advanced.audio_device_name_display,
                )
            }) {
                if saved.as_ref() != Some(&name) {
                    updates.push((b.id.clone(), second, name));
                }
            }
        }
    }
    if !updates.is_empty() {
        app.update_config(cx, |c| {
            for (id, second, name) in updates {
                if let Some(b) = c.hotkeys.iter_mut().find(|b| b.id == id) {
                    if second {
                        b.target_2_name = Some(name);
                    } else {
                        b.target_name = Some(name);
                    }
                }
            }
        });
    }
    let p = app.palette;
    let header = div()
        .flex()
        .h(px(40.))
        .items_center()
        .justify_between()
        .gap(px(14.))
        .child(style::h1("Hotkeys"))
        .child(controls::button(
            "hotkey-add",
            ButtonKind::Secondary,
            Some("plus-linear"),
            Some("Add hotkey".into()),
            false,
            window,
            cx,
            |app, _, _, cx| {
                app.hotkey_modal_request = Some(HotkeyModalRequest::Add {
                    preset_action: None,
                });
                cx.notify();
            },
        ));
    let mut table = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .w_full()
        .min_w_0()
        .overflow_hidden();
    if app.snapshot.config.hotkeys.is_empty() {
        table = table.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .py(px(32.))
                .rounded(px(12.))
                .bg(p.surface.hsla())
                .border_1()
                .border_dashed()
                .border_color(p.border.hsla())
                .child(
                    svg()
                        .path(icon("keyboard-bold"))
                        .size(px(20.))
                        .text_color(p.faint.hsla()),
                )
                .child(style::p("No hotkeys configured.", cx)),
        );
    }
    for b in app.snapshot.config.hotkeys.clone() {
        let id = SharedString::from(format!("hotkey-row-{}", b.id));
        let (pointer, hover, _) = controls::hover_tween(id.clone(), window, cx);
        let edit = b.clone();
        let remove = b.id.clone();
        let title = div()
            .flex()
            .flex_col()
            .gap(px(2.))
            .flex_grow(1.)
            .flex_basis(px(0.))
            .map(|mut el| {
                el.style().flex_grow = Some(1.2);
                el
            })
            .min_w_0()
            .child(
                style::h3(b.action.label())
                    .text_size(px(15.))
                    .text_ellipsis(),
            )
            .when(b.action.needs_target(), |el| {
                el.child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.muted.hsla())
                        .text_ellipsis()
                        .child(target_label(
                            b.action,
                            b.target.as_deref(),
                            b.target_name.as_deref(),
                            b.target_2.as_deref(),
                            b.target_2_name.as_deref(),
                            &app.snapshot.devices,
                            &app.snapshot.output_devices,
                            &app.snapshot.config.advanced.audio_device_name_display,
                        )),
                )
            });
        let display = key_display(
            app,
            &format!("row-{}", b.id),
            Some(&b.shortcut),
            b.gamepad.as_ref(),
            false,
            false,
            window,
            cx,
        );
        let shortcut = div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .flex_grow(1.)
            .flex_basis(px(0.))
            .map(|mut el| {
                el.style().flex_grow = Some(1.1);
                el
            })
            .min_w_0()
            .child(display)
            .when(b.gamepad.is_none() && b.ignore_modifiers, |el| {
                el.child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.muted.hsla())
                        .child("Ignores modifiers"),
                )
            });
        // `.hotkey-row-actions .icon-button`: transparent until the button itself is hovered.
        let (ep, eh, _) = controls::hover_tween((ElementId::from(id.clone()), "edit"), window, cx);
        let edit_button = controls::track_pointer(
            div()
                .id((ElementId::from(id.clone()), "edit"))
                .size(px(36.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(lerp(TRANSPARENT, p.surface_3, eh).hsla())
                .child(
                    svg()
                        .path(icon("pen-linear"))
                        .size(px(18.))
                        .text_color(p.muted.hsla()),
                ),
            ep,
        )
        .tooltip(|_, cx| controls::tooltip("Edit hotkey", cx))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.hotkey_modal_request = Some(HotkeyModalRequest::Edit {
                binding: edit.clone(),
            });
            cx.notify();
        }));
        let (dp, dh, _) =
            controls::hover_tween((ElementId::from(id.clone()), "remove"), window, cx);
        let delete = controls::track_pointer(
            div()
                .id((ElementId::from(id.clone()), "remove"))
                .size(px(36.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .bg(lerp(TRANSPARENT, p.surface_3, dh).hsla())
                .child(
                    svg()
                        .path(icon("trash-bin-trash-linear"))
                        .size(px(18.))
                        .text_color(lerp(p.muted, p.danger, dh).hsla()),
                ),
            dp,
        )
        .tooltip(|_, cx| controls::tooltip("Remove hotkey", cx))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.update_config(cx, |c| {
                c.hotkeys.retain(|b| b.id != remove);
                sync_legacy_shortcut(c);
            })
        }));
        table = table.child(controls::track_pointer(
            div()
                .id(id)
                .flex()
                .items_center()
                .gap(px(16.))
                .min_w_0()
                .px(px(20.))
                .py(px(16.))
                .rounded(px(12.))
                .bg(p.surface_2.hsla())
                .child(title)
                .child(shortcut)
                .child(
                    div()
                        .flex()
                        .opacity(0.4 + hover * 0.6)
                        .child(edit_button)
                        .child(delete),
                ),
            pointer,
        ));
    }
    vec![
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .w_full()
            .min_w_0()
            .child(header)
            .child(table)
            .into_any_element(),
    ]
}

fn target_options(
    action: crate::HotkeyAction,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    name_display: &str,
) -> Vec<SelectOption> {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .into_iter()
            .map(|device| {
                let name = device.display_name(name_display);
                SelectOption::new(device.id, name).icon("microphone-3-linear")
            })
            .collect(),
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .into_iter()
            .map(|device| {
                let name = device.display_name(name_display);
                SelectOption::new(device.id, name).icon("volume-loud-linear")
            })
            .collect(),
        _ => {
            let default_detail = default_target_detail(&devices, name_display);
            std::iter::once(
                SelectOption::new("", DEFAULT_TARGET_LABEL)
                    .detail(default_detail)
                    .icon("microphone-3-linear"),
            )
            .chain(std::iter::once(
                SelectOption::new(crate::HOTKEY_TARGET_ALL_MICROPHONES, ALL_MICROPHONES_LABEL)
                    .detail("Apply the action to every active microphone")
                    .icon("microphone-3-linear"),
            ))
            .chain(devices.into_iter().map(|device| {
                let name = device.display_name(name_display);
                SelectOption::new(device.id, name).icon("microphone-3-linear")
            }))
            .collect()
        }
    }
}

fn action_options() -> Vec<SelectOption> {
    const ORDERED_ACTIONS: &[crate::HotkeyAction] = &[
        crate::HotkeyAction::Mute,
        crate::HotkeyAction::Unmute,
        crate::HotkeyAction::ToggleMute,
        crate::HotkeyAction::HoldToMute,
        crate::HotkeyAction::HoldToUnmute,
        crate::HotkeyAction::HoldToToggle,
        crate::HotkeyAction::SetDefaultOutputDevice,
        crate::HotkeyAction::ToggleDefaultOutputDevice,
        crate::HotkeyAction::NextOutputDevice,
        crate::HotkeyAction::PreviousOutputDevice,
        crate::HotkeyAction::SetDefaultInputDevice,
        crate::HotkeyAction::ToggleDefaultInputDevice,
        crate::HotkeyAction::IncreaseVolume,
        crate::HotkeyAction::DecreaseVolume,
        crate::HotkeyAction::SetVolume,
        crate::HotkeyAction::ToggleOutputMute,
        crate::HotkeyAction::OpenSettings,
    ];

    ORDERED_ACTIONS
        .iter()
        .map(|action| {
            let option = SelectOption::new(action.id(), action.label());
            match action {
                crate::HotkeyAction::ToggleMute => option.group("Mute").icon("microphone-3-linear"),
                crate::HotkeyAction::Mute => option.group("Mute").icon("microphone-3-linear"),
                crate::HotkeyAction::Unmute => option.group("Mute").icon("microphone-3-linear"),
                crate::HotkeyAction::HoldToToggle => {
                    option.group("Hold to mute").icon("oven-mitts-linear")
                }
                crate::HotkeyAction::HoldToMute => {
                    option.group("Hold to mute").icon("oven-mitts-linear")
                }
                crate::HotkeyAction::HoldToUnmute => {
                    option.group("Hold to mute").icon("oven-mitts-linear")
                }
                crate::HotkeyAction::SetDefaultInputDevice => {
                    option.group("Devices").icon("microphone-3-linear")
                }
                crate::HotkeyAction::SetDefaultOutputDevice => {
                    option.group("Devices").icon("volume-loud-linear")
                }
                crate::HotkeyAction::ToggleDefaultInputDevice => {
                    option.group("Devices").icon("microphone-3-linear")
                }
                crate::HotkeyAction::ToggleDefaultOutputDevice => {
                    option.group("Devices").icon("volume-loud-linear")
                }
                crate::HotkeyAction::NextOutputDevice => {
                    option.group("Devices").icon("volume-loud-linear")
                }
                crate::HotkeyAction::PreviousOutputDevice => {
                    option.group("Devices").icon("volume-loud-linear")
                }
                crate::HotkeyAction::SetVolume => option.group("Volume").icon("volume-loud-linear"),
                crate::HotkeyAction::IncreaseVolume => {
                    option.group("Volume").icon("volume-loud-linear")
                }
                crate::HotkeyAction::DecreaseVolume => {
                    option.group("Volume").icon("volume-loud-linear")
                }
                crate::HotkeyAction::ToggleOutputMute => {
                    option.group("Volume").icon("volume-loud-linear")
                }
                crate::HotkeyAction::OpenSettings => option.group("System").icon("settings-linear"),
            }
        })
        .collect()
}

fn target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    target_name: Option<&str>,
    target_2: Option<&str>,
    target_2_name: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> String {
    if action.needs_second_target() {
        let first = device_target_label(
            action,
            target,
            target_name,
            devices,
            output_devices,
            name_display,
        );
        let second = device_target_label(
            action,
            target_2,
            target_2_name,
            devices,
            output_devices,
            name_display,
        );
        return format!("{first} / {second}");
    }

    device_target_label(
        action,
        target,
        target_name,
        devices,
        output_devices,
        name_display,
    )
}

fn device_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    saved_name: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> String {
    if action_uses_volume_target(action) {
        let value = volume_target_value(target.unwrap_or_default(), action);
        return format!("{value}%");
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultInputDevice | crate::HotkeyAction::ToggleDefaultInputDevice
    ) {
        return target
            .and_then(|target| {
                device_name_for_action(action, target, devices, output_devices, name_display)
            })
            .or_else(|| {
                saved_name
                    .filter(|name| !name.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Choose an input device".to_string());
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultOutputDevice
            | crate::HotkeyAction::ToggleDefaultOutputDevice
    ) {
        return target
            .and_then(|target| {
                device_name_for_action(action, target, devices, output_devices, name_display)
            })
            .or_else(|| {
                saved_name
                    .filter(|name| !name.trim().is_empty())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "Choose an output device".to_string());
    }

    if matches!(target, Some(crate::HOTKEY_TARGET_ALL_MICROPHONES)) {
        return ALL_MICROPHONES_LABEL.to_string();
    }

    target
        .filter(|target| !target.is_empty())
        .and_then(|target| {
            device_name_for_action(action, target, devices, output_devices, name_display)
        })
        .or_else(|| {
            saved_name
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| DEFAULT_TARGET_LABEL.to_string())
}

fn device_name_for_action(
    action: crate::HotkeyAction,
    target: &str,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> Option<String> {
    if matches!(
        action,
        crate::HotkeyAction::SetDefaultOutputDevice
            | crate::HotkeyAction::ToggleDefaultOutputDevice
    ) {
        return output_devices
            .iter()
            .find(|device| device.id == target)
            .map(|device| device.display_name(name_display));
    }

    devices
        .iter()
        .find(|device| device.id == target)
        .map(|device| device.display_name(name_display))
}

fn target_is_valid_for_action(
    action: crate::HotkeyAction,
    target: &str,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> bool {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => {
            devices.iter().any(|device| device.id == target)
        }
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => {
            output_devices.iter().any(|device| device.id == target)
        }
        crate::HotkeyAction::SetVolume
        | crate::HotkeyAction::IncreaseVolume
        | crate::HotkeyAction::DecreaseVolume => {
            target.trim().parse::<u8>().is_ok_and(|value| value <= 100)
        }
        _ => true,
    }
}

fn default_target_for_action(
    action: crate::HotkeyAction,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    hotkeys: &[crate::HotkeyBinding],
) -> String {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .first()
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .first()
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        crate::HotkeyAction::SetVolume => "100".to_string(),
        crate::HotkeyAction::IncreaseVolume | crate::HotkeyAction::DecreaseVolume => hotkeys
            .iter()
            .find(|hotkey| {
                matches!(
                    hotkey.action,
                    crate::HotkeyAction::IncreaseVolume | crate::HotkeyAction::DecreaseVolume
                )
            })
            .and_then(|hotkey| hotkey.target.as_deref())
            .map(|target| volume_target_value(target, action).to_string())
            .unwrap_or_else(|| "10".to_string()),
        _ => String::new(),
    }
}

fn action_uses_volume_target(action: crate::HotkeyAction) -> bool {
    matches!(
        action,
        crate::HotkeyAction::SetVolume
            | crate::HotkeyAction::IncreaseVolume
            | crate::HotkeyAction::DecreaseVolume
    )
}

fn volume_target_value(target: &str, action: crate::HotkeyAction) -> u8 {
    target
        .trim()
        .parse::<u8>()
        .unwrap_or_else(|_| {
            if matches!(action, crate::HotkeyAction::SetVolume) {
                100
            } else {
                10
            }
        })
        .min(100)
}

fn default_second_target_for_action(
    action: crate::HotkeyAction,
    target_1: &str,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> String {
    match action {
        crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .iter()
            .find(|device| device.id != target_1)
            .or_else(|| devices.first())
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .iter()
            .find(|device| device.id != target_1)
            .or_else(|| output_devices.first())
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn default_target_detail(devices: &[crate::MicDevice], name_display: &str) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.display_name(name_display))
        .unwrap_or_else(|| "Use the current Windows default microphone".to_string())
}

fn sync_legacy_shortcut(config: &mut crate::Config) {
    if let Some(shortcut) = config
        .hotkeys
        .iter()
        .find(|binding| {
            binding.gamepad.is_none() && binding.action == crate::HotkeyAction::ToggleMute
        })
        .map(|binding| binding.shortcut.clone())
    {
        config.shortcut = shortcut;
    }
}

fn draft_second_target_for(action: crate::HotkeyAction, target: String) -> Option<String> {
    if action.needs_second_target() && !target.is_empty() {
        Some(target)
    } else {
        None
    }
}

fn draft_target_for(action: crate::HotkeyAction, target: String) -> Option<String> {
    if action.needs_target() && !target.is_empty() {
        Some(target)
    } else {
        None
    }
}

fn draft_target_name_for(
    action: crate::HotkeyAction,
    target: &str,
    previous_target: Option<&str>,
    previous_name: Option<String>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> Option<String> {
    if !action.needs_target()
        || action_uses_volume_target(action)
        || target.is_empty()
        || matches!(target, crate::HOTKEY_TARGET_ALL_MICROPHONES)
    {
        return None;
    }

    device_name_for_action(action, target, devices, output_devices, name_display).or_else(|| {
        (previous_target == Some(target))
            .then_some(previous_name)
            .flatten()
    })
}

fn current_modifier_shortcut() -> Option<crate::Shortcut> {
    let ctrl = crate::key_down(crate::VK_CONTROL);
    let alt = crate::key_down(crate::VK_MENU);
    let shift = crate::key_down(crate::VK_SHIFT);
    let win = crate::key_down(crate::VK_LWIN) || crate::key_down(crate::VK_RWIN);
    if ctrl || alt || shift || win {
        Some(crate::Shortcut {
            ctrl,
            alt,
            shift,
            win,
            vk: 0,
            mouse_buttons: Vec::new(),
        })
    } else {
        None
    }
}

fn current_mouse_shortcut() -> Option<crate::Shortcut> {
    let mut mouse_buttons = crate::settings_mouse_held_buttons();
    mouse_buttons.truncate(2);
    if mouse_buttons.is_empty() {
        return None;
    }

    Some(crate::Shortcut {
        ctrl: crate::key_down(crate::VK_CONTROL),
        alt: crate::key_down(crate::VK_MENU),
        shift: crate::key_down(crate::VK_SHIFT),
        win: crate::key_down(crate::VK_LWIN) || crate::key_down(crate::VK_RWIN),
        vk: 0,
        mouse_buttons,
    })
}

fn shortcut_has_modifier(shortcut: &crate::Shortcut) -> bool {
    shortcut.ctrl || shortcut.alt || shortcut.shift || shortcut.win
}

fn key_display(
    app: &SettingsApp,
    id: &str,
    shortcut: Option<&crate::Shortcut>,
    gamepad: Option<&crate::GamepadShortcut>,
    boxed: bool,
    recording: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = app.palette;
    let s = &app.sections.hotkeys;
    let mut parts = shortcut.map(|s| s.parts()).unwrap_or_default();
    if recording && gamepad.is_none() {
        if let Some(shortcut) = shortcut {
            let mut ordered = Vec::new();
            for vk in &s.modifier_order {
                let (down, label) = match *vk {
                    crate::VK_CONTROL => (shortcut.ctrl, "Ctrl"),
                    crate::VK_MENU => (shortcut.alt, "Alt"),
                    crate::VK_SHIFT => (shortcut.shift, "Shift"),
                    _ => (shortcut.win, "Win"),
                };
                if down {
                    ordered.push(label.to_string());
                }
            }
            for part in parts {
                if !ordered.contains(&part) {
                    ordered.push(part);
                }
            }
            parts = ordered;
        }
    }
    let mut caps = div().flex().items_center().gap(px(6.)).min_w_0();
    let labels = gamepad.map(|g| g.parts()).unwrap_or(parts);
    for (i, label) in labels.into_iter().enumerate() {
        let cap_id = SharedString::from(format!("hotkey-cap-{id}-{i}-{label}"));
        let rise = if boxed {
            anim::tween_from(
                window,
                cx,
                cap_id,
                0.,
                1.,
                Transition::new(200, Easing::Ease),
            )
        } else {
            1.
        };
        let image = gamepad
            .and_then(|g| g.inputs.get(i))
            .and_then(|input| input.icon_id());
        if let Some(image) = image {
            caps = caps.child(
                div()
                    .relative()
                    .top(px(8. * (1. - rise)))
                    .opacity(rise)
                    .h(px(26.))
                    .child(
                        img(SharedString::from(format!("gamepad/{image}.png")))
                            .h(px(26.))
                            .object_fit(ObjectFit::Contain),
                    ),
            );
        } else {
            caps = caps.child(
                div()
                    .relative()
                    .top(px(8. * (1. - rise)))
                    .opacity(rise)
                    .flex_shrink_0()
                    .min_w(px(28.))
                    .max_w(px(92.))
                    .h(px(26.))
                    .px(px(9.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .bg(if recording {
                        mix_pct(p.accent, 28., p.surface_3)
                    } else {
                        p.surface_3
                    }
                    .hsla())
                    .text_size(px(13.))
                    .font_weight(FontWeight(620.))
                    .text_ellipsis()
                    .shadow(vec![BoxShadow {
                        color: p.shadow_medium.hsla(),
                        offset: point(px(0.), px(1.)),
                        blur_radius: px(0.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .child(label),
            );
        }
    }
    let pending = recording && s.hold.is_some();
    let exiting = boxed
        && s.pending_exit
            .is_some_and(|t| t.elapsed() < Duration::from_millis(200));
    if pending || exiting {
        let t = anim::tween_from(
            window,
            cx,
            SharedString::from(format!("hotkey-pending-{id}")),
            0.,
            if pending { 1. } else { 0. },
            Transition::new(200, Easing::Ease),
        );
        // Text here is the source's pending-input prompt, not a substitute icon.
        caps = caps.child(
            div()
                .relative()
                .top(px(8. * (1. - t)))
                .opacity(t)
                .text_size(px(14.))
                .text_color(p.text.hsla())
                .child("+ ..."),
        );
    }
    div()
        .relative()
        .flex()
        .items_center()
        .min_w_0()
        .w_full()
        .min_h(px(28.))
        .overflow_hidden()
        .when(boxed, |el| {
            el.h(px(38.))
                .p(px(5.))
                .border_1()
                .border_color(
                    if recording {
                        mix_pct(p.accent, 46., TRANSPARENT)
                    } else {
                        TRANSPARENT
                    }
                    .hsla(),
                )
                .rounded(px(8.))
                .bg(if recording {
                    mix_pct(p.surface_2, 86., p.accent)
                } else {
                    p.surface_2
                }
                .hsla())
                .shadow(vec![BoxShadow {
                    color: p.shadow_soft.hsla(),
                    offset: point(px(0.), px(1.)),
                    blur_radius: px(2.),
                    spread_radius: px(0.),
                    inset: true,
                }])
        })
        .child(caps)
        .when(recording, |el| {
            el.child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .h(px(2.))
                    .w(relative(s.progress))
                    .bg(p.accent.hsla()),
            )
        })
}

fn dismiss_picker(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.sections.hotkeys.picker_closing.is_none() {
        app.sections.hotkeys.picker_closing = Some(Instant::now());
    }
    cx.notify();
}

fn action_picker(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let p = app.palette;
    let s = &app.sections.hotkeys;
    let action = s.draft.as_ref().unwrap().action;
    let closing = s.picker_closing;
    let t = if let Some(start) = closing {
        1. - Easing::Bezier(0.4, 0., 1., 1.).apply(start.elapsed().as_secs_f32() / 0.18)
    } else {
        Easing::Bezier(0.22, 1., 0.36, 1.)
            .apply(s.picker_entered.unwrap().elapsed().as_secs_f32() / 0.22)
    };
    if t > 0. && t < 1. {
        window.request_animation_frame();
    }
    let mut groups = div()
        .id("hotkey-action-groups")
        .flex()
        .flex_col()
        .gap(px(18.))
        .p(px(32.))
        .pr(px(24.))
        .size_full()
        .overflow_y_scroll();
    let options = action_options();
    for group in ["Mute", "Hold to mute", "Devices", "Volume", "System"] {
        let mut cards = div().grid().grid_cols(2).gap(px(10.));
        for option in options
            .iter()
            .filter(|o| o.group_label.as_deref() == Some(group))
        {
            let value = option.value.clone();
            let selected = value.as_ref() == action.id();
            let id = SharedString::from(format!("hotkey-action-card-{value}"));
            let (pointer, hover, _) = controls::hover_tween(id.clone(), window, cx);
            let amount = if selected { 30. } else { 6. + 14. * hover };
            let card = controls::track_pointer(
                div()
                    .id(id)
                    .relative()
                    .top(px(-hover.max(selected as u8 as f32)))
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .px(px(16.))
                    .py(px(12.))
                    .border_1()
                    .border_color(TRANSPARENT.hsla())
                    .rounded(px(12.))
                    .cursor_pointer()
                    .bg(mix_pct(p.accent, amount, p.surface_2).hsla())
                    .when(selected || hover > 0., |el| {
                        el.shadow(vec![BoxShadow {
                            color: p.shadow_faint.hsla(),
                            offset: point(px(0.), px(8.)),
                            blur_radius: px(20.),
                            spread_radius: px(0.),
                            inset: false,
                        }])
                    })
                    .when_some(option.icon.clone(), |el, name| {
                        el.child(
                            svg()
                                .path(icon(&name))
                                .size(px(22.))
                                .flex_shrink_0()
                                .text_color(p.accent_text.hsla()),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .text_size(px(14.))
                            .font_weight(FontWeight(650.))
                            .line_height(relative(1.16))
                            .child(option.label.clone()),
                    ),
                pointer,
            )
            .on_click(cx.listener(move |app, _, _, cx| {
                change_action(app, crate::HotkeyAction::from_id(&value), cx)
            }));
            cards = cards.child(card);
        }
        groups = groups.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    style::h3(group.to_uppercase())
                        .text_size(px(12.))
                        .font_weight(FontWeight(700.))
                        .text_color(p.muted.hsla()),
                )
                .child(cards),
        );
    }
    div()
        .id("hotkey-picker-backdrop")
        .absolute()
        .inset_0()
        .bg(gray(0., 34. * t).hsla())
        .on_click(cx.listener(|app, _, _, cx| dismiss_picker(app, cx)))
        .child(
            div()
                .id("hotkey-action-sidepanel")
                .absolute()
                .right(px(-32. * (1. - t)))
                .top_0()
                .w(relative(0.7))
                .h_full()
                .opacity(t)
                .rounded_tl(px(24.))
                .rounded_bl(px(24.))
                .bg(p.hotkey_panel_bg.hsla())
                .shadow(vec![BoxShadow {
                    color: p.shadow_medium.hsla(),
                    offset: point(px(-24.), px(0.)),
                    blur_radius: px(54.),
                    spread_radius: px(0.),
                    inset: false,
                }])
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(groups),
        )
        .into_any_element()
}

pub(crate) fn modal_host(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    if let Some(request) = app.hotkey_modal_request.take() {
        open(app, request, window, cx);
    }
    let Some(draft) = app.sections.hotkeys.draft.clone() else {
        if app.sections.hotkeys.poll.take().is_some() {
            app.focus_handle.focus(window, cx);
        }
        return div().into_any_element();
    };
    let p = app.palette;
    let s = &app.sections.hotkeys;
    let editing = s.editing;
    let gamepad = s.gamepad;
    let can_create = if gamepad {
        s.gamepad_draft.is_some()
    } else {
        s.keyboard_draft.is_some()
    };
    let target_missing = draft.action.requires_explicit_target()
        && (draft.target.as_deref().unwrap_or_default().is_empty()
            || (draft.action.needs_second_target()
                && draft.target_2.as_deref().unwrap_or_default().is_empty()));
    let closing = s.closing;
    let elapsed = s.entered.unwrap().elapsed().as_secs_f32();
    let (panel_t, scrim_t) = if let Some(start) = closing {
        let t = start.elapsed().as_secs_f32() / 0.18;
        (
            1. - Easing::Bezier(0.4, 0., 1., 1.).apply(t),
            1. - Easing::Ease.apply(t),
        )
    } else {
        (
            Easing::Bezier(0.22, 1., 0.36, 1.).apply(elapsed / 0.22),
            Easing::Ease.apply(elapsed / 0.16),
        )
    };
    if panel_t > 0. && panel_t < 1. {
        window.request_animation_frame();
    }
    let source = controls::segmented_toggle(
        "hotkey-source",
        if gamepad { "Gamepad" } else { "Keyboard" },
        vec![
            SegmentedToggleOption::new("Keyboard", "Keyboard").icon("keyboard-bold"),
            SegmentedToggleOption::new("Gamepad", "Gamepad").icon("gamepad-bold"),
        ],
        window,
        cx,
        |app, value, _, cx| {
            stop_recording(app);
            app.sections.hotkeys.gamepad = value.as_ref() == "Gamepad";
            app.sections.hotkeys.preview = None;
            app.sections.hotkeys.gamepad_preview = None;
            cx.notify();
        },
    );
    let action_option = action_options()
        .into_iter()
        .find(|o| o.value.as_ref() == draft.action.id())
        .unwrap();
    let (pointer, hover, _) = controls::hover_tween("hotkey-action-trigger", window, cx);
    let trigger = controls::track_pointer(
        div()
            .id("hotkey-action-trigger")
            .h(px(48.))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(14.))
            .pl(px(14.))
            .pr(px(12.))
            .border_1()
            .border_color(TRANSPARENT.hsla())
            .rounded(px(12.))
            .cursor_pointer()
            .bg(lerp(p.surface_2, p.surface_3, hover).hsla())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .when_some(action_option.icon, |el, name| {
                        el.child(
                            svg()
                                .path(icon(&name))
                                .size(px(18.))
                                .text_color(p.muted.hsla()),
                        )
                    })
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight(560.))
                            .child(action_option.label),
                    ),
            )
            .child(
                svg()
                    .path(icon("alt-arrow-down-linear"))
                    .size(px(16.))
                    .text_color(p.muted.hsla()),
            ),
        pointer,
    )
    .on_click(cx.listener(|app, _, _, cx| {
        stop_recording(app);
        app.open_select = None;
        app.sections.hotkeys.picker = true;
        app.sections.hotkeys.picker_entered = Some(Instant::now());
        app.sections.hotkeys.picker_closing = None;
        cx.notify();
    }));
    let recorder = record_panes(app, window, cx);
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(40.))
        .pb(px(32.))
        .child(source)
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(style::label(if gamepad {
                    "Gamepad input"
                } else {
                    "Shortcut"
                }))
                .child(recorder),
        );
    if !gamepad {
        body = body.child(
            controls::checkbox(
                "hotkey-ignore",
                draft.ignore_modifiers,
                false,
                "Ignore modifiers",
                window,
                cx,
                |app, checked, _, cx| {
                    if let Some(d) = app.sections.hotkeys.draft.as_mut() {
                        d.ignore_modifiers = checked;
                    }
                    persist(app, false, cx);
                },
            )
            .mt(px(-2.)),
        );
    }
    body = body.child(
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(style::label("Action"))
            .child(trigger),
    );
    if draft.action.needs_target() {
        if action_uses_volume_target(draft.action) {
            let value =
                volume_target_value(draft.target.as_deref().unwrap_or_default(), draft.action);
            body = body.child(controls::range(
                "hotkey-volume",
                value as f64,
                RangeOptions {
                    min: 0.,
                    max: 100.,
                    step: 1.,
                    label: if draft.action == crate::HotkeyAction::SetVolume {
                        "Volume"
                    } else {
                        "Amount"
                    }
                    .into(),
                    value_label: format!("{value}%").into(),
                    icon: Some("volume-loud-linear".into()),
                },
                window,
                cx,
                |app, value, _, cx| {
                    if let Some(d) = app.sections.hotkeys.draft.as_mut() {
                        d.target = Some((value as u8).to_string());
                    }
                    persist(app, false, cx);
                },
            ));
        } else {
            for second in [false, true] {
                if second && !draft.action.needs_second_target() {
                    continue;
                }
                let target = if second {
                    &draft.target_2
                } else {
                    &draft.target
                };
                let mut options = SelectOptions::new(target_options(
                    draft.action,
                    app.snapshot.devices.clone(),
                    app.snapshot.output_devices.clone(),
                    &app.snapshot.config.advanced.audio_device_name_display,
                ));
                options.trigger_height = Some(48.);
                let select = controls::select(
                    if second {
                        "hotkey-target-2"
                    } else {
                        "hotkey-target"
                    },
                    target.clone().unwrap_or_default(),
                    options,
                    app,
                    window,
                    cx,
                    move |app, value, _, cx| {
                        if let Some(d) = app.sections.hotkeys.draft.as_mut() {
                            if second {
                                d.target_2 = draft_second_target_for(d.action, value.to_string());
                            } else {
                                d.target = draft_target_for(d.action, value.to_string());
                            }
                        }
                        persist(app, false, cx);
                    },
                );
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(style::label(if second {
                            "Target 2"
                        } else if draft.action.needs_second_target() {
                            "Target 1"
                        } else {
                            "Target"
                        }))
                        .child(select),
                );
            }
        }
    }
    let mut actions = div()
        .flex()
        .justify_end()
        .gap(px(14.))
        .pt(px(10.))
        .pb(px(2.));
    if editing {
        actions = actions
            .child(controls::button(
                "hotkey-cancel",
                ButtonKind::Secondary,
                None,
                Some("Cancel".into()),
                false,
                window,
                cx,
                |app, _, _, cx| close(app, cx),
            ))
            .child(controls::button(
                "hotkey-done",
                ButtonKind::Primary,
                None,
                Some("Done".into()),
                false,
                window,
                cx,
                |app, _, _, cx| close(app, cx),
            ));
    } else {
        actions = actions.child(controls::button(
            "hotkey-create",
            ButtonKind::Primary,
            Some("plus-linear"),
            Some("Add hotkey".into()),
            !can_create || target_missing || closing.is_some(),
            window,
            cx,
            |app, _, _, cx| {
                persist(app, true, cx);
                close(app, cx);
            },
        ));
    }
    body = body.child(actions);
    let close_button = controls::button(
        "hotkey-close",
        ButtonKind::Icon,
        Some("codicon_close"),
        None,
        false,
        window,
        cx,
        |app, _, _, cx| close(app, cx),
    );
    let panel = div()
        .id("hotkey-editor-panel")
        .track_focus(app.sections.hotkeys.focus.as_ref().unwrap())
        .relative()
        .right(px(-28. * (1. - panel_t)))
        .w(px(480.))
        .h_full()
        .flex()
        .flex_col()
        .gap(px(24.))
        .pt(px(32.))
        .rounded_tl(px(24.))
        .rounded_bl(px(24.))
        .bg(p.hotkey_panel_bg.hsla())
        .opacity(panel_t)
        .shadow(vec![BoxShadow {
            color: p.shadow_medium.hsla(),
            offset: point(px(-18.), px(0.)),
            blur_radius: px(44.),
            spread_radius: px(0.),
            inset: false,
        }])
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(14.))
                .px(px(40.))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            style::h2(if editing { "Edit hotkey" } else { "Add hotkey" })
                                .text_size(px(23.)),
                        )
                        .child(div().text_size(px(13.)).text_color(p.muted.hsla()).child(
                            if editing {
                                "Changes apply as soon as you make them."
                            } else {
                                "Choose the action, then record the shortcut."
                            },
                        )),
                )
                .child(close_button),
        )
        .child(
            div()
                .id("hotkey-panel-scroll")
                .min_h_0()
                .flex_1()
                .w_full()
                .overflow_y_scroll()
                .child(body),
        );
    let viewport = window.viewport_size();
    let mut overlay = div()
        .id("hotkey-panel-backdrop")
        .occlude()
        .relative()
        .w(viewport.width)
        .h(viewport.height)
        .flex()
        .justify_end()
        .bg(gray(0., 60. * scrim_t).hsla())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _, cx| {
            if app.sections.hotkeys.picker {
                dismiss_picker(app, cx);
            } else {
                close(app, cx);
            }
        }))
        .on_key_down(cx.listener(|app, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && !app.sections.hotkeys.recording {
                if app.sections.hotkeys.picker {
                    dismiss_picker(app, cx);
                } else {
                    close(app, cx);
                }
                window.prevent_default();
                cx.stop_propagation();
            }
        }))
        .child(panel);
    if app.sections.hotkeys.picker {
        overlay = overlay.child(action_picker(app, window, cx));
    }
    deferred(anchored().position(point(px(0.), px(0.))).child(overlay))
        .with_priority(10)
        .into_any_element()
}

/// The CSS track is 200% wide and translates by one pane; both sides stay mounted.
fn record_panes(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let active_gamepad = app.sections.hotkeys.gamepad;
    let slide = anim::tween(
        window,
        cx,
        "hotkey-record-source",
        active_gamepad as u8 as f32,
        Transition::new(240, Easing::Bezier(0.22, 1., 0.36, 1.)),
    );
    let height = anim::tween(
        window,
        cx,
        "hotkey-record-height",
        if active_gamepad { 75.2 } else { 59.6 },
        Transition::new(220, Easing::Bezier(0.22, 1., 0.36, 1.)),
    );
    let mut track = div().relative().left(px(-400. * slide)).w(px(800.)).flex();
    for gamepad in [false, true] {
        let s = &app.sections.hotkeys;
        let recording = s.recording && gamepad == active_gamepad;
        let keyboard = if recording {
            s.preview.as_ref()
        } else {
            s.keyboard_draft.as_ref()
        }
        .cloned();
        let controller = if recording {
            s.gamepad_preview.as_ref()
        } else {
            s.gamepad_draft.as_ref()
        }
        .cloned();
        let id = if gamepad {
            "editor-gamepad"
        } else {
            "editor-keyboard"
        };
        let display = key_display(
            app,
            id,
            if gamepad { None } else { keyboard.as_ref() },
            if gamepad { controller.as_ref() } else { None },
            true,
            recording,
            window,
            cx,
        );
        let record = controls::button(
            SharedString::from(format!("hotkey-record-{id}")),
            ButtonKind::Secondary,
            Some("record-circle-linear"),
            Some(if recording { "Cancel" } else { "Record" }.into()),
            s.closing.is_some(),
            window,
            cx,
            move |app, _, window, cx| {
                app.sections.hotkeys.gamepad = gamepad;
                begin_recording(app, window, cx);
            },
        );
        let opacity = anim::tween(
            window,
            cx,
            SharedString::from(format!("hotkey-record-opacity-{id}")),
            (gamepad == active_gamepad) as u8 as f32,
            Transition::new(180, Easing::Ease),
        );
        let offset = if gamepad {
            10. * (1. - slide)
        } else {
            -10. * slide
        };
        track = track.child(div().relative().left(px(offset)).w(px(400.)).flex_shrink_0().opacity(opacity).flex().flex_col().gap(px(6.))
            .child(div().flex().gap(px(10.)).items_center().child(div().flex_1().min_w_0().child(display)).child(record))
            .child(div().text_size(px(12.)).line_height(relative(1.3)).text_color(app.palette.faint.hsla())
                .child(if gamepad { "Hold one control to bind it, or press a second while the first is still held" } else { "Hold to bind only modifier keys" })));
    }
    div().h(px(height)).w_full().overflow_hidden().child(track)
}
