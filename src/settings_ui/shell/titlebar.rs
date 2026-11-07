//! Custom titlebar (`assets/styles/titlebar.css`): brand, update pill, drag
//! area and the close button.
use crate::settings_ui::{
    SettingsApp,
    anim::{self, UI},
    assets::icon,
    color::lerp,
    theme::FONT_HEADING,
};
use gpui::{
    BoxShadow, Context, FontWeight, InteractiveElement, IntoElement, ParentElement, Styled,
    Window, WindowControlArea, div, img, point, prelude::*, px, svg,
};

pub(crate) fn render(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let palette = app.palette;
    let brand = app.snapshot.config.welcome_completed.then(|| {
        let update = app.available_update.is_some().then(|| update_pill(window, cx));
        div()
            .w(px(216.))
            .h(px(34.))
            .mt(px(14.))
            .mx(px(12.))
            .mb(px(12.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .flex_shrink_0()
            .font_family(FONT_HEADING)
            .text_size(px(14.))
            .font_weight(FontWeight(620.))
            .text_color(palette.text.hsla())
            .child(img("app.png").size(px(24.)).flex_shrink_0())
            .child(div().flex_1().min_w_0().child("silence!"))
            .children(update)
    });

    let close_pointer = anim::pointer(window, cx, "titlebar-close-pointer");
    let hovered = close_pointer.read(cx).hovered;
    let hover = anim::tween(window, cx, "titlebar-close-hover", hovered as u8 as f32, UI);
    let close_bg = lerp(crate::settings_ui::color::TRANSPARENT, palette.danger, hover);
    let close_fg = lerp(palette.muted, palette.on_accent, hover);

    div()
        .absolute()
        .top_0()
        .left_0()
        .right_0()
        .h(px(42.))
        .flex()
        .items_start()
        .children(brand.map(|brand| brand.id("titlebar-brand").window_control_area(WindowControlArea::Drag)))
        .child(
            div()
                .id("titlebar-drag")
                .flex_1()
                .h(px(42.))
                .window_control_area(WindowControlArea::Drag),
        )
        .child(
            div()
                .id("titlebar-close")
                .w(px(46.))
                .h(px(42.))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .bg(close_bg.hsla())
                .text_color(close_fg.hsla())
                .cursor_pointer()
                .on_hover({
                    let close_pointer = close_pointer.clone();
                    move |hovered, _, cx| anim::set_hovered(&close_pointer, *hovered, cx)
                })
                .on_click(cx.listener(|app, _, _, cx| app.close(cx)))
                .child(
                    svg()
                        .path(icon("codicon_close"))
                        .size(px(16.))
                        .text_color(close_fg.hsla()),
                ),
        )
}

/// `.titlebar-update-pill`: accent-tinted pill that lifts on hover.
fn update_pill(window: &mut Window, cx: &mut Context<SettingsApp>) -> impl IntoElement {
    let palette = crate::settings_ui::style::palette(cx);
    let pointer = anim::pointer(window, cx, "titlebar-update-pointer");
    let hovered = pointer.read(cx).hovered;
    let hover = anim::tween(window, cx, "titlebar-update-hover", hovered as u8 as f32, UI);
    let rest_bg = palette.accent_soft;
    let hover_bg = crate::settings_ui::color::mix_pct(palette.accent, 18., palette.surface_2);
    let shadow_color = lerp(palette.shadow_faint, palette.shadow_soft, hover);
    div()
        .id("titlebar-update-pill")
        .h(px(22.))
        .flex()
        .items_center()
        .gap(px(5.))
        .flex_shrink_0()
        .px(px(9.))
        .rounded_full()
        .bg(lerp(rest_bg, hover_bg, hover).hsla())
        .text_color(palette.accent_text.hsla())
        .font_family(crate::settings_ui::theme::FONT_TEXT)
        .text_size(px(11.))
        .font_weight(FontWeight(760.))
        .cursor_pointer()
        .relative()
        .top(px(-hover))
        .shadow(vec![BoxShadow {
            color: shadow_color.hsla(),
            offset: point(px(0.), px(6. + 2. * hover)),
            blur_radius: px(14. + 4. * hover),
            spread_radius: px(0.),
            inset: false,
        }])
        .on_hover(move |hovered, _, cx| anim::set_hovered(&pointer, *hovered, cx))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(cx.listener(|app, _, _, cx| {
            app.nav.select(super::nav::SettingsTab::About);
            crate::settings_ui::sections::about::check(app, true, cx);
        }))
        .child(
            svg()
                .path(icon("download-minimalistic-bold"))
                .size(px(13.))
                .text_color(palette.accent_text.hsla()),
        )
        .child("Update")
}
