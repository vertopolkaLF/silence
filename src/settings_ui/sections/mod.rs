//! One module per sidebar tab. Each `render` returns one block per entry in
//! `SettingsTab::sections()`, in order: the content area uses the blocks'
//! positions for sub-section tracking and smooth scrolling.
use super::{SettingsApp, shell::nav::SettingsTab};
use gpui::{AnyElement, Context, Window};
pub(crate) mod general;
pub(crate) mod devices;
pub(crate) mod sounds;
pub(crate) mod overlay;
pub(crate) mod hold_to_mute;
pub(crate) mod tray_icon;
pub(crate) mod auto_mute;
pub(crate) mod about;
pub(crate) mod hotkeys;

/// Per-section UI state that must survive re-renders and tab switches.
#[derive(Default)]
pub(crate) struct Sections {
    pub general: general::State,
    pub devices: devices::State,
    pub sounds: sounds::State,
    pub overlay: overlay::State,
    pub hold_to_mute: hold_to_mute::State,
    pub tray_icon: tray_icon::State,
    pub auto_mute: auto_mute::State,
    pub about: about::State,
    pub hotkeys: hotkeys::State,
}

pub(crate) fn render(
    app: &mut SettingsApp,
    tab: SettingsTab,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    match tab {
        SettingsTab::General => general::render(app, window, cx),
        SettingsTab::Devices => devices::render(app, window, cx),
        SettingsTab::Sounds => sounds::render(app, window, cx),
        SettingsTab::Overlay => overlay::render(app, window, cx),
        SettingsTab::HoldToMute => hold_to_mute::render(app, window, cx),
        SettingsTab::TrayIcon => tray_icon::render(app, window, cx),
        SettingsTab::AutoMute => auto_mute::render(app, window, cx),
        SettingsTab::About => about::render(app, window, cx),
        SettingsTab::Hotkeys => hotkeys::render(app, window, cx),
    }
}
