use dioxus::prelude::*;

use super::tabs::SettingsTab;

mod about;
mod auto_mute;
mod general;
mod hold_to_mute;
mod overlay;
mod sounds;
mod tray_icon;

pub fn render(
    tab: SettingsTab,
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    recording: Signal<bool>,
    saved: Signal<bool>,
) -> Element {
    match tab {
        SettingsTab::General => general::render(shortcut, mic_device_id, recording, saved),
        SettingsTab::HoldToMute => hold_to_mute::render(),
        SettingsTab::Sounds => sounds::render(),
        SettingsTab::Overlay => overlay::render(),
        SettingsTab::TrayIcon => tray_icon::render(),
        SettingsTab::AutoMute => auto_mute::render(),
        SettingsTab::About => about::render(),
    }
}

fn empty_section(tab: SettingsTab) -> Element {
    rsx! {
        section {
            class: "empty-section",
            div {
                class: "empty-card",
                span { class: "solar-icon empty-icon icon-settings" }
                h1 { "{tab.label()}" }
                p { "This section is reserved for future settings." }
            }
        }
    }
}
