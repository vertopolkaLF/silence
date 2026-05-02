use dioxus::prelude::*;
use std::time::{Duration, Instant};

const APP_ICON: Asset = asset!("/assets/app.png");
const TAB_TRANSITION_DURATION: Duration = Duration::from_millis(300);
const TAB_TRANSITION_HANDOFF_DELAY: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SettingsSection {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    General,
    Devices,
    HoldToMute,
    Hotkeys,
    Sounds,
    Overlay,
    TrayIcon,
    AutoMute,
    Advanced,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabSlideDirection {
    Left,
    Right,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct TabTransition {
    pub id: u64,
    pub from: SettingsTab,
    pub to: SettingsTab,
    pub direction: TabSlideDirection,
    pub started_at: Instant,
}

impl SettingsTab {
    const ALL: &'static [Self] = &[
        Self::General,
        Self::Devices,
        Self::Hotkeys,
        Self::Sounds,
        Self::Overlay,
        Self::HoldToMute,
        Self::TrayIcon,
        Self::AutoMute,
        Self::Advanced,
        Self::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Devices => "Devices",
            Self::HoldToMute => "Hold to Mute",
            Self::Hotkeys => "Hotkeys",
            Self::Sounds => "Sounds",
            Self::Overlay => "Overlay",
            Self::TrayIcon => "Tray Icon",
            Self::AutoMute => "Auto-Mute",
            Self::Advanced => "Advanced",
            Self::About => "About",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::General => "icon-settings",
            Self::Devices => "icon-microphone",
            Self::HoldToMute => "icon-oven-mitts",
            Self::Hotkeys => "icon-keyboard",
            Self::Sounds => "icon-volume",
            Self::Overlay => "icon-monitor",
            Self::TrayIcon => "icon-widget",
            Self::AutoMute => "icon-clock-circle",
            Self::Advanced => "icon-magic",
            Self::About => "icon-info",
        }
    }

    fn active_icon(self) -> &'static str {
        match self {
            Self::General => "icon-settings-bold",
            Self::Devices => "icon-microphone-3-bold",
            Self::HoldToMute => "icon-oven-mitts-bold",
            Self::Hotkeys => "icon-keyboard-bold",
            Self::Sounds => "icon-volume-loud-bold",
            Self::Overlay => "icon-monitor-bold",
            Self::TrayIcon => "icon-widget-bold",
            Self::AutoMute => "icon-clock-circle-bold",
            Self::Advanced => "icon-magic-stick-3-bold",
            Self::About => "icon-info-circle-bold",
        }
    }

    pub fn sections(self) -> &'static [SettingsSection] {
        match self {
            Self::General => &[SettingsSection {
                id: "general-status",
                label: "General",
            }],
            Self::Devices => &[SettingsSection {
                id: "devices-overview",
                label: "Devices",
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
            Self::Advanced => &[SettingsSection {
                id: "advanced-overview",
                label: "Advanced",
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

    fn order(self) -> usize {
        match self {
            Self::General => 0,
            Self::Devices => 1,
            Self::Hotkeys => 2,
            Self::Sounds => 3,
            Self::Overlay => 4,
            Self::HoldToMute => 5,
            Self::TrayIcon => 6,
            Self::AutoMute => 7,
            Self::Advanced => 8,
            Self::About => 9,
        }
    }
}

pub fn render(
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
) -> Element {
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
                        select_tab(
                            tab,
                            active_tab,
                            active_section,
                            displayed_tab,
                            transition,
                            transition_id,
                            pending_tab,
                        );
                    },
                    span { class: "nav-icon-stack",
                        span { class: "solar-icon nav-icon nav-icon-line {tab.icon()}" }
                        span { class: "solar-icon nav-icon nav-icon-filled {tab.active_icon()}" }
                    }
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

pub(crate) fn navigate_to_tab(
    next_tab: SettingsTab,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
) {
    select_tab(
        next_tab,
        active_tab,
        active_section,
        displayed_tab,
        transition,
        transition_id,
        pending_tab,
    );
}

fn select_tab(
    next_tab: SettingsTab,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    mut pending_tab: Signal<Option<SettingsTab>>,
) {
    if let Some(current) = transition() {
        if current.to == next_tab {
            return;
        }
        // While a swipe is active, we keep only one queued destination:
        // the latest user intent always replaces any older queued tab.
        pending_tab.set(Some(next_tab));
        return;
    }

    if active_tab() == next_tab {
        return;
    }

    start_transition(
        next_tab,
        active_tab,
        active_section,
        displayed_tab,
        transition,
        transition_id,
        pending_tab,
    );
}

fn start_transition(
    next_tab: SettingsTab,
    mut active_tab: Signal<SettingsTab>,
    mut active_section: Signal<String>,
    mut displayed_tab: Signal<SettingsTab>,
    mut transition: Signal<Option<TabTransition>>,
    mut transition_id: Signal<u64>,
    mut pending_tab: Signal<Option<SettingsTab>>,
) {
    let previous_tab = displayed_tab();
    active_tab.set(next_tab);
    active_section.set(next_tab.first_section_id().to_string());
    pending_tab.set(None);

    if previous_tab == next_tab {
        displayed_tab.set(next_tab);
        transition.set(None);
        return;
    }

    let direction = if next_tab.order() > previous_tab.order() {
        TabSlideDirection::Left
    } else {
        TabSlideDirection::Right
    };

    let next_transition_id = transition_id().wrapping_add(1);
    transition_id.set(next_transition_id);
    transition.set(Some(TabTransition {
        id: next_transition_id,
        from: previous_tab,
        to: next_tab,
        direction,
        started_at: Instant::now(),
    }));
}

pub(crate) fn process_transition_tick(
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
) {
    let Some(current) = transition() else {
        return;
    };

    if current.started_at.elapsed() < TAB_TRANSITION_DURATION {
        return;
    }

    if transition_id() != current.id {
        return;
    }

    finish_transition(
        current.to,
        active_tab,
        active_section,
        displayed_tab,
        transition,
        transition_id,
        pending_tab,
    );
}

fn finish_transition(
    completed_tab: SettingsTab,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    mut displayed_tab: Signal<SettingsTab>,
    mut transition: Signal<Option<TabTransition>>,
    transition_id: Signal<u64>,
    mut pending_tab: Signal<Option<SettingsTab>>,
) {
    displayed_tab.set(completed_tab);
    transition.set(None);

    let Some(next_tab) = pending_tab() else {
        return;
    };

    if next_tab == completed_tab {
        pending_tab.set(None);
        return;
    }

    spawn(async move {
        tokio::time::sleep(TAB_TRANSITION_HANDOFF_DELAY).await;

        if transition().is_some() {
            return;
        }

        if displayed_tab() != completed_tab {
            return;
        }

        let Some(queued_tab) = pending_tab() else {
            return;
        };

        // If the user clicked again during the handoff frame, use only the
        // newest queued tab and discard the older intermediate destination.
        if queued_tab != next_tab || queued_tab == completed_tab {
            return;
        }

        start_transition(
            queued_tab,
            active_tab,
            active_section,
            displayed_tab,
            transition,
            transition_id,
            pending_tab,
        );
    });
}

pub(crate) fn scroll_to_section(id: &str) {
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
