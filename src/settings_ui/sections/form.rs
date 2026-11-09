//! Form pieces shared by the microphone pages and the volume pages.
use crate::settings_ui::{
    SettingsApp, anim,
    controls::{self, RangeOptions},
    style,
};
use gpui::{prelude::*, *};

/// Label stacked above a control. Same spacing as the overlay fields.
pub(crate) fn field(label: &'static str, control: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(10.))
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
        .when(!last, |d| {
            d.pb(px(18.))
                .border_b_1()
                .border_color(style::palette(cx).border.hsla())
        })
}

/// Sub-section title inside a page ("Appearance", "Clicks").
pub(crate) fn section_heading(label: &'static str) -> Div {
    div()
        .h(px(40.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .child(style::h1(label))
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

/// Full-width choice card: preview above a label, cards share the row.
/// Used for tray icon styles on both the microphone and volume pages.
pub(crate) fn choice_card(
    id: SharedString,
    label: &'static str,
    selected: bool,
    preview: AnyElement,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_select: impl Fn(&mut SettingsApp, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    use crate::settings_ui::color::lerp;
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
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .py(px(12.))
            .rounded(px(12.))
            .border_1()
            .border_color(lerp(p.border, p.accent, active).hsla())
            .bg(lerp(p.surface_2, p.surface_hover, hover).hsla())
            .cursor_pointer()
            .child(
                div()
                    .h(px(56.))
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .overflow_hidden()
                    .child(preview),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .line_height(relative(1.2))
                    .font_weight(FontWeight(600.))
                    .whitespace_nowrap()
                    .text_color(lerp(p.muted, p.text, active.max(hover)).hsla())
                    .child(label),
            ),
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
