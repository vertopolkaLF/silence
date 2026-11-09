//! Volume → Media keys. Placeholder until the controls land.
use crate::settings_ui::SettingsApp;
use gpui::{AnyElement, Context, Window};

pub(crate) fn render(
    _app: &mut SettingsApp,
    _window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    vec![super::page(
        "Media keys",
        "What the keyboard media keys do to volume.",
        cx,
    )]
}