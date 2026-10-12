//! About tab. TODO: port from `src/gui/sections/about.rs`.
use crate::settings_ui::{SettingsApp, style};
use gpui::{AnyElement, Context, IntoElement, Window};

#[derive(Default)]
pub(crate) struct State {}

pub(crate) fn render(
    app: &mut SettingsApp,
    _window: &mut Window,
    _cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    app.nav
        .displayed_tab
        .sections()
        .iter()
        .map(|section| style::h1(section.label).into_any_element())
        .collect()
}
