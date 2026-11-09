//! Volume → Tray icon. Placeholder until the controls land.
use crate::settings_ui::SettingsApp;
use gpui::{AnyElement, Context, Window};

pub(crate) fn render(
    _app: &mut SettingsApp,
    _window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    vec![super::page(
        "Tray icon",
        "How volume shows up in the system tray.",
        cx,
    )]
}
