//! Sidebar tabs and the tab-switch state machine (ported from `gui/tabs.rs`).
//! A switch slides the outgoing and incoming panels for 300ms; clicks during
//! a slide queue only the latest destination.
use gpui::ScrollHandle;
use std::time::{Duration, Instant};

pub(crate) const TAB_TRANSITION_DURATION: Duration = Duration::from_millis(300);
pub(crate) const TAB_TRANSITION_HANDOFF_DELAY: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct SettingsSection {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum SettingsTab {
    Hotkeys,
    General,
    Devices,
    HoldToMute,
    Sounds,
    Overlay,
    TrayIcon,
    AutoMute,
    About,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabSlideDirection {
    /// The incoming panel enters from the right.
    Left,
    /// The incoming panel enters from the left.
    Right,
}

#[derive(Clone, Copy)]
pub(crate) struct TabTransition {
    pub id: u64,
    pub from: SettingsTab,
    pub to: SettingsTab,
    pub direction: TabSlideDirection,
    pub started_at: Instant,
}

impl SettingsTab {
    pub const ALL: &'static [Self] = &[
        Self::Hotkeys,
        Self::Devices,
        Self::Sounds,
        Self::Overlay,
        Self::HoldToMute,
        Self::TrayIcon,
        Self::AutoMute,
        Self::General,
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
            Self::About => "About",
        }
    }

    /// Outline icon (`assets/icons/<name>.svg`).
    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "settings-linear",
            Self::Devices => "microphone-3-linear",
            Self::HoldToMute => "oven-mitts-linear",
            Self::Hotkeys => "keyboard-linear",
            Self::Sounds => "volume-loud-linear",
            Self::Overlay => "monitor-linear",
            Self::TrayIcon => "widget-linear",
            Self::AutoMute => "clock-circle-linear",
            Self::About => "info-circle-linear",
        }
    }

    /// Filled icon shown while the tab is active.
    pub fn active_icon(self) -> &'static str {
        match self {
            Self::General => "settings-bold",
            Self::Devices => "microphone-3-bold",
            Self::HoldToMute => "oven-mitts-bold",
            Self::Hotkeys => "keyboard-bold",
            Self::Sounds => "volume-loud-bold",
            Self::Overlay => "monitor-bold",
            Self::TrayIcon => "widget-bold",
            Self::AutoMute => "clock-circle-bold",
            Self::About => "info-circle-bold",
        }
    }

    pub fn sections(self) -> &'static [SettingsSection] {
        macro_rules! s {
            ($id:literal, $label:literal) => {
                SettingsSection { id: $id, label: $label }
            };
        }
        match self {
            Self::General => &[
                s!("general-status", "General"),
                s!("general-import-export", "Import/Export"),
            ],
            Self::Devices => &[s!("devices-overview", "Devices")],
            Self::HoldToMute => &[s!("hold-to-mute-overview", "Hold to Mute")],
            Self::Hotkeys => &[s!("hotkeys-overview", "Hotkeys")],
            Self::Sounds => &[s!("sounds-overview", "Sounds")],
            Self::Overlay => &[
                s!("overlay-overview", "Overlay"),
                s!("overlay-appearance", "Appearance"),
                s!("overlay-behaviour", "Behaviour"),
            ],
            Self::TrayIcon => &[
                s!("tray-icon-overview", "Tray Icon"),
                s!("tray-icon-mic-in-use", "Mic in use"),
            ],
            Self::AutoMute => &[s!("auto-mute-overview", "Auto-Mute")],
            Self::About => &[s!("about-overview", "About")],
        }
    }

    pub fn first_section_id(self) -> &'static str {
        self.sections().first().map(|section| section.id).unwrap_or("")
    }

    pub fn order(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }
}

pub(crate) struct Nav {
    pub active_tab: SettingsTab,
    pub displayed_tab: SettingsTab,
    pub active_section: &'static str,
    pub transition: Option<TabTransition>,
    pub transition_id: u64,
    pub pending_tab: Option<SettingsTab>,
    /// Earliest moment a queued tab may start after a finished slide.
    pub handoff_at: Option<Instant>,
    /// Scroll position of the panel being shown; a fresh panel starts at the top.
    pub scroll: ScrollHandle,
    /// Scroll handle kept for the outgoing panel during a slide.
    pub outgoing_scroll: ScrollHandle,
    /// Smooth scroll toward a section: (start offset, target offset, start time).
    pub smooth_scroll: Option<(f32, f32, Instant)>,
}

impl Nav {
    pub fn new(tab: SettingsTab) -> Self {
        Self {
            active_tab: tab,
            displayed_tab: tab,
            active_section: tab.first_section_id(),
            transition: None,
            transition_id: 0,
            pending_tab: None,
            handoff_at: None,
            scroll: ScrollHandle::new(),
            outgoing_scroll: ScrollHandle::new(),
            smooth_scroll: None,
        }
    }

    /// Sidebar click / programmatic navigation.
    pub fn select(&mut self, next: SettingsTab) {
        if let Some(current) = self.transition {
            if current.to != next {
                // The latest user intent replaces any older queued tab.
                self.pending_tab = Some(next);
            }
            return;
        }
        if self.active_tab == next {
            return;
        }
        self.start(next);
    }

    fn start(&mut self, next: SettingsTab) {
        let previous = self.displayed_tab;
        self.active_tab = next;
        self.active_section = next.first_section_id();
        self.pending_tab = None;
        self.handoff_at = None;
        self.smooth_scroll = None;
        if previous == next {
            self.transition = None;
            return;
        }
        let direction = if next.order() > previous.order() {
            TabSlideDirection::Left
        } else {
            TabSlideDirection::Right
        };
        self.transition_id = self.transition_id.wrapping_add(1);
        self.outgoing_scroll = std::mem::replace(&mut self.scroll, ScrollHandle::new());
        self.transition = Some(TabTransition {
            id: self.transition_id,
            from: previous,
            to: next,
            direction,
            started_at: Instant::now(),
        });
    }

    /// Advances the state machine; returns true while anything is animating.
    pub fn tick(&mut self) -> bool {
        if let Some(current) = self.transition {
            if current.started_at.elapsed() < TAB_TRANSITION_DURATION {
                return true;
            }
            self.displayed_tab = current.to;
            self.transition = None;
            match self.pending_tab {
                Some(next) if next != current.to => {
                    self.handoff_at = Some(Instant::now() + TAB_TRANSITION_HANDOFF_DELAY);
                    return true;
                }
                _ => self.pending_tab = None,
            }
        }
        if let Some(at) = self.handoff_at {
            if Instant::now() < at {
                return true;
            }
            self.handoff_at = None;
            if let Some(next) = self.pending_tab.take() {
                if next != self.displayed_tab {
                    self.start(next);
                    return true;
                }
            }
        }
        false
    }

    /// Slide progress in 0..=1 for the current transition.
    pub fn progress(&self) -> Option<(TabTransition, f32)> {
        self.transition.map(|transition| {
            let t = transition.started_at.elapsed().as_secs_f32()
                / TAB_TRANSITION_DURATION.as_secs_f32();
            (transition, t.min(1.))
        })
    }
}
