use dioxus::prelude::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    HoldToMute,
    Sounds,
    Overlay,
    TrayIcon,
    AutoMute,
    About,
}

impl SettingsTab {
    const ALL: &'static [Self] = &[
        Self::General,
        Self::HoldToMute,
        Self::Sounds,
        Self::Overlay,
        Self::TrayIcon,
        Self::AutoMute,
        Self::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::HoldToMute => "Hold to Mute",
            Self::Sounds => "Sounds",
            Self::Overlay => "Overlay",
            Self::TrayIcon => "Tray Icon",
            Self::AutoMute => "Auto-Mute",
            Self::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "icon-settings",
            Self::HoldToMute => "icon-mic",
            Self::Sounds => "icon-volume",
            Self::Overlay => "icon-monitor",
            Self::TrayIcon => "icon-widget",
            Self::AutoMute => "icon-magic",
            Self::About => "icon-info",
        }
    }
}

pub fn render(mut active_tab: Signal<SettingsTab>) -> Element {
    rsx! {
        nav {
            class: "sidebar",
            for &tab in SettingsTab::ALL {
                button {
                    class: if active_tab() == tab { "nav-item active" } else { "nav-item" },
                    onclick: move |_| active_tab.set(tab),
                    span { class: "solar-icon nav-icon {tab.icon()}" }
                    span { "{tab.label()}" }
                }
            }
        }
    }
}
