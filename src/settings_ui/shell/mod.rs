//! Window chrome around the sections: titlebar, sidebar and content area.
pub(crate) mod content;
pub(crate) mod nav;
pub(crate) mod sidebar;
pub(crate) mod titlebar;

use super::SettingsApp;
use gpui::{Context, IntoElement, ParentElement, Styled, Window, div};

/// `.body`: 240px sidebar + content. After the welcome sequence both fade in
/// (`main-app-screen-in .8s ease`).
pub(crate) fn render_main(
    app: &mut SettingsApp,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    let intro = app.main_intro.map(|started| {
        let t = (started.elapsed().as_secs_f32() / 0.8).min(1.);
        if t < 1. {
            window.request_animation_frame();
        }
        super::anim::Easing::Ease.apply(t)
    });
    if intro == Some(1.) {
        app.main_intro = None;
    }
    let opacity = intro.unwrap_or(1.);
    div()
        .flex()
        .size_full()
        .child(div().h_full().opacity(opacity).child(sidebar::render(app, window, cx)))
        .child(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .h_full()
                .opacity(opacity)
                .child(content::render(app, window, cx)),
        )
}
