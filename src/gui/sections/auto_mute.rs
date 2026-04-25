use dioxus::prelude::*;

use super::super::tabs::SettingsTab;

pub fn render() -> Element {
    super::empty_section(SettingsTab::AutoMute)
}
