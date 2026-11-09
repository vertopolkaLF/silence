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
