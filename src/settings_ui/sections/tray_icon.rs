//! Tray icon (`src/gui/sections/tray_icon.rs`, `overlay.css` and `general.css`).
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{self, SelectOption, SelectOptions},
    style,
};
use super::form;
use gpui::{prelude::*, *};
use std::{collections::BTreeMap, time::Instant};

#[derive(Default)]
pub(crate) struct State {
    icons_expanded: bool,
    /// Row identities survive config/live refreshes; only new mounts animate.
    row_mounts: BTreeMap<String, Instant>,
}

/// Native `title` hints from the original buttons, hosted by GPUI.
struct Hint(SharedString);

impl Render for Hint {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = style::palette(cx);
        div()
            .px(px(8.))
            .py(px(5.))
            .rounded(px(6.))
            .bg(p.select_menu_bg.hsla())
            .text_color(p.text.hsla())
            .text_size(px(12.))
            .child(self.0.clone())
    }
}

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let config = app.snapshot.config.tray_icon.clone();
    let p = app.palette;
    let muted = app.snapshot.muted;
    let tone = if muted { p.danger } else { p.success };
    let status_open = config.variant == "StatusMic";
    let expanded = app.sections.tray_icon.icons_expanded;

    let mut variants = div().w_full().flex().gap(px(8.));
    for (value, label) in [
        ("Logo", "Logo"),
        ("StatusMic", "Mic status"),
        ("ColorDot", "Color dot"),
    ] {
        let preview = match value {
            "Logo" => img(crate::settings_ui::assets::logo(24., window))
                .size(px(24.))
                .into_any_element(),
            // The original style tile always uses Solar mic, even while muted.
            "StatusMic" => svg()
                .path(icon("microphone-3-linear"))
                .size(px(24.))
                .text_color(tone.hsla())
                .into_any_element(),
            // The style tile's dot stays green; it is not the live tray icon.
            _ => div()
                .size(px(16.))
                .rounded_full()
                .bg(p.success.hsla())
                .into_any_element(),
        };
        variants = variants.child(form::choice_card(
            format!("tray-variant-{value}").into(),
            label,
            config.variant == value,
            preview,
            window,
            cx,
            move |app, cx| {
                if value != "StatusMic" && app.open_select.as_deref() == Some("tray-status-color") {
                    app.open_select = None;
                }
                app.update_config(cx, |config| config.tray_icon.variant = value.to_string());
            },
        ));
    }

    let picker = form::mic_icon_picker(
        "tray-mic",
        "tray-mic-rest",
        &config.icon_pair,
        muted,
        expanded,
        window,
        cx,
        |app, id, cx| app.update_config(cx, |c| c.tray_icon.icon_pair = id.to_string()),
        |app, cx| {
            app.sections.tray_icon.icons_expanded = !app.sections.tray_icon.icons_expanded;
            cx.notify();
        },
    );
    // Mic status only: the icon pack and its color open under the style row.
    let status_content = {
        let mut options = SelectOptions::new(vec![
            SelectOption::new("Colored", "Colored").icon("pallete-2-linear"),
            SelectOption::new("Monochrome", "Monochrome").icon("ic-baseline-contrast"),
            SelectOption::new("SystemColor", "System color").icon("widget-linear"),
        ])
        .row("Mic icon color");
        options.disabled = !status_open;
        let select = controls::select(
            "tray-status-color",
            config.status_style.clone(),
            options,
            app,
            window,
            cx,
            |app, value, _, cx| {
                app.update_config(cx, |c| c.tray_icon.status_style = value.to_string())
            },
        );
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(form::row_box().child(picker))
            .child(style::divider(cx))
            .child(select)
    };
    let indicator = controls::select(
        "tray-indicator-device",
        app.snapshot.config.advanced.indicator_device.clone(),
        SelectOptions::new(super::devices::indicator_device_options(
            &app.snapshot.devices,
            &app.snapshot.config.hotkeys,
            &app.snapshot.config.advanced.indicator_device,
            &app.snapshot.config.advanced.audio_device_name_display,
        ))
        .row("Microphone to show"),
        app,
        window,
        cx,
        |app, value, _, cx| {
            app.update_config(cx, |c| c.advanced.indicator_device = value.to_string())
        },
    );
    let double_click = controls::toggle(
        "tray-double-click-settings",
        !app.snapshot.config.advanced.disable_tray_double_click_settings,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |config| {
                config.advanced.disable_tray_double_click_settings = !checked
            })
        },
    );
    let overview = div()
        .id("tray-icon-overview")
        .flex()
        .flex_col()
        .flex_shrink_0()
        .max_w(px(720.))
        .child(form::page_title("Tray icon", None))
        .child(
            form::block()
                .child(
                    form::card(cx)
                        .row(form::card_field("Tray icon style", variants))
                        .reveal("tray-status-content", status_open, status_content, window, cx)
                        .row(indicator)
                        .row(form::setting_row(
                            "Double-click the icon to open settings",
                            Some("Turn off to mute on a single click with no delay."),
                            double_click,
                            cx,
                        )),
                )
                .child(form::footnote(
                    "The chosen microphone is the one the tray icon and the overlay show.",
                    cx,
                )),
        );

    let show = controls::toggle(
        "tray-show-mic-in-use",
        config.show_mic_in_use,
        window,
        cx,
        |app, checked, _, cx| app.update_config(cx, |c| c.tray_icon.show_mic_in_use = checked),
    );
    let hide = controls::toggle(
        "tray-hide-ignored",
        config.hide_mic_in_use_ignored_apps,
        window,
        cx,
        |app, checked, _, cx| {
            app.update_config(cx, |c| c.tray_icon.hide_mic_in_use_ignored_apps = checked)
        },
    );

    let apps = app.snapshot.mic_using_apps.clone();
    let mut current_rows = Vec::new();
    let mut ignored_rows = Vec::new();
    let mut mounted = Vec::new();
    for mic_app in apps {
        let ignored = config
            .mic_in_use_ignored_apps
            .iter()
            .any(|exe| exe.eq_ignore_ascii_case(&mic_app.exe_name));
        let key = format!("tray-current-{}-{}", mic_app.exe_name, mic_app.name);
        mounted.push(key.clone());
        current_rows.push(app_row(
            app,
            key,
            mic_app.name,
            mic_app.exe_name,
            true,
            ignored,
            window,
            cx,
        ));
    }
    for exe in &config.mic_in_use_ignored_apps {
        let key = format!("tray-ignored-{exe}");
        mounted.push(key.clone());
        ignored_rows.push(app_row(
            app,
            key,
            exe.clone(),
            exe.clone(),
            false,
            true,
            window,
            cx,
        ));
    }
    app.sections
        .tray_icon
        .row_mounts
        .retain(|key, _| mounted.contains(key));
    let current = app_group(
        "tray-current-group",
        "Currently using mic",
        "No apps are using the microphone",
        current_rows,
        window,
        cx,
    );
    let ignored = app_group(
        "tray-ignored-group",
        "Ignored apps",
        "No ignored apps",
        ignored_rows,
        window,
        cx,
    );
    let mic_in_use = div()
        .id("tray-icon-mic-in-use")
        .pt(px(form::SECTION_GAP))
        .max_w(px(720.))
        .flex()
        .flex_col()
        .flex_shrink_0()
        .gap(px(form::TILE_GAP))
        .child(form::section_heading("Mic in use", cx))
        .child(
            form::card(cx)
                .row(form::setting_row(
                    "Show when mic is in use by an app",
                    None,
                    show,
                    cx,
                ))
                .row(form::setting_row(
                    "Hide blacklisted apps from tray menu",
                    None,
                    hide,
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(20.))
                .pt(px(12.))
                .child(current)
                .child(ignored),
        );
    vec![overview.into_any_element(), mic_in_use.into_any_element()]
}

fn app_row(
    app: &mut SettingsApp,
    key: String,
    name: String,
    exe: String,
    current: bool,
    ignored: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> AnyElement {
    let p = app.palette;
    let started = app
        .sections
        .tray_icon
        .row_mounts
        .entry(key.clone())
        .or_insert_with(Instant::now);
    let t = (started.elapsed().as_secs_f32() / 0.18).min(1.);
    if t < 1. {
        window.request_animation_frame();
    }
    let entrance = Easing::Bezier(0.22, 1., 0.36, 1.).apply(t);
    let active = anim::tween(
        window,
        cx,
        (ElementId::from(SharedString::from(key.clone())), "ignored"),
        ignored as u8 as f32,
        anim::UI,
    );
    let button_id = SharedString::from(format!("{key}-toggle"));
    let (pointer, hover, press) = controls::hover_tween(button_id.clone(), window, cx);
    let focus = window
        .use_keyed_state(
            (ElementId::from(button_id.clone()), "focus"),
            cx,
            |_, cx| cx.focus_handle(),
        )
        .read(cx)
        .clone();
    let target = exe.clone();
    let key_target = exe.clone();
    let mouse_focus = focus.clone();
    let hint = if ignored { "Include" } else { "Ignore" };
    let button = controls::track_pointer(
        div()
            .id(button_id)
            .track_focus(&focus)
            .tab_index(0)
            .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
            .min_w(px(74.))
            .h(px(34.))
            .px(px(12.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .border_1()
            .border_color(TRANSPARENT.hsla())
            .bg(lerp(lerp(TRANSPARENT, p.surface_3, hover), p.surface_2, press).hsla())
            .text_color(lerp(lerp(p.muted, p.text, hover), p.accent_text, active).hsla())
            .text_size(px(12.))
            .font_weight(FontWeight(760.))
            .cursor_pointer()
            .child(if ignored { "Include" } else { "Ignore" }),
        pointer,
    )
    .tooltip(move |_, cx| cx.new(|_| Hint(hint.into())).into())
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        mouse_focus.focus(window, cx)
    })
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            toggle_ignored(app, &key_target, current, cx);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, _, cx| toggle_ignored(app, &target, current, cx)));
    div()
        .id(SharedString::from(key))
        .min_h(px(54.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.))
        .py(px(8.))
        .pl(px(form::ROW_PAD))
        .pr(px(8.))
        .rounded(px(12.))
        .bg(lerp(p.surface_2, mix_pct(p.accent, 10., p.surface_2), active).hsla())
        .relative()
        .top(px(8. * (1. - entrance)))
        .opacity(entrance)
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(12.))
                .child(
                    svg()
                        .path(icon("record-circle-linear"))
                        .size(px(20.))
                        .flex_shrink_0()
                        .text_color(p.accent_text.hsla()),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight(620.))
                                .line_height(px(18.))
                                .text_ellipsis()
                                .child(name),
                        )
                        .when(current, |this| {
                            this.child(
                                div()
                                    .max_w(px(360.))
                                    .text_size(px(12.))
                                    .font_weight(FontWeight(560.))
                                    .line_height(px(14.))
                                    .text_color(p.muted.hsla())
                                    .child(exe),
                            )
                        }),
                ),
        )
        .child(button)
        .into_any_element()
}

fn toggle_ignored(app: &mut SettingsApp, exe: &str, current: bool, cx: &mut Context<SettingsApp>) {
    app.update_config(cx, |config| {
        let ignored = &mut config.tray_icon.mic_in_use_ignored_apps;
        if ignored.iter().any(|name| name.eq_ignore_ascii_case(exe)) {
            ignored.retain(|name| !name.eq_ignore_ascii_case(exe));
        } else if current {
            ignored.push(exe.to_string());
        }
    });
}

fn app_group(
    id: &'static str,
    title: &'static str,
    empty: &'static str,
    rows: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = style::palette(cx);
    let count = rows.len();
    let estimate = if count == 0 {
        44.
    } else {
        count as f32 * 54. + count.saturating_sub(1) as f32 * 8.
    };
    // Executable names wrap in the original. Measure the actual rows so a
    // long name can grow the list without being clipped by its height tween.
    let measured =
        window.use_keyed_state((ElementId::from(id), "size"), cx, |_, _| (count, estimate));
    let (measured_count, measured_height) = *measured.read(cx);
    let height = anim::tween(
        window,
        cx,
        (ElementId::from(id), "height"),
        if measured_count == count {
            measured_height
        } else {
            estimate
        },
        Transition::new(180, Easing::Bezier(0.22, 1., 0.36, 1.)),
    );
    let list = if count == 0 {
        div()
            .min_h(px(44.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(form::ROW_PAD))
            .rounded(px(12.))
            .border_1()
            .border_dashed()
            .border_color(p.border_strong.hsla())
            .bg(mix_pct(p.surface_2, 76., TRANSPARENT).hsla())
            .text_size(px(13.))
            .text_color(p.muted.hsla())
            .child(empty)
    } else {
        div().flex().flex_col().gap(px(8.)).children(rows)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(form::TILE_GAP))
        .child(form::section_heading(title, cx))
        .child(div().h(px(height)).flex_shrink_0().overflow_hidden().child(
            list.on_children_prepainted(move |bounds, _, cx| {
                let height = if count == 0 {
                    44.
                } else {
                    bounds.iter().map(|b| b.size.height.as_f32()).sum::<f32>()
                        + count.saturating_sub(1) as f32 * 8.
                };
                measured.update(cx, |size, cx| {
                    if size.0 != count || (size.1 - height).abs() > 0.5 {
                        *size = (count, height);
                        cx.notify();
                    }
                });
            }),
        ))
}
