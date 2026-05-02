use dioxus::prelude::*;

use super::tabs::{SettingsTab, TabTransition};

mod about;
mod advanced;
mod auto_mute;
mod general;
mod hold_to_mute;
mod hotkeys;
mod overlay;
mod sounds;
mod tray_icon;

pub fn render(
    tab: SettingsTab,
    settings: Signal<super::SettingsSnapshot>,
    recording: Signal<bool>,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
    hotkey_modal_request: Signal<Option<super::HotkeyModalRequest>>,
    pending_hotkey_modal_after_nav: Signal<Option<super::HotkeyModalRequest>>,
) -> Element {
    match tab {
        SettingsTab::General => rsx! { GeneralSection { settings, recording } },
        SettingsTab::HoldToMute => rsx! {
            HoldToMuteSection {
                settings,
                active_tab,
                active_section,
                displayed_tab,
                transition,
                transition_id,
                pending_tab,
                pending_hotkey_modal_after_nav,
            }
        },
        SettingsTab::Hotkeys => rsx! { HotkeysSection { settings, hotkey_modal_request } },
        SettingsTab::Sounds => rsx! { SoundsSection { settings } },
        SettingsTab::Overlay => rsx! { OverlaySection { settings } },
        SettingsTab::TrayIcon => rsx! { TrayIconSection { settings } },
        SettingsTab::AutoMute => rsx! { AutoMuteSection { settings } },
        SettingsTab::Advanced => rsx! { AdvancedSection { settings } },
        SettingsTab::About => rsx! { AboutSection {} },
    }
}

#[component]
fn GeneralSection(settings: Signal<super::SettingsSnapshot>, recording: Signal<bool>) -> Element {
    general::render(settings, recording)
}

#[component]
fn HoldToMuteSection(
    settings: Signal<super::SettingsSnapshot>,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
    pending_hotkey_modal_after_nav: Signal<Option<super::HotkeyModalRequest>>,
) -> Element {
    hold_to_mute::render(
        settings,
        active_tab,
        active_section,
        displayed_tab,
        transition,
        transition_id,
        pending_tab,
        pending_hotkey_modal_after_nav,
    )
}

pub fn hotkey_modal_host(
    settings: Signal<super::SettingsSnapshot>,
    hotkey_modal_request: Signal<Option<super::HotkeyModalRequest>>,
) -> Element {
    hotkeys::modal_host(settings, hotkey_modal_request)
}

#[component]
fn HotkeysSection(
    settings: Signal<super::SettingsSnapshot>,
    hotkey_modal_request: Signal<Option<super::HotkeyModalRequest>>,
) -> Element {
    hotkeys::render(settings, hotkey_modal_request)
}

#[component]
fn SoundsSection(settings: Signal<super::SettingsSnapshot>) -> Element {
    sounds::render(settings)
}

#[component]
fn OverlaySection(settings: Signal<super::SettingsSnapshot>) -> Element {
    overlay::render(settings)
}

#[component]
fn TrayIconSection(settings: Signal<super::SettingsSnapshot>) -> Element {
    tray_icon::render(settings)
}

#[component]
fn AutoMuteSection(settings: Signal<super::SettingsSnapshot>) -> Element {
    auto_mute::render(settings)
}

#[component]
fn AdvancedSection(settings: Signal<super::SettingsSnapshot>) -> Element {
    advanced::render(settings)
}

#[component]
fn AboutSection() -> Element {
    about::render()
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
