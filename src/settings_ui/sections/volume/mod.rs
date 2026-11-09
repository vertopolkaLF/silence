//! Volume pages. Each one is its own file so it can be filled in later
//! without colliding with the others.
pub(crate) mod media_keys;
pub(crate) mod osd;
pub(crate) mod sounds;
pub(crate) mod tray_icon;
pub(crate) mod wheel;

use crate::settings_ui::{
    color::{TRANSPARENT, mix_pct},
    style,
};
use gpui::{AnyElement, App, IntoElement, ParentElement, Styled, div, px};

/// Page title, one-line intro, and an empty body for a volume page that
/// does not have its controls yet.
pub(crate) fn page(title: &'static str, intro: &'static str, cx: &App) -> AnyElement {
    let palette = style::palette(cx);
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(22.))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(8.))
                .child(style::h1(title))
                .child(style::p(intro, cx)),
        )
        .child(
            div()
                .min_h(px(72.))
                .flex()
                .items_center()
                .flex_shrink_0()
                .px(px(16.))
                .rounded(px(12.))
                .border_1()
                .border_dashed()
                .border_color(palette.border_strong.hsla())
                .bg(mix_pct(palette.surface_2, 76., TRANSPARENT).hsla())
                .text_size(px(14.))
                .text_color(palette.muted.hsla())
                .child("Nothing to change here yet."),
        )
        .into_any_element()
}
