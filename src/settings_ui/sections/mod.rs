//! One module per sidebar tab. Each `render` returns one block per entry in
//! `SettingsTab::sections()`, in order: the content area uses the blocks'
//! positions for sub-section tracking and smooth scrolling.
use super::{
    SettingsApp,
    modules::{self, ModuleId},
    shell::nav::SettingsTab,
};
use gpui::{AnyElement, Context, IntoElement, ParentElement, Styled, Window, div};
pub(crate) mod about;
pub(crate) mod auto_mute;
pub(crate) mod devices;
pub(crate) mod form;
pub(crate) mod general;
pub(crate) mod hold_to_mute;
pub(crate) mod hotkeys;
pub(crate) mod overlay;
pub(crate) mod sounds;
pub(crate) mod tray_icon;
pub(crate) mod volume;

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
    pub volume: volume::State,
}

pub(crate) fn render(
    app: &mut SettingsApp,
    tab: SettingsTab,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let module = tab.module();
    let blocks = match tab {
        SettingsTab::General => general::render(app, window, cx),
        SettingsTab::Devices => devices::render(app, window, cx),
        SettingsTab::Sounds => sounds::render(app, window, cx),
        SettingsTab::Overlay => overlay::render(app, window, cx),
        SettingsTab::HoldToMute => hold_to_mute::render(app, window, cx),
        SettingsTab::TrayIcon => tray_icon::render(app, window, cx),
        SettingsTab::AutoMute => auto_mute::render(app, window, cx),
        SettingsTab::About => about::render(app, window, cx),
        SettingsTab::Hotkeys => hotkeys::render(app, window, cx),
        SettingsTab::VolumeWheel => volume::wheel::render(app, window, cx),
        SettingsTab::VolumeOsd => volume::osd::render(app, window, cx),
        SettingsTab::VolumeMusic => volume::music::render(app, window, cx),
        SettingsTab::VolumeTray => volume::tray_icon::render(app, window, cx),
        SettingsTab::VolumeMediaKeys => volume::media_keys::render(app, window, cx),
    };
    match module {
        Some(module) => with_off_banner(app, module, blocks, window, cx),
        None => blocks,
    }
}

/// Keeps the "module is turned off" notice inside the first section block so
/// scroll-spy still sees one child per `SettingsTab::sections()` entry.
fn with_off_banner(
    app: &mut SettingsApp,
    module: ModuleId,
    blocks: Vec<AnyElement>,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
) -> Vec<AnyElement> {
    let Some(banner) = modules::off_banner(app, module, window, cx) else {
        return blocks;
    };
    let mut blocks = blocks.into_iter();
    let Some(first) = blocks.next() else {
        return vec![banner];
    };
    let mut wrapped = Vec::with_capacity(1 + blocks.len());
    wrapped.push(
        div()
            .flex()
            .flex_col()
            .w_full()
            .child(banner)
            .child(first)
            .into_any_element(),
    );
    wrapped.extend(blocks);
    wrapped
}
