use dioxus::prelude::*;

const APP_ICON: Asset = asset!("/assets/app.png");

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SettingsSection {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    HoldToMute,
    Hotkeys,
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
        Self::Hotkeys,
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
            Self::Hotkeys => "Hotkeys",
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
            Self::Hotkeys => "icon-keyboard",
            Self::Sounds => "icon-volume",
            Self::Overlay => "icon-monitor",
            Self::TrayIcon => "icon-widget",
            Self::AutoMute => "icon-magic",
            Self::About => "icon-info",
        }
    }

    pub fn sections(self) -> &'static [SettingsSection] {
        match self {
            Self::General => &[SettingsSection {
                id: "general-status",
                label: "General",
            }],
            Self::HoldToMute => &[SettingsSection {
                id: "hold-to-mute-overview",
                label: "Hold to Mute",
            }],
            Self::Hotkeys => &[SettingsSection {
                id: "hotkeys-overview",
                label: "Hotkeys",
            }],
            Self::Sounds => &[SettingsSection {
                id: "sounds-overview",
                label: "Sounds",
            }],
            Self::Overlay => &[
                SettingsSection {
                    id: "overlay-overview",
                    label: "Overlay",
                },
                SettingsSection {
                    id: "overlay-appearance",
                    label: "Appearance",
                },
            ],
            Self::TrayIcon => &[SettingsSection {
                id: "tray-icon-overview",
                label: "Tray Icon",
            }],
            Self::AutoMute => &[SettingsSection {
                id: "auto-mute-overview",
                label: "Auto-Mute",
            }],
            Self::About => &[SettingsSection {
                id: "about-overview",
                label: "About",
            }],
        }
    }

    pub fn first_section_id(self) -> &'static str {
        self.sections()
            .first()
            .map(|section| section.id)
            .unwrap_or("")
    }
}

pub fn render(mut active_tab: Signal<SettingsTab>, mut active_section: Signal<String>) -> Element {
    rsx! {
        nav {
            class: "sidebar",
            div {
                class: "sidebar-brand",
                img {
                    class: "sidebar-brand-icon",
                    src: APP_ICON,
                    alt: "silence!"
                }
                span { "silence!" }
            }
            for &tab in SettingsTab::ALL {
                button {
                    class: if active_tab() == tab { "nav-item active" } else { "nav-item" },
                    onclick: move |_| {
                        active_tab.set(tab);
                        active_section.set(tab.first_section_id().to_string());
                        scroll_to_section(tab.first_section_id());
                    },
                    span { class: "solar-icon nav-icon {tab.icon()}" }
                    span { "{tab.label()}" }
                }

                if tab.sections().len() > 1 {
                    div {
                        class: if active_tab() == tab { "nav-subsections-shell open" } else { "nav-subsections-shell" },
                        div { class: "nav-subsections",
                            for section in tab.sections() {
                                button {
                                    class: if active_section() == section.id { "nav-subitem active" } else { "nav-subitem" },
                                    tabindex: if active_tab() == tab { "0" } else { "-1" },
                                    onclick: move |_| {
                                        scroll_to_section(section.id);
                                    },
                                    "{section.label}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn scroll_to_section(id: &str) {
    let script = format!(
        r#"
        requestAnimationFrame(() => requestAnimationFrame(() => {{
          const section = document.getElementById({id:?});
          if (section) {{
            section.scrollIntoView({{ behavior: 'smooth', block: 'start' }});
          }}
        }}));
        "#
    );
    let _ = dioxus::document::eval(&script);
}
