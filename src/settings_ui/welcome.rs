//! Welcome sequence shown until `config.welcome_completed`. TODO: port from
//! `src/gui/welcome.rs`.
use super::{SettingsApp, style};
use gpui::{Context, IntoElement, Window};

#[derive(Default)]
pub(crate) struct State {}

pub(crate) fn render(
    _app: &mut SettingsApp,
    _window: &mut Window,
    _cx: &mut Context<SettingsApp>,
) -> impl IntoElement {
    style::h1("Welcome")
}
