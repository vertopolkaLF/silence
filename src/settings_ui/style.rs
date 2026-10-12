//! Typography and small element builders shared by every section, ported from
//! `assets/styles/global.css` and the section stylesheets.
use super::theme::{FONT_DISPLAY, FONT_HEADING, Palette};
use gpui::{
    App, Div, FontWeight, IntoElement, ParentElement, SharedString, Styled, div, px, relative,
};

pub(crate) fn palette(cx: &App) -> Palette {
    *cx.global::<Palette>()
}

/// `h1`: Bricolage Grotesque 24px/1.2, weight 650.
pub(crate) fn h1(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(FONT_DISPLAY)
        .text_size(px(24.))
        .line_height(relative(1.2))
        .font_weight(FontWeight(650.))
        .child(text.into())
}

/// `h2`: Bricolage Grotesque 17px/1.25, weight 650.
pub(crate) fn h2(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(FONT_HEADING)
        .text_size(px(17.))
        .line_height(relative(1.25))
        .font_weight(FontWeight(650.))
        .child(text.into())
}

/// `h3`: Bricolage Grotesque 14px/1.35, weight 600.
pub(crate) fn h3(text: impl Into<SharedString>) -> Div {
    div()
        .font_family(FONT_HEADING)
        .text_size(px(14.))
        .line_height(relative(1.35))
        .font_weight(FontWeight(600.))
        .child(text.into())
}

/// `h4`: Bricolage Grotesque 14px/1.35, weight 600.
pub(crate) fn h4(text: impl Into<SharedString>) -> Div {
    h3(text)
}

/// `label`: Plus Jakarta Sans 14px/1.35, weight 600.
pub(crate) fn label(text: impl Into<SharedString>) -> Div {
    div()
        .text_size(px(14.))
        .line_height(relative(1.35))
        .font_weight(FontWeight(600.))
        .child(text.into())
}

/// `p`: muted 16px body copy with 1.45 line height.
pub(crate) fn p(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .text_color(palette(cx).muted.hsla())
        .line_height(relative(1.45))
        .child(text.into())
}

/// A horizontal 1px rule in `--border` (section dividers).
pub(crate) fn divider(cx: &App) -> Div {
    div()
        .w_full()
        .h(px(1.))
        .flex_shrink_0()
        .bg(palette(cx).border.hsla())
}

/// Lays out a section page: `.content-inner > *` padding `44px 38px`.
pub(crate) fn page() -> Div {
    div()
        .flex()
        .flex_col()
        .w_full()
        .min_h_full()
        .px(px(38.))
        .py(px(44.))
}

/// `.auto-mute-header`: the page title row used by most sections.
pub(crate) fn page_header() -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.))
}

pub(crate) fn element_id(prefix: &str, suffix: impl std::fmt::Display) -> SharedString {
    SharedString::from(format!("{prefix}-{suffix}"))
}

/// Keeps a `div` only when `condition` holds (`if` in rsx).
pub(crate) fn when(condition: bool, element: impl FnOnce() -> Div) -> Option<Div> {
    condition.then(element)
}

pub(crate) trait IntoAny {
    fn any(self) -> gpui::AnyElement;
}

impl<T: IntoElement> IntoAny for T {
    fn any(self) -> gpui::AnyElement {
        self.into_any_element()
    }
}
