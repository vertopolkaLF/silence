//! Overlay settings, ported from the Dioxus section and overlay.css.
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, gray, lerp, mix_pct},
    controls::{
        self, ButtonKind, RangeOptions, SelectOption, SelectOptions, TextInputEvent,
        TextInputOptions,
    },
    style::{self, BLOCK_GAP, CONTROL_GAP},
};
use super::form::{self, collapse_in_gap, field, percent_slider};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

#[derive(Default)]
pub(crate) struct State {
    positioning: bool,
    icons_expanded: bool,
    editor: Option<OverlayActionSlot>,
    editor_closing: Option<Instant>,
    picker: bool,
    picker_closing: Option<Instant>,
    focus_pending: bool,
}

fn panel(gap: f32, last: bool, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(gap))
        .max_w(px(720.))
        .flex_shrink_0()
        .when(!last, |d| {
            d.pb(px(BLOCK_GAP))
                .border_b_1()
                .border_color(style::palette(cx).border.hsla())
        })
}
fn slider(
    id: &'static str,
    label: &'static str,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    suffix: &str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    change: impl Fn(&mut crate::OverlayConfig, f64) + 'static,
) -> Stateful<Div> {
    let value = value.clamp(min, max);
    let text = if step < 1. {
        format!("{value:.1}{suffix}")
    } else {
        format!("{value:.0}{suffix}")
    };
    controls::range(
        id,
        value,
        RangeOptions {
            min,
            max,
            step,
            label: label.into(),
            value_label: text.into(),
            icon: (id == "overlay-action-volume").then(|| "volume-loud-linear".into()),
        },
        window,
        cx,
        move |app, value, _, cx| app.update_config(cx, |c| change(&mut c.overlay, value)),
    )
}
fn choose(
    id: &'static str,
    label: &'static str,
    value: String,
    options: SelectOptions,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    change: impl Fn(&mut crate::OverlayConfig, String) + 'static,
) -> Div {
    field(
        label,
        controls::select(
            id,
            value,
            options,
            app,
            window,
            cx,
            move |app, value, _, cx| {
                app.update_config(cx, |c| change(&mut c.overlay, value.to_string()))
            },
        ),
    )
}
fn options(values: &[(&str, &str, &str)]) -> SelectOptions {
    SelectOptions::new(
        values
            .iter()
            .map(|(v, l, i)| SelectOption::new(v.to_string(), l.to_string()).icon(i.to_string()))
            .collect(),
    )
}

/// Monitor rows shared with the volume OSD. The OSD prepends "under cursor".
pub(crate) fn display_options(displays: &[crate::OverlayDisplay]) -> Vec<SelectOption> {
    displays
        .iter()
        .map(|display| {
            SelectOption::new(display.id.clone(), display.label.clone())
                .detail(display.detail.clone())
                .icon("monitor-linear")
        })
        .collect()
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let o = app.snapshot.config.overlay.clone();
    let p = app.palette;
    let custom = o.theme == crate::gpui_overlay::theme::CUSTOM;
    let has_icon = matches!(o.variant.as_str(), "MicIcon" | "IconText");
    let has_text = matches!(o.variant.as_str(), "IconText" | "Text");
    let enable = controls::toggle("overlay-enable", o.enabled, window, cx, |app, v, _, cx| {
        app.update_config(cx, |c| c.overlay.enabled = v)
    });
    let duration = slider(
        "overlay-duration",
        "Duration",
        o.duration_secs,
        0.5,
        10.,
        0.5,
        "s",
        window,
        cx,
        |o, v| o.duration_secs = v,
    );
    let displays = SelectOptions::new(display_options(&app.snapshot.overlay_displays));
    let positioning = controls::toggle(
        "overlay-positioning",
        app.sections.overlay.positioning,
        window,
        cx,
        |app, v, _, cx| {
            app.sections.overlay.positioning = v;
            if let Some(next) = crate::set_overlay_positioning(v) {
                app.update_config(cx, |c| c.overlay = next);
            }
            cx.notify();
        },
    );
    let dim = form::governed_opacity("overlay-governed", o.enabled, window, cx);
    let overview = div()
        .flex()
        .flex_col()
        .flex_shrink_0()
        .max_w(px(720.))
        .child(form::page_title("Overlay", None))
        .child(form::master_switch(
            "overlay-master",
            "monitor-bold",
            "Show the microphone overlay",
            o.enabled,
            enable,
            window,
            cx,
        ))
        .child(
            panel(CONTROL_GAP, false, cx)
                .mt(px(BLOCK_GAP))
                .opacity(dim)
                .child(form::group().child(choose(
                    "overlay-visibility",
                    "Visibility",
                    o.visibility.clone(),
                    options(&[
                        ("Always", "Always visible", "widget-linear"),
                        ("WhenMuted", "Visible when muted", "mic-off"),
                        ("WhenUnmuted", "Visible when unmuted", "mic"),
                        ("WhenMicInUse", "Visible while mic is used", "record-bold"),
                        ("AfterToggle", "Show after toggle", "clock-circle-linear"),
                    ]),
                    app,
                    window,
                    cx,
                    |o, v| o.visibility = v,
                ))
                .child(collapse_in_gap(
                    "overlay-duration-collapse",
                    o.visibility == "AfterToggle",
                    CONTROL_GAP,
                    div().child(duration),
                    window,
                    cx,
                )))
                .child(choose(
                    "overlay-display",
                    "Display",
                    o.display.clone(),
                    displays,
                    app,
                    window,
                    cx,
                    |o, v| o.display = v,
                ))
                .child(
                    percent_slider(
                        "overlay-x",
                        "Horizontal position",
                        o.position_x,
                        "%",
                        window,
                        cx,
                        |app, value, _, cx| {
                            app.update_config(cx, |config| config.overlay.position_x = value);
                        },
                    ),
                )
                .child(
                    percent_slider(
                        "overlay-y",
                        "Vertical position",
                        o.position_y,
                        "%",
                        window,
                        cx,
                        |app, value, _, cx| {
                            app.update_config(cx, |config| config.overlay.position_y = value);
                        },
                    ),
                )
                .child(form::setting_row(
                    "Move overlay",
                    Some("Shows the overlay until this is turned off."),
                    positioning,
                    cx,
                )),
        );
    let mut variants = div().w_full().flex().gap(px(8.));
    for (value, label) in [
        ("MicIcon", "Icon"),
        ("IconText", "Icon + Text"),
        ("Text", "Text"),
        ("Dot", "Dot"),
    ] {
        if value == "Dot" && !custom {
            continue;
        }
        let preview = match value {
            "MicIcon" => svg()
                .path(icon("microphone-3-linear"))
                .size(px(24.))
                .text_color(p.success.hsla())
                .into_any_element(),
            "IconText" => div()
                .flex()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .font_weight(FontWeight(750.))
                .text_color(p.success.hsla())
                .child(
                    svg()
                        .path(icon("microphone-3-linear"))
                        .size(px(20.))
                        .text_color(p.success.hsla()),
                )
                .child("On")
                .into_any_element(),
            "Text" => div()
                .text_size(px(15.))
                .font_weight(FontWeight(800.))
                .text_color(p.text.hsla())
                .child("On")
                .into_any_element(),
            _ => div()
                .size(px(16.))
                .rounded_full()
                .bg(p.success.hsla())
                .into_any_element(),
        };
        variants = variants.child(form::choice_card(
            format!("overlay-variant-{value}").into(),
            label,
            o.variant == value || (value == "IconText" && !custom && o.variant == "Dot"),
            preview,
            window,
            cx,
            move |app, cx| app.update_config(cx, |c| c.overlay.variant = value.to_string()),
        ));
    }
    let mut labels = div().flex().flex_wrap().gap(px(14.));
    for (muted, label, value) in [
        (true, "Muted label", o.muted_label.clone()),
        (false, "Unmuted label", o.unmuted_label.clone()),
    ] {
        let id = if muted {
            "overlay-muted-label"
        } else {
            "overlay-unmuted-label"
        };
        let input = controls::text_input(
            id,
            value,
            TextInputOptions::default(),
            window,
            cx,
            move |app, e, _, cx| {
                if let TextInputEvent::Change(v) = e {
                    app.update_config(cx, |c| {
                        if muted {
                            c.overlay.muted_label = v.to_string()
                        } else {
                            c.overlay.unmuted_label = v.to_string()
                        }
                    });
                }
            },
        );
        let (pointer, hover, _) = controls::hover_tween(format!("{id}-shell"), window, cx);
        let focused = input.read(cx).focus_handle(cx).is_focused(window);
        let focus = anim::tween(
            window,
            cx,
            (ElementId::from(id), "input-focus"),
            focused as u8 as f32,
            anim::UI,
        );
        let shell =
            controls::text_input_container(SharedString::from(format!("{id}-shell")), &input)
                .h(px(44.))
                .px(px(14.))
                .rounded(px(12.))
                .border_1()
                .border_color(lerp(TRANSPARENT, p.focus, focus).hsla())
                .shadow(vec![BoxShadow {
                    color: mix_pct(p.accent, 18., TRANSPARENT).opacity(focus).hsla(),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(1.),
                    inset: false,
                }])
                .bg(lerp(p.surface_2, p.surface_3, hover).hsla())
                // The input sits in a `<label>` (global.css: 14px / 600).
                .text_size(px(14.))
                .font_weight(FontWeight(600.))
                .text_color(p.text.hsla());
        labels = labels.child(
            div()
                .flex_basis(px(220.))
                .flex_grow(1.)
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(8.))
                // `.overlay-text-field { color: var(--muted) }`.
                .child(style::label(label).text_color(p.muted.hsla()))
                .child(controls::track_pointer(shell, pointer)),
        );
    }
    let icons = form::mic_icon_picker(
        "overlay-icon",
        "overlay-icon-rest",
        &o.icon_pair,
        app.snapshot.muted,
        app.sections.overlay.icons_expanded,
        window,
        cx,
        |app, id, cx| app.update_config(cx, |c| c.overlay.icon_pair = id.to_string()),
        |app, cx| {
            app.sections.overlay.icons_expanded = !app.sections.overlay.icons_expanded;
            cx.notify();
        },
    );
    let mut fonts = app
        .snapshot
        .system_fonts
        .iter()
        .map(|f| SelectOption::new(f.family.clone(), f.family.clone()))
        .collect::<Vec<_>>();
    if !o.text_font.is_empty()
        && !fonts
            .iter()
            .any(|f| f.value.eq_ignore_ascii_case(&o.text_font))
    {
        fonts.insert(
            0,
            SelectOption::new(o.text_font.clone(), o.text_font.clone()),
        );
    }
    // Hovering a font tries it on the live overlay; rendering every row in
    // its own font froze the window with a few hundred installed families.
    let mut font_options = SelectOptions::new(fonts).on_preview(|app, value, _, cx| {
        app.update_config(cx, |c| c.overlay.text_font = value.to_string())
    });
    font_options.searchable = true;
    let font_content = div()
        .flex()
        .flex_wrap()
        .items_end()
        .gap(px(14.))
        .child(
            choose(
                "overlay-font",
                "Font",
                o.text_font.clone(),
                font_options,
                app,
                window,
                cx,
                |o, v| o.text_font = v,
            )
            .flex_basis(px(280.))
            .flex_grow(1.),
        )
        .child(
            div()
                .flex_basis(px(220.))
                .flex_grow(1.)
                .min_w_0()
                .child(slider(
                    "overlay-weight",
                    "Font weight",
                    o.text_font_weight as f64,
                    100.,
                    900.,
                    100.,
                    "",
                    window,
                    cx,
                    |o, v| o.text_font_weight = v as u16,
                )),
        );
    let mut icon_style = options(&[
        ("Colored", "Colored", "pallete-2-linear"),
        ("Monochrome", "Monochrome", "ic-baseline-contrast"),
        ("SystemColor", "System color", "widget-linear"),
    ]);
    icon_style.disabled = !has_icon;
    // The font block comes first, so its gap goes below it.
    let custom_content = div()
        .flex()
        .flex_col()
        .gap(px(CONTROL_GAP))
        .child(
            form::group()
                .child(form::collapse(
                    "overlay-font-collapse",
                    has_text,
                    div().pb(px(CONTROL_GAP)).child(font_content),
                    window,
                    cx,
                ))
                .child(
            div()
                .flex()
                .gap(px(14.))
                .child(
                    choose(
                        "overlay-icon-style",
                        "Icon style",
                        o.icon_style.clone(),
                        icon_style,
                        app,
                        window,
                        cx,
                        |o, v| o.icon_style = v,
                    )
                    .flex_1(),
                )
                .child(
                    choose(
                        "overlay-background",
                        "Background",
                        o.background_style.clone(),
                        options(&[
                            ("Dark", "Dark", "moon-linear"),
                            ("Light", "Light", "sun-2-linear"),
                        ]),
                        app,
                        window,
                        cx,
                        |o, v| o.background_style = v,
                    )
                    .flex_1(),
                ),
                ),
        )
        .child(slider(
            "overlay-bg-opacity",
            "Background opacity",
            o.background_opacity as f64,
            0.,
            100.,
            5.,
            "%",
            window,
            cx,
            |o, v| o.background_opacity = v as u8,
        ))
        .child(form::setting_row(
            "Show border",
            None,
            controls::toggle("overlay-border", o.show_border, window, cx, |app, v, _, cx| {
                app.update_config(cx, |c| c.overlay.show_border = v)
            }),
            cx,
        ))
        .child(slider(
            "overlay-opacity",
            "Opacity",
            o.content_opacity as f64,
            20.,
            100.,
            5.,
            "%",
            window,
            cx,
            |o, v| o.content_opacity = v as u8,
        ));
    let radius = slider(
        "overlay-radius",
        "Border radius",
        crate::gpui_overlay::theme::corner_radius(&o) as f64,
        0.,
        32.,
        1.,
        "px",
        window,
        cx,
        |o, v| {
            o.corner_radii.insert(o.theme.clone(), v as u8);
        },
    );
    let appearance = panel(CONTROL_GAP, false, cx)
        .mt(px(BLOCK_GAP))
        .opacity(dim)
        .child(form::section_heading("Appearance"))
        .child(super::themes::theme_link(
            "overlay-theme",
            super::themes::Target::Microphone,
            &o.theme,
            window,
            cx,
        ))
        .child(field("Overlay style", variants))
        .child(labels)
        .child(
            form::group()
                .child(icons)
                .child(collapse_in_gap(
                    "overlay-custom-collapse",
                    custom,
                    CONTROL_GAP,
                    custom_content,
                    window,
                    cx,
                ))
                .child(collapse_in_gap(
                    "overlay-radius-collapse",
                    crate::gpui_overlay::theme::supports_corner_radius(&o.theme),
                    CONTROL_GAP,
                    div().child(radius),
                    window,
                    cx,
                )),
        )
        .child(
            slider(
                "overlay-scale",
                "Size scale",
                o.scale as f64,
                10.,
                400.,
                5.,
                "%",
                window,
                cx,
                |o, v| o.scale = v as u32,
            ),
        );
    let behaviour = behaviour(app, window, cx);
    // Exactly one block for each SettingsTab::Overlay section, in sidebar order.
    vec![
        overview.into_any_element(),
        appearance.into_any_element(),
        behaviour.into_any_element(),
    ]
}

// Binding semantics mirror the original Overlay action editor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OverlayActionSlot {
    SingleClick,
    DoubleClick,
    MiddleClick,
    RightClick,
    WheelUp,
    WheelDown,
}

impl OverlayActionSlot {
    fn id(self) -> &'static str {
        match self {
            Self::SingleClick => "single-click",
            Self::DoubleClick => "double-click",
            Self::MiddleClick => "middle-click",
            Self::RightClick => "right-click",
            Self::WheelUp => "wheel-up",
            Self::WheelDown => "wheel-down",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SingleClick => "Single click",
            Self::DoubleClick => "Double click",
            Self::MiddleClick => "Middle click",
            Self::RightClick => "Right click",
            Self::WheelUp => "Mouse wheel up",
            Self::WheelDown => "Mouse wheel down",
        }
    }
}

#[derive(Clone, PartialEq)]
struct OverlayActionEntry {
    slot: OverlayActionSlot,
    binding: crate::OverlayActionBinding,
}

const OVERLAY_ACTION_SLOTS: &[OverlayActionSlot] = &[
    OverlayActionSlot::SingleClick,
    OverlayActionSlot::DoubleClick,
    OverlayActionSlot::MiddleClick,
    OverlayActionSlot::RightClick,
    OverlayActionSlot::WheelUp,
    OverlayActionSlot::WheelDown,
];

fn overlay_action_entries(overlay: &crate::OverlayConfig) -> Vec<OverlayActionEntry> {
    OVERLAY_ACTION_SLOTS
        .iter()
        .copied()
        .filter_map(|slot| {
            let binding = overlay_slot_binding(overlay, slot);
            binding.action.map(|_| OverlayActionEntry { slot, binding })
        })
        .collect()
}

fn first_empty_overlay_action_slot(overlay: &crate::OverlayConfig) -> OverlayActionSlot {
    OVERLAY_ACTION_SLOTS
        .iter()
        .copied()
        .find(|slot| overlay_slot_binding(overlay, *slot).action.is_none())
        .unwrap_or(OverlayActionSlot::SingleClick)
}

fn overlay_slot_binding(
    overlay: &crate::OverlayConfig,
    slot: OverlayActionSlot,
) -> crate::OverlayActionBinding {
    match slot {
        OverlayActionSlot::SingleClick => overlay.single_click.clone(),
        OverlayActionSlot::DoubleClick => overlay.double_click.clone(),
        OverlayActionSlot::MiddleClick => overlay.middle_click.clone(),
        OverlayActionSlot::RightClick => overlay.right_click.clone(),
        OverlayActionSlot::WheelUp => overlay.wheel_up.clone(),
        OverlayActionSlot::WheelDown => overlay.wheel_down.clone(),
    }
}

fn set_overlay_slot_binding(
    overlay: &mut crate::OverlayConfig,
    slot: OverlayActionSlot,
    binding: crate::OverlayActionBinding,
) {
    match slot {
        OverlayActionSlot::SingleClick => overlay.single_click = binding,
        OverlayActionSlot::DoubleClick => overlay.double_click = binding,
        OverlayActionSlot::MiddleClick => overlay.middle_click = binding,
        OverlayActionSlot::RightClick => overlay.right_click = binding,
        OverlayActionSlot::WheelUp => overlay.wheel_up = binding,
        OverlayActionSlot::WheelDown => overlay.wheel_down = binding,
    }
}

fn overlay_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    target_2: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> String {
    if action.needs_second_target() {
        let first =
            overlay_single_target_label(action, target, devices, output_devices, name_display);
        let second =
            overlay_single_target_label(action, target_2, devices, output_devices, name_display);
        return format!("{first} / {second}");
    }

    overlay_single_target_label(action, target, devices, output_devices, name_display)
}

fn overlay_single_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
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
            .and_then(|target| devices.iter().find(|device| device.id == target))
            .map(|device| device.display_name(name_display))
            .unwrap_or_else(|| "Choose an input device".to_string());
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultOutputDevice
            | crate::HotkeyAction::ToggleDefaultOutputDevice
    ) {
        return target
            .and_then(|target| output_devices.iter().find(|device| device.id == target))
            .map(|device| device.display_name(name_display))
            .unwrap_or_else(|| "Choose an output device".to_string());
    }

    if matches!(target, Some(crate::HOTKEY_TARGET_ALL_MICROPHONES)) {
        return "All microphones".to_string();
    }

    target
        .filter(|target| !target.is_empty())
        .and_then(|target| devices.iter().find(|device| device.id == target))
        .map(|device| device.display_name(name_display))
        .unwrap_or_else(|| "Default".to_string())
}

fn overlay_binding_for_action(
    mut binding: crate::OverlayActionBinding,
    action: Option<crate::HotkeyAction>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    hotkeys: &[crate::HotkeyBinding],
) -> crate::OverlayActionBinding {
    let previous_action = binding.action;
    binding.action = action;
    let Some(action) = action else {
        binding.target = None;
        binding.target_2 = None;
        return binding;
    };

    let mut target = binding.target.clone().unwrap_or_default();
    if action_uses_volume_target(action) && previous_action != Some(action) {
        target = default_target_for_action(action, devices, output_devices, hotkeys);
    } else if !action.needs_target()
        || !target_is_valid_for_action(action, &target, devices, output_devices)
    {
        target = default_target_for_action(action, devices, output_devices, hotkeys);
    }
    binding.target = action.needs_target().then_some(target.clone());

    let mut target_2 = binding.target_2.clone().unwrap_or_default();
    if !action.needs_second_target()
        || !target_is_valid_for_action(action, &target_2, devices, output_devices)
    {
        target_2 = default_second_target_for_action(action, &target, devices, output_devices);
    }
    binding.target_2 = action.needs_second_target().then_some(target_2);
    binding
}

pub(crate) fn action_options() -> Vec<SelectOption> {
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

pub(crate) fn target_options(
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
                SelectOption::new("", "Default")
                    .detail(default_detail)
                    .icon("microphone-3-linear"),
            )
            .chain(std::iter::once(
                SelectOption::new(crate::HOTKEY_TARGET_ALL_MICROPHONES, "All microphones")
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

pub(crate) fn target_is_valid_for_action(
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

pub(crate) fn default_target_for_action(
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

pub(crate) fn action_uses_volume_target(action: crate::HotkeyAction) -> bool {
    matches!(
        action,
        crate::HotkeyAction::SetVolume
            | crate::HotkeyAction::IncreaseVolume
            | crate::HotkeyAction::DecreaseVolume
    )
}

pub(crate) fn volume_target_value(target: &str, action: crate::HotkeyAction) -> u8 {
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

pub(crate) fn default_second_target_for_action(
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

fn open_editor(app: &mut SettingsApp, slot: OverlayActionSlot, cx: &mut Context<SettingsApp>) {
    let s = &mut app.sections.overlay;
    s.editor = Some(slot);
    s.editor_closing = None;
    s.picker = false;
    s.picker_closing = None;
    s.focus_pending = true;
    cx.notify();
}
fn close_editor(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.sections.overlay.editor_closing.is_some() {
        return;
    }
    app.open_select = None;
    app.sections.overlay.editor_closing = Some(Instant::now());
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
        let _ = this.update(cx, |app, cx| {
            app.sections.overlay.editor = None;
            app.sections.overlay.editor_closing = None;
            app.sections.overlay.picker = false;
            app.sections.overlay.picker_closing = None;
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}
fn close_picker(app: &mut SettingsApp, cx: &mut Context<SettingsApp>) {
    if app.sections.overlay.picker_closing.is_some() {
        return;
    }
    app.sections.overlay.picker_closing = Some(Instant::now());
    cx.spawn(async move |this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(180))
            .await;
        let _ = this.update(cx, |app, cx| {
            app.sections.overlay.picker = false;
            app.sections.overlay.picker_closing = None;
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}
fn behaviour(app: &mut SettingsApp, window: &mut Window, cx: &mut Context<SettingsApp>) -> Div {
    let o = app.snapshot.config.overlay.clone();
    let p = app.palette;
    let pass_through = o.behaviour == "PassThrough";
    let dim = form::governed_opacity("overlay-governed", o.enabled, window, cx);
    let mut rows = div().flex().flex_col().gap(px(10.));
    let entries = overlay_action_entries(&o);
    let action_count = entries.len();
    if entries.is_empty() {
        rows = rows.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .py(px(32.))
                .border_1()
                .border_dashed()
                .border_color(p.border.hsla())
                .rounded(px(12.))
                .child(
                    svg()
                        .path(icon("widget-linear"))
                        .size(px(24.))
                        .text_color(p.faint.hsla()),
                )
                .child(style::p("No overlay actions configured.", cx)),
        );
    }
    for entry in entries {
        let slot = entry.slot;
        let action = entry.binding.action.unwrap();
        let (pointer, hover, _) = controls::hover_tween(
            SharedString::from(format!("overlay-action-{}", slot.id())),
            window,
            cx,
        );
        let target = overlay_target_label(
            action,
            entry.binding.target.as_deref(),
            entry.binding.target_2.as_deref(),
            &app.snapshot.devices,
            &app.snapshot.output_devices,
            &app.snapshot.config.advanced.audio_device_name_display,
        );
        let mut actions = div().flex().opacity(0.4 + 0.6 * hover);
        for (remove, ic) in [(false, "pen-linear"), (true, "trash-bin-trash-linear")] {
            let id = SharedString::from(format!(
                "overlay-{}-{}",
                slot.id(),
                if remove { "remove" } else { "edit" }
            ));
            let (_, h, _) = controls::hover_tween(id.clone(), window, cx);
            actions = actions.child(
                controls::button(
                    id,
                    ButtonKind::Icon,
                    Some(ic),
                    None,
                    false,
                    window,
                    cx,
                    move |app, _, _, cx| {
                        if remove {
                            app.update_config(cx, |c| {
                                set_overlay_slot_binding(
                                    &mut c.overlay,
                                    slot,
                                    crate::OverlayActionBinding::default(),
                                )
                            });
                        } else {
                            open_editor(app, slot, cx);
                        }
                    },
                )
                .size(px(36.))
                .bg(lerp(TRANSPARENT, p.surface_3, h).hsla())
                .shadow(Vec::<BoxShadow>::new())
                .text_color(lerp(p.muted, if remove { p.danger } else { p.text }, h).hsla())
                .tooltip(move |_, cx| {
                    controls::tooltip(
                        if remove {
                            "Remove overlay action"
                        } else {
                            "Edit overlay action"
                        },
                        cx,
                    )
                }),
            );
        }
        rows = rows.child(controls::track_pointer(
            div()
                .id(SharedString::from(format!("overlay-action-{}", slot.id())))
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(20.))
                .py(px(16.))
                .rounded(px(12.))
                .bg(p.surface_2.hsla())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(style::label(action.label()).text_ellipsis())
                        .when(action.needs_target(), |d| {
                            d.child(
                                div()
                                    .text_size(px(13.))
                                    .text_color(p.muted.hsla())
                                    .text_ellipsis()
                                    .child(target),
                            )
                        }),
                )
                .child(
                    div()
                        .min_w(px(132.))
                        .text_size(px(13.))
                        .font_weight(FontWeight(650.))
                        .text_color(p.muted.hsla())
                        .child(slot.label()),
                )
                .child(actions),
            pointer,
        ));
    }
    let (add_pointer, add_hover, _) = controls::hover_tween("overlay-add-action", window, cx);
    let add_bg = lerp(p.surface_2, p.surface_hover, add_hover);
    let add = controls::track_pointer(
        div()
            .id("overlay-add-action")
            .h(px(38.))
            .px(px(16.))
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(8.))
            .rounded(px(8.))
            .cursor_pointer()
            .bg(add_bg.hsla())
            .text_size(px(14.))
            .font_weight(FontWeight(500.))
            .text_color(p.text.hsla())
            .shadow(vec![BoxShadow {
                color: p.shadow_soft.hsla(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(1.),
                spread_radius: px(0.),
                inset: false,
            }])
            .child(
                svg()
                    .path(icon("plus-linear"))
                    .size(px(17.))
                    .text_color(p.text.hsla()),
            )
            .child("Add action"),
        add_pointer,
    )
    .on_click(cx.listener(|app, _, _, cx| {
        let slot = first_empty_overlay_action_slot(&app.snapshot.config.overlay);
        open_editor(app, slot, cx);
    }));
    // Click actions do nothing while clicks pass through, so the list folds
    // away and one line says why. The actions themselves are kept.
    let hint_text = match action_count {
        0 => "Clicks go through the overlay to the window underneath.".to_string(),
        1 => "Clicks go through the overlay to the window underneath. 1 click action is kept and comes back when you switch to Button.".to_string(),
        count => format!(
            "Clicks go through the overlay to the window underneath. {count} click actions are kept and come back when you switch to Button."
        ),
    };
    let hint = div()
        .flex()
        .items_start()
        .gap(px(10.))
        .child(
            svg()
                .path(icon("info-circle-linear"))
                .size(px(16.))
                .mt(px(1.))
                .flex_shrink_0()
                .text_color(p.faint.hsla()),
        )
        .child(
            style::desc(hint_text, cx)
                .line_height(relative(1.45))
                .flex_1()
                .min_w_0(),
        );
    let actions = div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(rows)
        .child(div().flex().child(add));
    panel(CONTROL_GAP, true, cx)
        .mt(px(BLOCK_GAP))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_shrink_0()
                .gap(px(CONTROL_GAP))
                .opacity(dim)
                .child(form::section_heading("Behaviour"))
                .child(form::group().child(field(
                    "Click mode",
                    controls::segmented_toggle(
                        "overlay-click-mode",
                        o.behaviour.clone(),
                        vec![
                            controls::SegmentedToggleOption::new("PassThrough", "Pass-through"),
                            controls::SegmentedToggleOption::new("Button", "Button"),
                        ],
                        window,
                        cx,
                        |app, v, _, cx| {
                            app.update_config(cx, |c| c.overlay.behaviour = v.to_string())
                        },
                    ),
                ))
                .child(collapse_in_gap(
                    "overlay-pass-hint",
                    pass_through,
                    CONTROL_GAP,
                    hint,
                    window,
                    cx,
                ))
                .child(collapse_in_gap(
                    "overlay-action-list",
                    !pass_through,
                    CONTROL_GAP,
                    actions,
                    window,
                    cx,
                ))),
        )
        .children(editor(app, window, cx))
}
fn editor(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    let slot = app.sections.overlay.editor?;
    let p = app.palette;
    let closing = app.sections.overlay.editor_closing.is_some();
    let t = anim::tween_from(
        window,
        cx,
        "overlay-editor-slide",
        0.,
        if closing { 0. } else { 1. },
        Transition::new(
            if closing { 180 } else { 220 },
            if closing {
                Easing::Bezier(0.4, 0., 1., 1.)
            } else {
                Easing::Bezier(0.22, 1., 0.36, 1.)
            },
        ),
    );
    let scrim = anim::tween_from(
        window,
        cx,
        "overlay-editor-scrim",
        0.,
        if closing { 0. } else { 1. },
        Transition::new(if closing { 180 } else { 160 }, Easing::Ease),
    );
    let focus = window
        .use_keyed_state("overlay-editor-focus", cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    if app.sections.overlay.focus_pending {
        focus.focus(window, cx);
        app.sections.overlay.focus_pending = false;
    }
    let binding = overlay_slot_binding(&app.snapshot.config.overlay, slot);
    let gestures = SelectOptions::new(
        OVERLAY_ACTION_SLOTS
            .iter()
            .map(|s| SelectOption::new(s.id(), s.label()))
            .collect(),
    );
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(40.))
        .pb(px(32.))
        .child(field(
            "Gesture",
            controls::select(
                "overlay-gesture",
                slot.id(),
                gestures,
                app,
                window,
                cx,
                |app, v, _, cx| {
                    if let Some(slot) = OVERLAY_ACTION_SLOTS.iter().find(|s| s.id() == v.as_ref()) {
                        app.sections.overlay.editor = Some(*slot);
                        cx.notify();
                    }
                },
            ),
        ));
    let current = action_options()
        .into_iter()
        .find(|v| Some(crate::HotkeyAction::from_id(v.value.as_ref())) == binding.action)
        .unwrap_or_else(|| SelectOption::new("", "Empty").icon("codicon_close"));
    let trigger = controls::button(
        "overlay-action-trigger",
        ButtonKind::Secondary,
        None,
        None,
        false,
        window,
        cx,
        |app, _, _, cx| {
            app.sections.overlay.picker = true;
            app.sections.overlay.picker_closing = None;
            cx.notify();
        },
    )
    .h(px(48.))
    .w_full()
    .rounded(px(12.))
    .pl(px(14.))
    .pr(px(12.))
    .child(
        div()
            .flex()
            .items_center()
            .gap(px(12.))
            .children(current.icon.as_ref().map(|i| {
                svg()
                    .path(icon(i))
                    .size(px(18.))
                    .text_color(p.accent_text.hsla())
            }))
            .child(current.label),
    )
    .justify_between()
    .child(
        svg()
            .path(icon("alt-arrow-down-linear"))
            .size(px(17.))
            .flex_shrink_0()
            .text_color(p.faint.hsla()),
    );
    body = body.child(field("Action", trigger));
    if let Some(action) = binding.action {
        if action_uses_volume_target(action) {
            let value = volume_target_value(binding.target.as_deref().unwrap_or_default(), action);
            body = body.child(slider(
                "overlay-action-volume",
                if action == crate::HotkeyAction::SetVolume {
                    "Volume"
                } else {
                    "Amount"
                },
                value as f64,
                0.,
                100.,
                1.,
                "%",
                window,
                cx,
                move |o, v| {
                    let mut b = overlay_slot_binding(o, slot);
                    b.target = Some((v as u8).to_string());
                    set_overlay_slot_binding(o, slot, b);
                },
            ));
        } else if action.needs_target() {
            for second in [false, true] {
                if second && !action.needs_second_target() {
                    continue;
                }
                let opts = target_options(
                    action,
                    app.snapshot.devices.clone(),
                    app.snapshot.output_devices.clone(),
                    &app.snapshot.config.advanced.audio_device_name_display,
                );
                let value = if second {
                    binding.target_2.clone()
                } else {
                    binding.target.clone()
                }
                .unwrap_or_default();
                body = body.child(field(
                    if second { "Target 2" } else { "Target 1" },
                    controls::select(
                        if second {
                            "overlay-action-target-2"
                        } else {
                            "overlay-action-target"
                        },
                        value,
                        SelectOptions::new(opts),
                        app,
                        window,
                        cx,
                        move |app, v, _, cx| {
                            let devices = app.snapshot.devices.clone();
                            let outputs = app.snapshot.output_devices.clone();
                            app.update_config(cx, |c| {
                                let mut b = overlay_slot_binding(&c.overlay, slot);
                                if second {
                                    b.target_2 = Some(v.to_string());
                                } else {
                                    b.target = Some(v.to_string());
                                    if action.needs_second_target() {
                                        let old = b.target_2.as_deref().unwrap_or_default();
                                        if old == v.as_ref()
                                            || !target_is_valid_for_action(
                                                action, old, &devices, &outputs,
                                            )
                                        {
                                            b.target_2 = Some(default_second_target_for_action(
                                                action,
                                                v.as_ref(),
                                                &devices,
                                                &outputs,
                                            ));
                                        }
                                    }
                                }
                                set_overlay_slot_binding(&mut c.overlay, slot, b);
                            });
                        },
                    ),
                ));
            }
        }
    }
    let viewport = window.viewport_size();
    let panel = div()
        .id("overlay-editor")
        .track_focus(&focus)
        .w(px(480.))
        .h_full()
        .flex_shrink_0()
        .relative()
        .left(px(480. * (1. - t)))
        .rounded_tl(px(24.))
        .rounded_bl(px(24.))
        .bg(p.hotkey_panel_bg.hsla())
        .pt(px(32.))
        .flex()
        .flex_col()
        .gap(px(24.))
        .shadow(vec![BoxShadow {
            color: p.shadow_medium.hsla(),
            offset: point(px(-18.), px(0.)),
            blur_radius: px(44.),
            spread_radius: px(0.),
            inset: false,
        }])
        .on_click(|_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(|app, e: &KeyDownEvent, _, cx| {
            if e.keystroke.key == "escape" {
                if app.sections.overlay.picker {
                    close_picker(app, cx);
                } else {
                    close_editor(app, cx);
                }
                cx.stop_propagation();
            }
        }))
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
                            style::h2(if binding.action.is_some() {
                                "Edit overlay action"
                            } else {
                                "Add overlay action"
                            })
                            .text_size(px(23.)),
                        )
                        .child(style::p("Changes apply immediately.", cx).text_size(px(13.))),
                )
                .child(controls::button(
                    "overlay-editor-close",
                    ButtonKind::Icon,
                    Some("codicon_close"),
                    None,
                    false,
                    window,
                    cx,
                    |app, _, _, cx| close_editor(app, cx),
                )),
        )
        .child(
            div()
                .id("overlay-editor-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .child(body),
        );
    let backdrop = div()
        .id("overlay-editor-backdrop")
        .occlude()
        .w(viewport.width)
        .h(viewport.height)
        .flex()
        .justify_end()
        .bg(gray(0., 60. * scrim).hsla())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _, cx| close_editor(app, cx)))
        .child(panel);
    Some(
        deferred(
            anchored()
                .position(point(px(0.), px(0.)))
                .child(backdrop)
                .children(picker(app, slot, &binding, window, cx)),
        )
        .with_priority(10)
        .into_any_element(),
    )
}
fn picker(
    app: &mut SettingsApp,
    slot: OverlayActionSlot,
    binding: &crate::OverlayActionBinding,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Option<AnyElement> {
    if !app.sections.overlay.picker {
        return None;
    }
    let p = app.palette;
    let closing = app.sections.overlay.picker_closing.is_some();
    let t = anim::tween_from(
        window,
        cx,
        "overlay-picker-slide",
        0.,
        if closing { 0. } else { 1. },
        Transition::new(
            if closing { 180 } else { 220 },
            if closing {
                Easing::Bezier(0.4, 0., 1., 1.)
            } else {
                Easing::Bezier(0.22, 1., 0.36, 1.)
            },
        ),
    );
    let viewport = window.viewport_size();
    let width = viewport.width.as_f32() * 0.7;
    let opts = std::iter::once(
        SelectOption::new("", "Empty")
            .group("Other")
            .icon("codicon_close"),
    )
    .chain(action_options())
    .collect::<Vec<_>>();
    let mut groups = div()
        .flex()
        .flex_col()
        .gap(px(18.))
        .pt(px(32.))
        .pl(px(32.))
        .pr(px(24.))
        .pb(px(32.));
    for name in [
        "Other",
        "Mute",
        "Hold to mute",
        "Devices",
        "Volume",
        "System",
    ] {
        let group = opts
            .iter()
            .filter(|o| o.group_label.as_deref() == Some(name))
            .collect::<Vec<_>>();
        let mut cards = div().flex().flex_col().gap(px(10.));
        for chunk in group.chunks(2) {
            let mut row = div().flex().gap(px(10.));
            for option in chunk {
                let id = SharedString::from(format!("overlay-pick-{}", option.value));
                let value = option.value.clone();
                let selected = if value.is_empty() {
                    binding.action.is_none()
                } else {
                    binding.action == Some(crate::HotkeyAction::from_id(value.as_ref()))
                };
                let (pointer, h, _) = controls::hover_tween(id.clone(), window, cx);
                let active = anim::tween(
                    window,
                    cx,
                    (ElementId::from(id.clone()), "selected"),
                    selected as u8 as f32,
                    anim::UI,
                );
                row = row.child(
                    controls::track_pointer(
                        div()
                            .id(id)
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(10.))
                            .px(px(16.))
                            .py(px(12.))
                            .rounded(px(12.))
                            .border_1()
                            .border_color(TRANSPARENT.hsla())
                            .bg(mix_pct(
                                p.accent,
                                6. + 14. * h + (24. - 14. * h) * active,
                                p.surface_2,
                            )
                            .hsla())
                            .relative()
                            .top(px(-h.max(active)))
                            .cursor_pointer()
                            .shadow(vec![BoxShadow {
                                color: p.shadow_faint.opacity(h.max(active)).hsla(),
                                offset: point(px(0.), px(8.)),
                                blur_radius: px(20.),
                                spread_radius: px(0.),
                                inset: false,
                            }])
                            .children(option.icon.as_ref().map(|i| {
                                svg()
                                    .path(icon(i))
                                    .size(px(22.))
                                    .flex_shrink_0()
                                    .text_color(p.accent_text.hsla())
                            }))
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
                        let action = if value.is_empty() {
                            None
                        } else {
                            Some(crate::HotkeyAction::from_id(value.as_ref()))
                        };
                        let next = overlay_binding_for_action(
                            overlay_slot_binding(&app.snapshot.config.overlay, slot),
                            action,
                            &app.snapshot.devices,
                            &app.snapshot.output_devices,
                            &app.snapshot.config.hotkeys,
                        );
                        app.update_config(cx, |c| {
                            set_overlay_slot_binding(&mut c.overlay, slot, next)
                        });
                        close_picker(app, cx);
                    })),
                );
            }
            if chunk.len() == 1 {
                row = row.child(div().flex_1());
            }
            cards = cards.child(row);
        }
        groups = groups.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight(700.))
                        .text_color(p.muted.hsla())
                        .child(name.to_uppercase()),
                )
                .child(cards),
        );
    }
    Some(
        div()
            .id("overlay-picker-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .w(viewport.width)
            .h(viewport.height)
            .flex()
            .justify_end()
            .bg(gray(0., 34. * t).hsla())
            .on_click(cx.listener(|app, _, _, cx| close_picker(app, cx)))
            .child(
                div()
                    .id("overlay-picker-panel")
                    .w(px(width))
                    .h_full()
                    .relative()
                    .left(px(width * (1. - t)))
                    .rounded_tl(px(24.))
                    .rounded_bl(px(24.))
                    .bg(p.hotkey_panel_bg.hsla())
                    .overflow_y_scroll()
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
            .into_any_element(),
    )
}
