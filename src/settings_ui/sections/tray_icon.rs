//! Tray icon (`src/gui/sections/tray_icon.rs`, `overlay.css` and `general.css`).
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition},
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{self, SelectOption, SelectOptions},
    style,
};
use gpui::{prelude::*, *};
use std::{collections::BTreeMap, time::Instant};

#[derive(Default)]
pub(crate) struct State {
    icons_expanded: bool,
    /// Row identities survive config/live refreshes; only new mounts animate.
    row_mounts: BTreeMap<String, Instant>,
}

const TILE_HEIGHT: f32 = 56. + 5. + 16.5;
const EXTRA_HEIGHT: f32 = 2. + TILE_HEIGHT * 2. + 10.;

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
    let status = anim::tween(
        window,
        cx,
        "tray-status-reveal",
        status_open as u8 as f32,
        anim::UI,
    );
    let extra = anim::tween(
        window,
        cx,
        "tray-extra-reveal",
        expanded as u8 as f32,
        anim::UI,
    );

    let mut variants = div().flex().gap(px(12.));
    for (value, label) in [
        ("Logo", "Logo"),
        ("StatusMic", "Mic status"),
        ("ColorDot", "Color dot"),
    ] {
        let preview = match value {
            "Logo" => img("app.png").size(px(28.)).into_any_element(),
            // The original style tile always uses Solar mic, even while muted.
            "StatusMic" => svg()
                .path(icon("microphone-3-linear"))
                .size(px(30.))
                .text_color(tone.hsla())
                .into_any_element(),
            // The style tile's dot stays green; it is not the live tray icon.
            _ => div()
                .size(px(22.))
                .rounded_full()
                .bg(p.success.hsla())
                .into_any_element(),
        };
        variants = variants.child(tile(
            format!("tray-variant-{value}").into(),
            label,
            config.variant == value,
            preview,
            window,
            cx,
            move |app, _, cx| {
                if value != "StatusMic" && app.open_select.as_deref() == Some("tray-status-color") {
                    app.open_select = None;
                }
                app.update_config(cx, |config| config.tray_icon.variant = value.to_string());
            },
        ));
    }

    let mut primary = div().flex().justify_between().w_full();
    for pair in crate::overlay_icons::featured_overlay_icon_pairs() {
        primary = primary.child(mic_tile(pair, &config.icon_pair, muted, window, cx));
    }
    primary = primary.child(tile(
        "tray-icons-expand".into(),
        if expanded { "Collapse" } else { "Expand" },
        false,
        svg()
            .path(icon("alt-arrow-down-linear"))
            .size(px(30.))
            .text_color(p.muted.hsla())
            .with_transformation(Transformation::rotate(gpui::radians(
                std::f32::consts::PI * extra,
            )))
            .into_any_element(),
        window,
        cx,
        |app, _, cx| {
            app.sections.tray_icon.icons_expanded = !app.sections.tray_icon.icons_expanded;
            cx.notify();
        },
    ));

    let mut extras = div().flex().flex_col().gap(px(10.)).pt(px(2.));
    if status > 0. && extra > 0. {
        for pairs in crate::overlay_icons::extra_overlay_icon_pairs().chunks(6) {
            extras = extras.child(
                div().flex().justify_between().w_full().children(
                    pairs
                        .iter()
                        .map(|pair| mic_tile(pair, &config.icon_pair, muted, window, cx)),
                ),
            );
        }
    }
    let mut status_content = div().flex().flex_col().gap(px(16.)).w_full().child(
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .flex_shrink_0()
            .child(style::label("Mic icons"))
            .child(primary)
            // CSS gives the closed extra wrapper a -2px margin, leaving 8px
            // of the field's 10px gap. Clip during the 200ms grid reveal.
            .child(
                div()
                    .mt(px(-2.))
                    .h(px(EXTRA_HEIGHT * extra))
                    .flex_shrink_0()
                    .overflow_hidden()
                    .opacity(extra)
                    .child(extras),
            ),
    );
    if status > 0. {
        let mut options = SelectOptions::new(vec![
            SelectOption::new("Colored", "Colored").icon("pallete-2-linear"),
            SelectOption::new("Monochrome", "Monochrome").icon("ic-baseline-contrast"),
            SelectOption::new("SystemColor", "System color").icon("widget-linear"),
        ]);
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
        status_content = status_content.child(
            div()
                .flex()
                .flex_col()
                .gap(px(10.))
                .flex_shrink_0()
                .child(style::label("Mic icon color"))
                .child(select),
        );
    }
    // Fixed window / six-column grid: label + primary + collapsed extra gap,
    // then the inner 16px gap and the color label + 44px select.
    let status_height =
        18.9 + 10. + TILE_HEIGHT + 8. + 16. + 18.9 + 10. + 44. + EXTRA_HEIGHT * extra;
    let overview = div()
        .id("tray-icon-overview")
        .flex()
        .flex_col()
        .gap(px(18.))
        .max_w(px(720.))
        .child(
            style::page_header()
                .h(px(40.))
                .mb(px(4.))
                .child(style::h1("Tray icon")),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(18.))
                .pb(px(18.))
                .border_b_1()
                .border_color(p.border.hsla())
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(10.))
                        .child(style::label("Tray icon style"))
                        .child(variants),
                )
                .child(
                    div()
                        .h(px(status_height * status))
                        .flex_shrink_0()
                        .overflow_hidden()
                        .opacity(status)
                        .when(status > 0., |this| this.child(status_content)),
                ),
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
        .pt(px(18.))
        .max_w(px(720.))
        .flex()
        .flex_col()
        .gap(px(16.))
        .child(toggle_row("Show when mic is in use by an app", show))
        .child(toggle_row("Hide blacklisted apps from tray menu", hide))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(14.))
                .child(current)
                .child(ignored),
        );
    vec![overview.into_any_element(), mic_in_use.into_any_element()]
}

fn toggle_row(title: &'static str, toggle: impl IntoElement) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(6.))
        .min_h(px(44.))
        .child(style::h2(title).min_w_0())
        .child(toggle)
}

fn mic_tile(
    pair: &crate::overlay_icons::OverlayIconPair,
    selected: &str,
    muted: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Stateful<Div> {
    let asset = if muted {
        pair.muted_asset
    } else {
        pair.unmuted_asset
    };
    let p = style::palette(cx);
    let id = pair.id;
    tile(
        format!("tray-mic-{id}").into(),
        pair.label,
        selected == id,
        svg()
            .path(SharedString::from(
                asset.trim_start_matches("/assets/").to_string(),
            ))
            .size(px(30.))
            .text_color(if muted { p.danger } else { p.success }.hsla())
            .into_any_element(),
        window,
        cx,
        move |app, _, cx| app.update_config(cx, |c| c.tray_icon.icon_pair = id.to_string()),
    )
}

/// `.overlay-icon-option` and its 56px preview; hover never changes borders.
fn tile(
    id: SharedString,
    label: &'static str,
    selected: bool,
    preview: AnyElement,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let p = style::palette(cx);
    let (pointer, hover, _) = controls::hover_tween(id.clone(), window, cx);
    let active = anim::tween(
        window,
        cx,
        (ElementId::from(id.clone()), "selected"),
        selected as u8 as f32,
        anim::UI,
    );
    let lift = hover.max(active);
    let focus = window
        .use_keyed_state((ElementId::from(id.clone()), "focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let on_click = std::rc::Rc::new(on_click);
    let key_click = on_click.clone();
    let mouse_focus = focus.clone();
    let hint = match label {
        "Expand" => "Expand icons",
        "Collapse" => "Collapse icons",
        label => label,
    };
    let mut preview_shadows = vec![BoxShadow {
        color: p.shadow_faint.opacity(active).hsla(),
        offset: point(px(0.), px(10.)),
        blur_radius: px(18.),
        spread_radius: px(0.),
        inset: false,
    }];
    // `.overlay-icon-option:focus` outlines the preview, not its caption.
    if focus.is_focused(window) {
        preview_shadows.push(BoxShadow {
            color: p.accent.hsla(),
            offset: point(px(0.), px(0.)),
            blur_radius: px(0.),
            spread_radius: px(1.),
            inset: false,
        });
    }
    controls::track_pointer(
        div()
            .id(id)
            .track_focus(&focus)
            .tab_index(0)
            .w(px(56.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(5.))
            .relative()
            .top(px(-2. * lift))
            .cursor_pointer()
            .text_size(px(11.))
            .line_height(relative(1.5))
            .font_weight(FontWeight(400.))
            .text_color(lerp(p.muted, p.text, lift).hsla())
            .child(
                div()
                    .size(px(56.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(12.))
                    .border_1()
                    .border_color(lerp(TRANSPARENT, p.accent, active).hsla())
                    .bg(p.surface_2.hsla())
                    .overflow_hidden()
                    .shadow(preview_shadows)
                    .child(preview),
            )
            // Labels wider than the 56px tile overflow centered, as in CSS.
            .child(div().whitespace_nowrap().child(label)),
        pointer,
    )
    .tooltip(move |_, cx| cx.new(|_| Hint(hint.into())).into())
    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
        mouse_focus.focus(window, cx)
    })
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            key_click(app, window, cx);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, window, cx| on_click(app, window, cx)))
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
        .pl(px(14.))
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
            .px(px(14.))
            .rounded(px(12.))
            .border_1()
            .border_dashed()
            .border_color(p.border_strong.hsla())
            .bg(mix_pct(p.surface_2, 76., TRANSPARENT).hsla())
            .text_color(p.muted.hsla())
            .child(empty)
    } else {
        div().flex().flex_col().gap(px(8.)).children(rows)
    };
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
        .child(style::h2(title))
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
