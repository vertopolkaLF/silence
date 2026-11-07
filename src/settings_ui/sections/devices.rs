//! Devices tab, matching the Dioxus implementation and `.device-*` in general.css.
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition, UI},
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{self, SelectOption, SelectOptions, TextInput, TextInputEvent, TextInputOptions},
    style::{self, palette},
};
use gpui::{prelude::*, *};
use std::{collections::HashMap, rc::Rc};

#[derive(Default)]
pub(crate) struct State {
    rows: HashMap<String, Rename>,
}
#[derive(Default)]
struct Rename {
    editing: bool,
    draft: String,
    input: Option<Entity<TextInput>>,
    focus_pending: bool,
    generation: u64,
}
#[derive(Clone)]
struct DeviceRow {
    id: String,
    name: String,
    pretty_name: String,
    default: bool,
    input: bool,
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let mode = app
        .snapshot
        .config
        .advanced
        .audio_device_name_display
        .clone();
    let rename_allowed = mode != crate::AUDIO_DEVICE_NAME_SYSTEM;
    let output = app
        .snapshot
        .output_devices
        .iter()
        .map(|d| DeviceRow {
            id: d.id.clone(),
            name: d.display_name(&mode),
            pretty_name: d.name.clone(),
            default: d.is_default,
            input: false,
        })
        .collect::<Vec<_>>();
    let input = app
        .snapshot
        .devices
        .iter()
        .map(|d| DeviceRow {
            id: d.id.clone(),
            name: d.display_name(&mode),
            pretty_name: d.name.clone(),
            default: d.is_default,
            input: true,
        })
        .collect::<Vec<_>>();
    // Unplugged rows lose their editor, just as keyed Dioxus rows unmount.
    app.sections
        .devices
        .rows
        .retain(|id, _| output.iter().chain(&input).any(|d| &d.id == id));
    let lists = div()
        .flex()
        .flex_col()
        .gap(px(20.))
        .pb(px(18.))
        .border_b_1()
        .border_color(app.palette.border.hsla())
        .child(device_list(
            "Output",
            "No active output devices found",
            output,
            rename_allowed,
            app,
            window,
            cx,
        ))
        .child(device_list(
            "Input",
            "No active input devices found",
            input,
            rename_allowed,
            app,
            window,
            cx,
        ));
    let mut options = SelectOptions::new(vec![
        SelectOption::new(crate::AUDIO_DEVICE_NAME_PRETTY, "Pretty Name")
            .icon("microphone-3-linear"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_SYSTEM, "System Name").icon("widget-linear"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_BOTH, "Both").icon("volume-loud-linear"),
    ]);
    options.show_current_detail = false;
    let select = controls::select(
        "devices-name-display",
        mode,
        options,
        app,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| {
                c.advanced.audio_device_name_display = value.to_string()
            })
        },
    );
    let toggle = controls::toggle(
        "devices-ungroup",
        app.snapshot.config.advanced.ungroup_tray_devices,
        window,
        cx,
        |app, value, _, cx| app.update_config(cx, |c| c.advanced.ungroup_tray_devices = value),
    );
    vec![
        div()
            .flex()
            .flex_col()
            .gap(px(18.))
            .max_w(px(720.))
            .child(super::general::header("Devices", None, cx))
            .child(lists)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(20.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(16.))
                            .child(style::h2("Displayed audio device name"))
                            .child(select),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(6.))
                            .min_h(px(44.))
                            .child(style::h2("Ungroup devices in tray menu"))
                            .child(toggle),
                    ),
            )
            .into_any_element(),
    ]
}

fn device_list(
    title: &'static str,
    empty: &'static str,
    rows: Vec<DeviceRow>,
    rename_allowed: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = app.palette;
    let body = if rows.is_empty() {
        div()
            .min_h(px(44.))
            .flex()
            .items_center()
            .px(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_dashed()
            .border_color(p.border_strong.hsla())
            .bg(mix_pct(p.surface_2, 76., TRANSPARENT).hsla())
            .text_color(p.muted.hsla())
            .child(empty)
    } else {
        div().flex().flex_col().gap(px(8.)).children(
            rows.into_iter()
                .map(|row| device_row(row, rename_allowed, app, window, cx))
                .collect::<Vec<_>>(),
        )
    };
    div()
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(style::h2(title))
        .child(body)
}

fn finish_rename(
    app: &mut SettingsApp,
    row: &DeviceRow,
    commit: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) {
    let Some(state) = app.sections.devices.rows.get_mut(&row.id) else {
        return;
    };
    if !state.editing {
        return;
    }
    let next = state.draft.trim().to_string();
    state.editing = false;
    state.focus_pending = false;
    state.draft = row.pretty_name.clone();
    state.input = None;
    // Enter is emitted while the input entity is borrowed; never update it here.
    if commit && !next.is_empty() && next != row.pretty_name {
        let _ = crate::rename_audio_device(&row.id, &next);
        app.refresh_devices(cx);
    }
    app.focus_handle.focus(window, cx);
    cx.notify();
}

fn device_row(
    row: DeviceRow,
    rename_allowed: bool,
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let p = app.palette;
    let key = ElementId::from(SharedString::from(format!("device-row-{}", row.id)));
    let state = app.sections.devices.rows.entry(row.id.clone()).or_default();
    let editing = state.editing;
    let draft = state.draft.clone();
    let generation = state.generation;
    let focus_pending = std::mem::take(&mut state.focus_pending);
    let active = anim::tween(
        window,
        cx,
        (key.clone(), "active"),
        row.default as u8 as f32,
        UI,
    );
    let edit = anim::tween(
        window,
        cx,
        (key.clone(), "editing"),
        editing as u8 as f32,
        UI,
    );
    let bg = lerp(
        lerp(p.surface_2, p.surface_color[2], active),
        p.surface_2,
        edit,
    );
    let name = if editing {
        let event_row = row.clone();
        let input = controls::text_input(
            (
                key.clone(),
                SharedString::from(format!("rename-input-{generation}")),
            ),
            draft.clone(),
            TextInputOptions {
                caret_color: Some(p.accent.hsla()),
                ..Default::default()
            },
            window,
            cx,
            move |app, event, window, cx| {
                if !app
                    .sections
                    .devices
                    .rows
                    .get(&event_row.id)
                    .is_some_and(|state| state.editing && state.generation == generation)
                {
                    return;
                }
                match event {
                    TextInputEvent::Change(value) => {
                        if let Some(state) = app.sections.devices.rows.get_mut(&event_row.id) {
                            state.draft = value.to_string();
                        }
                        cx.notify();
                    }
                    TextInputEvent::Blur(value) if value.trim().is_empty() => {
                        if let Some(state) = app.sections.devices.rows.get_mut(&event_row.id) {
                            state.draft = event_row.pretty_name.clone();
                        }
                        // Defer to avoid borrowing the input again inside its blur callback.
                        let id = event_row.id.clone();
                        let old = event_row.pretty_name.clone();
                        cx.defer_in(window, move |app, _, cx| {
                            if let Some(state) = app.sections.devices.rows.get(&id)
                                && state.editing
                                && state.generation == generation
                                && let Some(input) = &state.input
                            {
                                input.update(cx, |input, cx| input.set_value(old, cx));
                            }
                        });
                        cx.notify();
                    }
                    TextInputEvent::Enter(_) => finish_rename(app, &event_row, true, window, cx),
                    TextInputEvent::Escape => finish_rename(app, &event_row, false, window, cx),
                    _ => {}
                }
            },
        );
        app.sections.devices.rows.get_mut(&row.id).unwrap().input = Some(input.clone());
        if focus_pending {
            let id = row.id.clone();
            let focus_input = input.clone();
            cx.defer_in(window, move |app, window, cx| {
                if app
                    .sections
                    .devices
                    .rows
                    .get(&id)
                    .is_some_and(|s| s.editing && s.generation == generation)
                {
                    focus_input.update(cx, |input, cx| input.focus_at_end(window, cx));
                }
            });
        }
        let width = anim::tween(
            window,
            cx,
            (key.clone(), "rename-width"),
            (draft.chars().count() as f32 * 0.54 + 0.65).clamp(1.8, 20.) * 14.,
            UI,
        );
        let commit_row = row.clone();
        let cancel_row = row.clone();
        let commit = action(
            (
                key.clone(),
                SharedString::from(format!("commit-{generation}")),
            ),
            "check-mark",
            "Confirm rename",
            ActionKind::Commit,
            false,
            window,
            cx,
            move |app, window, cx| finish_rename(app, &commit_row, true, window, cx),
        );
        let cancel = action(
            (
                key.clone(),
                SharedString::from(format!("cancel-{generation}")),
            ),
            "codicon_close",
            "Cancel rename",
            ActionKind::Cancel,
            false,
            window,
            cx,
            move |app, window, cx| finish_rename(app, &cancel_row, false, window, cx),
        );
        div()
            .relative()
            .flex()
            .items_center()
            .min_w_0()
            .h(px(18.))
            .pr(px(52.))
            .text_size(px(14.))
            .font_weight(FontWeight(620.))
            .line_height(px(18.))
            .child(
                controls::text_input_container((key.clone(), "rename-shell"), &input)
                    .w(px(width))
                    .min_w(px(25.2))
                    .flex_shrink_1(),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right(px(24.))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(commit),
            )
            .child(
                div()
                    .absolute()
                    .top_0()
                    .right_0()
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(cancel),
            )
    } else {
        div()
            .min_w_0()
            .text_size(px(14.))
            .font_weight(FontWeight(620.))
            .line_height(px(18.))
            .text_ellipsis()
            .child(row.name.clone())
    };
    let default_row = row.clone();
    let default = action(
        (key.clone(), "default"),
        "check-mark",
        "Set as default",
        ActionKind::Default,
        row.default,
        window,
        cx,
        move |app, _, cx| {
            if default_row.id.is_empty() {
                return;
            }
            if default_row.input {
                let _ = crate::set_default_capture_device(&default_row.id);
            } else {
                let _ = crate::set_default_render_device(&default_row.id);
            }
            app.refresh_devices(cx);
        },
    );
    let rename_row = row.clone();
    let rename = (rename_allowed && !editing).then(|| {
        action(
            (key.clone(), "rename"),
            "pen-linear",
            "Rename",
            ActionKind::Normal,
            false,
            window,
            cx,
            move |app, _, cx| {
                let state = app
                    .sections
                    .devices
                    .rows
                    .entry(rename_row.id.clone())
                    .or_default();
                state.draft = rename_row.pretty_name.clone();
                state.editing = true;
                state.focus_pending = true;
                state.generation += 1;
                cx.notify();
            },
        )
    });
    let properties_row = row.clone();
    let properties = action(
        (key.clone(), "properties"),
        "settings-linear",
        "Open properties",
        ActionKind::Normal,
        false,
        window,
        cx,
        move |_, _, _| {
            let _ = crate::open_audio_device_properties(&properties_row.id, properties_row.input);
        },
    );
    let rename_slot = anim::tween(
        window,
        cx,
        (key.clone(), "rename-slot"),
        (rename_allowed && !editing) as u8 as f32,
        UI,
    );
    div()
        .id(key)
        .flex()
        .items_center()
        .gap(px(12.))
        .min_h(px(52.))
        .pl(px(14.))
        .pr(px(8.))
        .py(px(8.))
        .rounded(px(12.))
        .bg(bg.hsla())
        .shadow(vec![
            BoxShadow {
                color: p.focus.opacity(edit).hsla(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(1.),
                inset: true,
            },
            BoxShadow {
                color: mix_pct(p.focus, 18. * edit, TRANSPARENT).hsla(),
                offset: point(px(0.), px(0.)),
                blur_radius: px(0.),
                spread_radius: px(2.),
                inset: false,
            },
        ])
        .child(
            div()
                .flex()
                .items_center()
                .min_w_0()
                .flex_1()
                .gap(px(12.))
                .child(
                    svg()
                        .path(icon(if row.input {
                            "microphone-3-linear"
                        } else {
                            "volume-loud-linear"
                        }))
                        .size(px(20.))
                        .flex_shrink_0()
                        .text_color(p.accent_text.hsla()),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .child(name)
                        .when(active > 0., |this| {
                            this.child(
                                div()
                                    .h(px(16. * active))
                                    .pt(px(2. * active))
                                    .overflow_hidden()
                                    .opacity(active)
                                    .text_size(px(12.))
                                    .font_weight(FontWeight(560.))
                                    .line_height(px(14.))
                                    .text_color(lerp(p.muted, p.accent_text, active).hsla())
                                    .child("Default"),
                            )
                        }),
                ),
        )
        .child(
            div()
                .flex()
                .items_center()
                .flex_shrink_0()
                .child(default)
                .when(rename_slot > 0., |this| {
                    this.child(
                        div()
                            .w(px(42. * rename_slot))
                            .pl(px(6. * rename_slot))
                            .overflow_hidden()
                            .opacity(rename_slot)
                            .children(rename),
                    )
                })
                .child(div().pl(px(6.)).child(properties)),
        )
}

#[derive(Clone, Copy)]
enum ActionKind {
    Normal,
    Default,
    Commit,
    Cancel,
}
struct ActionTooltip(&'static str);
impl Render for ActionTooltip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(p.select_menu_bg.hsla())
            .text_color(p.text.hsla())
            .text_size(px(12.))
            .child(self.0)
    }
}
fn action(
    id: impl Into<ElementId>,
    icon_name: &'static str,
    title: &'static str,
    kind: ActionKind,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    callback: impl Fn(&mut SettingsApp, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let p = palette(cx);
    let callback = Rc::new(callback);
    let key_callback = callback.clone();
    let (pointer, hover, press) = controls::hover_tween(id.clone(), window, cx);
    let inline = matches!(kind, ActionKind::Commit | ActionKind::Cancel);
    // GPUI Div has no transform. Scale the painted box and SVG inside the
    // fixed 18px action slot, preserving the CSS transform's layout footprint.
    let entrance = if inline {
        anim::tween_from(
            window,
            cx,
            (id.clone(), "entrance"),
            0.,
            1.,
            Transition::new(160, Easing::Bezier(0.22, 1., 0.36, 1.)),
        )
    } else {
        1.
    };
    let scale = 0.82 + 0.18 * entrance;
    let default_active = anim::tween(
        window,
        cx,
        (id.clone(), "default-active"),
        disabled as u8 as f32,
        UI,
    );
    let fg = match kind {
        ActionKind::Default => lerp(lerp(p.muted, p.text, hover), p.accent_text, default_active),
        ActionKind::Commit => p.success,
        ActionKind::Cancel => lerp(p.danger, p.danger_hover, hover),
        _ => lerp(p.muted, p.text, if disabled { 0. } else { hover }),
    };
    let focus = window
        .use_keyed_state((id.clone(), "focus"), cx, |_, cx| cx.focus_handle())
        .read(cx)
        .clone();
    controls::track_pointer(
        div()
            .id(id)
            .size(px(if inline { 18. * scale } else { 36. }))
            .flex_shrink_0()
            .opacity(entrance)
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(if inline { 6. * scale } else { 8. }))
            .bg(lerp(
                lerp(TRANSPARENT, p.surface_3, hover),
                p.surface_2,
                if disabled || inline { 0. } else { press },
            )
            .hsla())
            .when(!disabled, |this| {
                this.cursor_pointer()
                    .track_focus(&focus)
                    .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
                    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            key_callback(app, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |app, _, window, cx| callback(app, window, cx)))
            })
            .tooltip(move |_, cx| cx.new(|_| ActionTooltip(title)).into())
            .child(
                svg()
                    .path(icon(icon_name))
                    .size(px(if inline { 14. * scale } else { 17. }))
                    .text_color(fg.hsla()),
            ),
        pointer,
    )
}
