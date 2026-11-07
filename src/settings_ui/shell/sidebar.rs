//! Sidebar navigation (`assets/styles/tabs.css`).
use super::nav::SettingsTab;
use crate::settings_ui::{
    SettingsApp,
    anim::{self, Easing, Transition, UI},
    assets::icon,
    color::{TRANSPARENT, lerp},
};
use gpui::{
    Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled, Transformation,
    Window, div, prelude::*, px, size, svg,
};

const SUBITEM_HEIGHT: f32 = 32.;

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let mut column = div()
        .id("sidebar-scroll")
        .w(px(240.))
        .h_full()
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap(px(4.))
        .pt(px(60.))
        .px(px(12.))
        .pb(px(12.))
        .overflow_y_scroll();
    for &tab in SettingsTab::ALL {
        column = column.child(nav_item(app, tab, window, cx));
        if tab.sections().len() > 1 {
            column = column.child(subsections(app, tab, window, cx));
        }
    }
    column
}

fn nav_item(
    app: &mut SettingsApp,
    tab: SettingsTab,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let key = tab.order();
    let pointer = anim::pointer(window, cx, ("nav-pointer", key));
    let hovered = pointer.read(cx).hovered;
    let active = app.nav.active_tab == tab;
    let hover = anim::tween(window, cx, ("nav-hover", key), hovered as u8 as f32, UI);
    let on = anim::tween(window, cx, ("nav-active", key), active as u8 as f32, UI);

    // Active wins over hover, as in the stylesheet's cascade.
    let rest_bg = lerp(TRANSPARENT, palette.surface_color(2), hover);
    let bg = lerp(rest_bg, palette.surface_color(3), on);
    let rest_fg = lerp(palette.muted, palette.text, hover);
    let fg = lerp(rest_fg, palette.accent_text, on);

    let line_scale = 1. - 0.5 * on;
    let filled_scale = 0.5 + 0.6 * on;
    let icons = div()
        .relative()
        .size(px(21.))
        .flex_shrink_0()
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(21.))
                .path(icon(tab.icon()))
                .with_transformation(Transformation::scale(size(line_scale, line_scale)))
                .text_color(fg.opacity(1. - on).hsla()),
        )
        .child(
            svg()
                .absolute()
                .inset_0()
                .size(px(21.))
                .path(icon(tab.active_icon()))
                .with_transformation(Transformation::scale(size(filled_scale, filled_scale)))
                .text_color(fg.opacity(on).hsla()),
        );

    let arrow = (tab.sections().len() > 1).then(|| {
        svg()
            .size(px(15.))
            .flex_shrink_0()
            .ml_auto()
            .path(icon("alt-arrow-down-linear"))
            .with_transformation(Transformation::rotate(gpui::radians(-std::f32::consts::FRAC_PI_2 * (1. - on))))
            .text_color(fg.opacity(0.72 + 0.28 * on).hsla())
    });

    div()
        .id(("nav-item", key))
        .relative()
        .flex()
        .items_center()
        .flex_shrink_0()
        .gap(px(12.))
        .w_full()
        .h(px(42.))
        .px(px(14.))
        .rounded(px(8.))
        .bg(bg.hsla())
        .text_color(fg.hsla())
        .text_size(px(14.))
        .font_weight(FontWeight(600.))
        .cursor_pointer()
        .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
        .on_click(cx.listener(move |app, _, _, cx| {
            app.nav.select(tab);
            cx.notify();
        }))
        .child(icons)
        .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(tab.label()))
        .children(arrow)
}

/// `.nav-subsections-shell`: an accordion that opens under the active tab.
fn subsections(
    app: &mut SettingsApp,
    tab: SettingsTab,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let key = tab.order();
    let open = app.nav.active_tab == tab;
    let t = anim::tween(
        window,
        cx,
        ("nav-sub-open", key),
        open as u8 as f32,
        Transition::new(200, Easing::Ease),
    );
    let sections = tab.sections();
    let content_height = sections.len() as f32 * SUBITEM_HEIGHT + (sections.len() - 1) as f32;

    let mut list = div()
        .relative()
        .flex()
        .flex_col()
        .gap(px(1.))
        .pl(px(22.))
        .border_l_1()
        .border_color(palette.border.hsla());
    for (index, section) in sections.iter().enumerate() {
        let active = open && app.nav.active_section == section.id;
        let pointer = anim::pointer(window, cx, ("nav-sub-pointer", key * 8 + index));
        let hovered = pointer.read(cx).hovered;
        let hover = anim::tween(window, cx, ("nav-sub-hover", key * 8 + index), hovered as u8 as f32, UI);
        let on = anim::tween(window, cx, ("nav-sub-active", key * 8 + index), active as u8 as f32, UI);
        let fg = lerp(lerp(palette.muted, palette.text, hover), palette.accent, on);
        let id = section.id;
        list = list.child(
            div()
                .id(("nav-sub", key * 8 + index))
                .relative()
                .h(px(SUBITEM_HEIGHT))
                .flex()
                .items_center()
                .text_size(px(13.))
                .font_weight(if active { FontWeight(600.) } else { FontWeight(500.) })
                .text_color(fg.hsla())
                .cursor_pointer()
                .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
                .on_click(cx.listener(move |app, _, window, cx| {
                    super::content::scroll_to_section(app, id, window, cx);
                }))
                .children(active.then(|| {
                    div()
                        .absolute()
                        .left(px(-23.))
                        .top(px(8.))
                        .bottom(px(8.))
                        .w(px(2.))
                        .rounded_full()
                        .bg(palette.accent.hsla())
                }))
                .child(section.label),
        );
    }

    // grid-template-rows 0fr -> 1fr, margin -4px 0 0 -> 4px 0 8px,
    // opacity 0 -> 1, translateY(-3px) -> 0.
    div()
        .flex_shrink_0()
        .ml(px(24.))
        .mt(px(-4. + 8. * t))
        .mb(px(8. * t))
        .h(px(content_height * t))
        .overflow_hidden()
        .opacity(t)
        .child(div().relative().top(px(-3. * (1. - t))).child(list))
}
