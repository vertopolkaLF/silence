//! Form pieces shared by the microphone pages and the volume pages.
use crate::settings_ui::{
    SettingsApp, anim,
    assets::icon,
    color::{TRANSPARENT, lerp, mix_pct},
    controls::{self, RangeOptions},
    style,
};
use gpui::{prelude::*, *};

/// Page title row with an optional trailing action; every page starts with it.
pub(crate) fn page_title(title: &'static str, trailing: Option<AnyElement>) -> Div {
    style::page_header()
        .h(px(40.))
        .flex_shrink_0()
        .mb(px(style::TITLE_GAP))
        .child(style::h1(title))
        .children(trailing)
}

/// One setting: label, optional description, control on the right.
pub(crate) fn setting_row(
    title: impl Into<SharedString>,
    description: Option<&'static str>,
    trailing: impl IntoElement,
    cx: &App,
) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.))
        .min_h(px(32.))
        .flex_shrink_0()
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .flex_1()
                .child(style::label(title))
                .children(description.map(|text| style::desc(text, cx))),
        )
        .child(trailing)
}

/// The switch for a whole page's feature. It is a labeled strip, so it cannot
/// be mistaken for the first setting's toggle under it.
pub(crate) fn master_switch(
    id: &'static str,
    icon_name: &'static str,
    title: &'static str,
    on: bool,
    toggle: impl IntoElement,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let p = style::palette(cx);
    let t = anim::tween(window, cx, (ElementId::from(id), "master"), on as u8 as f32, anim::UI);
    div()
        .flex()
        .items_center()
        .gap(px(14.))
        .flex_shrink_0()
        .px(px(14.))
        .py(px(12.))
        .rounded(px(14.))
        .border_1()
        .border_color(lerp(TRANSPARENT, mix_pct(p.accent, 34., TRANSPARENT), t).hsla())
        .bg(lerp(p.surface_2, p.accent_soft, t).hsla())
        .child(
            div()
                .size(px(34.))
                .flex_shrink_0()
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(lerp(p.surface_3, p.accent, t).hsla())
                .child(
                    svg()
                        .path(icon(icon_name))
                        .size(px(18.))
                        .text_color(lerp(p.muted, p.on_accent, t).hsla()),
                ),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .min_w_0()
                .flex_1()
                .child(style::label(title))
                .child(
                    style::desc(if on { "On" } else { "Off" }, cx)
                        .text_color(lerp(p.muted, p.accent_text, t).hsla()),
                ),
        )
        .child(toggle)
}

/// Opacity for everything a master switch governs: it fades while the
/// feature is off, and stays usable.
pub(crate) fn governed_opacity(
    id: &'static str,
    on: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> f32 {
    let t = anim::tween(window, cx, (ElementId::from(id), "governed"), on as u8 as f32, anim::UI);
    0.45 + 0.55 * t
}

/// Label stacked above a control.
pub(crate) fn field(label: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(style::LABEL_GAP))
        .min_w_0()
        .flex_shrink_0()
        .child(style::label(label))
        .child(control)
}

/// Page block separated from the next one by a rule, like the overlay page.
pub(crate) fn panel(gap: f32, last: bool, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(gap))
        .max_w(px(720.))
        .flex_shrink_0()
        .when(!last, |d| {
            d.pb(px(style::BLOCK_GAP))
                .border_b_1()
                .border_color(style::palette(cx).border.hsla())
        })
}

/// Sub-section title inside a page ("Appearance", "Clicks").
pub(crate) fn section_heading(label: &'static str) -> Div {
    style::section(label).flex_shrink_0().mb(px(2.))
}

/// Integer slider with a suffix ("2%", "1500 ms").
pub(crate) fn int_slider(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    suffix: &str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, f64, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let value = value.clamp(min, max);
    let text = format!("{value:.0}{suffix}");
    controls::range(
        id,
        value,
        RangeOptions {
            min,
            max,
            step,
            label: label.into(),
            value_label: text.into(),
            icon: None,
        },
        window,
        cx,
        on_change,
    )
}

/// 0–100% slider used for overlay placement and the volume OSD.
/// Callers add any page-specific margin; the track itself matches.
pub(crate) fn percent_slider(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: f64,
    suffix: &str,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, f64, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let value = value.clamp(0., 100.);
    let text = format!("{value:.0}{suffix}");
    controls::range(
        id,
        value,
        RangeOptions {
            min: 0.,
            max: 100.,
            step: 1.,
            label: label.into(),
            value_label: text.into(),
            icon: None,
        },
        window,
        cx,
        on_change,
    )
}

/// A control and the collapsible blocks that hang under it. The parent's gap
/// applies to the group as a whole, so a closed block leaves no hole.
pub(crate) fn group() -> Div {
    div().flex().flex_col().flex_shrink_0()
}

/// [`collapse`] for a block inside a [`group`]: the space above it is part of
/// the animated height. A zero-high child of a gapped column would still take
/// a gap on each side, and GPUI lays out negative margins wrongly.
pub(crate) fn collapse_in_gap(
    id: &'static str,
    open: bool,
    gap: f32,
    content: Div,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    collapse(id, open, div().pt(px(gap)).child(content), window, cx)
}

/// Measure the unshrunk child, just as CSS grid's 0fr/1fr transition does.
pub(crate) fn collapse(
    id: &'static str,
    open: bool,
    content: Div,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Div {
    let measured = window.use_keyed_state((ElementId::from(id), "measure"), cx, |_, _| 0f32);
    let height = *measured.read(cx);
    let inner = div()
        .w_full()
        .child(content.w_full().flex_shrink_0())
        .on_children_prepainted(move |bounds, _, cx| {
            if let Some(bounds) = bounds.first() {
                let height = bounds.size.height.as_f32();
                measured.update(cx, |old, cx| {
                    if (*old - height).abs() > 0.5 {
                        *old = height;
                        cx.notify();
                    }
                });
            }
        });
    if height == 0. {
        return div()
            .flex_shrink_0()
            .when(!open, |d| d.h(px(0.)).overflow_hidden())
            .child(inner);
    }
    let t = anim::tween(
        window,
        cx,
        (ElementId::from(id), "reveal"),
        open as u8 as f32,
        anim::UI,
    );
    let animated_height = anim::tween(
        window,
        cx,
        (ElementId::from(id), "height"),
        if open { height } else { 0. },
        anim::UI,
    );
    div()
        .flex_shrink_0()
        .h(px(animated_height))
        .opacity(t)
        .when(!open || (animated_height - height).abs() > 0.5, |d| {
            d.overflow_hidden()
        })
        .child(inner)
}

/// Choice card: preview and label side by side, cards share the row. The
/// selected card is filled and carries a check badge.
pub(crate) fn choice_card(
    id: SharedString,
    label: &'static str,
    selected: bool,
    preview: AnyElement,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_select: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
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
    let focus = window
        .use_keyed_state((ElementId::from(id.clone()), "focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let on_select = std::rc::Rc::new(on_select);
    let key_select = on_select.clone();
    controls::track_pointer(
        div()
            .id(id)
            .track_focus(&focus)
            .tab_index(0)
            .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
            .relative()
            .flex_1()
            .min_w_0()
            .h(px(52.))
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(lerp(TRANSPARENT, p.accent, active).hsla())
            .bg(lerp(
                lerp(p.surface_2, p.surface_hover, hover),
                p.accent_soft,
                active,
            )
            .hsla())
            .cursor_pointer()
            .child(
                div()
                    .w(px(32.))
                    .h(px(32.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(preview),
            )
            .child(
                div()
                    .min_w_0()
                    .text_size(px(13.))
                    .line_height(relative(1.2))
                    .font_weight(FontWeight(600.))
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(lerp(p.muted, p.text, active.max(hover)).hsla())
                    .child(label),
            )
            .child(check_badge(active, cx)),
        pointer,
    )
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            key_select(app, cx);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, _, cx| on_select(app, cx)))
}

/// Accent dot with a check in the top-right corner of a selected option.
fn check_badge(active: f32, cx: &App) -> Div {
    let p = style::palette(cx);
    div()
        .absolute()
        .top(px(5.))
        .right(px(5.))
        .size(px(14.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.accent.opacity(active).hsla())
        .child(
            svg()
                .path(icon("check-mark"))
                .size(px(9.))
                .text_color(p.on_accent.opacity(active).hsla()),
        )
}

/// Square icon option. Its name lives in the tooltip and, when selected, in
/// the picker's label.
fn icon_tile(
    id: SharedString,
    hint: &'static str,
    selected: bool,
    preview: AnyElement,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_click: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
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
    let focus = window
        .use_keyed_state((ElementId::from(id.clone()), "focus"), cx, |_, cx| {
            cx.focus_handle()
        })
        .read(cx)
        .clone();
    let on_click = std::rc::Rc::new(on_click);
    let key_click = on_click.clone();
    controls::track_pointer(
        div()
            .id(id)
            .track_focus(&focus)
            .tab_index(0)
            .focus_visible(move |style| controls::focus_outline(style, p.focus.hsla()))
            .size(px(56.))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .border_1()
            .border_color(lerp(TRANSPARENT, p.accent, active).hsla())
            .bg(lerp(
                lerp(p.surface_2, p.surface_hover, hover),
                p.accent_soft,
                active,
            )
            .hsla())
            .cursor_pointer()
            .child(preview),
        pointer,
    )
    .tooltip(move |_, cx| controls::tooltip(hint, cx))
    .on_key_down(cx.listener(move |app, event: &KeyDownEvent, _, cx| {
        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
            key_click(app, cx);
            cx.stop_propagation();
        }
    }))
    .on_click(cx.listener(move |app, _, _, cx| on_click(app, cx)))
}

/// Icons shown before the picker is expanded.
const MIC_ICONS_COLLAPSED: usize = 8;

/// Microphone icon picker shared by the overlay and the tray icon: an even
/// grid, the selected pack named in the label, the rest behind "All N".
/// `rest_id` keys the collapsed part of the grid.
pub(crate) fn mic_icon_picker(
    prefix: &'static str,
    rest_id: &'static str,
    selected: &str,
    muted: bool,
    expanded: bool,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_select: impl Fn(&mut SettingsApp, &'static str, &mut Context<SettingsApp>) + 'static,
    on_toggle: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
) -> Div {
    let p = style::palette(cx);
    let on_select = std::rc::Rc::new(on_select);
    let pairs = crate::overlay_icons::featured_overlay_icon_pairs()
        .iter()
        .chain(crate::overlay_icons::extra_overlay_icon_pairs())
        .collect::<Vec<_>>();
    let total = pairs.len();
    let selected_label = pairs
        .iter()
        .find(|pair| pair.id == selected)
        .map(|pair| pair.label)
        .unwrap_or("");
    let grid = || div().flex().flex_wrap().gap(px(8.));
    let mut primary = grid();
    let mut rest = grid();
    for (index, pair) in pairs.into_iter().enumerate() {
        let asset = if muted {
            pair.muted_asset
        } else {
            pair.unmuted_asset
        };
        let id = pair.id;
        let on_select = on_select.clone();
        let tile = icon_tile(
            format!("{prefix}-{id}").into(),
            pair.label,
            selected == id,
            svg()
                .path(SharedString::from(
                    asset.trim_start_matches("/assets/").to_string(),
                ))
                .size(px(28.))
                .text_color(if muted { p.danger } else { p.success }.hsla())
                .into_any_element(),
            window,
            cx,
            move |app, cx| on_select(app, id, cx),
        );
        if index < MIC_ICONS_COLLAPSED {
            primary = primary.child(tile);
        } else {
            rest = rest.child(tile);
        }
    }

    let link_id = SharedString::from(format!("{prefix}-all"));
    let (pointer, hover, _) = controls::hover_tween(link_id.clone(), window, cx);
    let turn = anim::tween(
        window,
        cx,
        (ElementId::from(link_id.clone()), "turn"),
        expanded as u8 as f32,
        anim::UI,
    );
    let link_color = lerp(p.accent_text, p.text, hover).hsla();
    let link = controls::track_pointer(
        div()
            .id(link_id)
            .flex()
            .items_center()
            .gap(px(4.))
            .flex_shrink_0()
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight(600.))
            .text_color(link_color)
            .child(if expanded {
                "Show fewer".to_string()
            } else {
                format!("All {total}")
            })
            .child(
                svg()
                    .path(icon("alt-arrow-down-linear"))
                    .size(px(14.))
                    .text_color(link_color)
                    .with_transformation(Transformation::rotate(radians(
                        std::f32::consts::PI * turn,
                    ))),
            ),
        pointer,
    )
    .on_click(cx.listener(move |app, _, _, cx| on_toggle(app, cx)));

    div()
        .flex()
        .flex_col()
        .gap(px(style::LABEL_GAP))
        .flex_shrink_0()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.))
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .min_w_0()
                        .child(style::label("Mic icons"))
                        .child(
                            style::label(format!("· {selected_label}"))
                                .font_weight(FontWeight(500.))
                                .text_color(p.muted.hsla()),
                        ),
                )
                .when(total > MIC_ICONS_COLLAPSED, |row| row.child(link)),
        )
        .child(
            group()
                .child(primary)
                .child(collapse_in_gap(rest_id, expanded, 8., rest, window, cx)),
        )
}
