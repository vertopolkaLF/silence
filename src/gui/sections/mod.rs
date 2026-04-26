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
    settings: Signal<super::SettingsSnapshot>,
    recording: Signal<bool>,
) -> Element {
    match tab {
        SettingsTab::General => general::render(settings, recording),
        SettingsTab::HoldToMute => hold_to_mute::render(),
        SettingsTab::Sounds => sounds::render(settings),
        SettingsTab::Overlay => overlay::render(settings),
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

#[component]
pub(super) fn Toggle(checked: bool, onchange: EventHandler<bool>) -> Element {
    rsx! {
        label { class: "toggle",
            input {
                r#type: "checkbox",
                checked,
                onchange: move |evt| onchange.call(evt.checked())
            }
            span { class: "toggle-track" }
        }
    }
}
