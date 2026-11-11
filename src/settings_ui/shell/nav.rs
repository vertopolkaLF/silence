//! Sidebar tabs and the tab-switch state machine (ported from `gui/tabs.rs`).
//! A switch slides the outgoing and incoming panels for 300ms; clicks during
//! a slide queue only the latest destination.
//!
//! Microphone and Volume pages live in collapsible groups. Only one group is
//! expanded, and choosing a page inside a closed group opens it.
use crate::settings_ui::modules::ModuleId;
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
    VolumeWheel,
    VolumeOsd,
    VolumeMusic,
    VolumeTray,
    VolumeMediaKeys,
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
    /// Visual order. Drives the content slide direction.
    pub const ALL: &'static [Self] = &[
        Self::Hotkeys,
        Self::Devices,
        Self::Sounds,
        Self::Overlay,
        Self::TrayIcon,
        Self::HoldToMute,
        Self::AutoMute,
        Self::VolumeWheel,
        Self::VolumeOsd,
        Self::VolumeMusic,
        Self::VolumeTray,
        Self::VolumeMediaKeys,
        Self::General,
        Self::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::General => "General",
            Self::Devices => "Devices",
            Self::HoldToMute => "Hold to mute",
            Self::Hotkeys => "Hotkeys",
            Self::Sounds => "Sounds",
            Self::Overlay => "Overlay",
            Self::TrayIcon => "Tray icon",
            Self::AutoMute => "Auto mute",
            Self::About => "About",
            Self::VolumeWheel => "Wheel & clicks",
            Self::VolumeOsd => "OSD",
            Self::VolumeMusic => "Music",
            Self::VolumeTray => "Tray icon",
            Self::VolumeMediaKeys => "Media keys",
        }
    }

    /// Group that owns this page, if it is not a global page.
    pub fn module(self) -> Option<ModuleId> {
        match self {
            Self::Overlay | Self::TrayIcon | Self::HoldToMute | Self::AutoMute => {
                Some(ModuleId::Microphone)
            }
            Self::VolumeWheel
            | Self::VolumeOsd
            | Self::VolumeMusic
            | Self::VolumeTray
            | Self::VolumeMediaKeys => {
                Some(ModuleId::Volume)
            }
            Self::Hotkeys | Self::Devices | Self::Sounds | Self::General | Self::About => None,
        }
    }

    /// Outline icon (`assets/icons/<name>.svg`).
    pub fn icon(self) -> &'static str {
        match self {
            Self::General => "settings-linear",
            Self::Devices => "headphones-round-sound-linear",
            Self::HoldToMute => "oven-mitts-linear",
            Self::Hotkeys => "keyboard-linear",
            Self::Sounds => "music-note-2-linear",
            Self::Overlay => "monitor-linear",
            Self::TrayIcon => "widget-linear",
            Self::AutoMute => "clock-circle-linear",
            Self::About => "info-circle-linear",
            Self::VolumeWheel => "mouse-linear",
            Self::VolumeOsd => "slider-minimalistic-horizontal-linear",
            Self::VolumeMusic => "vinyl-linear",
            Self::VolumeTray => "widget-linear",
            Self::VolumeMediaKeys => "play-circle-linear",
        }
    }

    /// Filled icon shown while the tab is active.
    pub fn active_icon(self) -> &'static str {
        match self {
            Self::General => "settings-bold",
            Self::Devices => "headphones-round-sound-bold",
            Self::HoldToMute => "oven-mitts-bold",
            Self::Hotkeys => "keyboard-bold",
            Self::Sounds => "music-note-2-bold",
            Self::Overlay => "monitor-bold",
            Self::TrayIcon => "widget-bold",
            Self::AutoMute => "clock-circle-bold",
            Self::About => "info-circle-bold",
            Self::VolumeWheel => "mouse-bold",
            Self::VolumeOsd => "slider-minimalistic-horizontal-bold",
            Self::VolumeMusic => "vinyl-bold",
            Self::VolumeTray => "widget-bold",
            Self::VolumeMediaKeys => "play-circle-bold",
        }
    }

    pub fn sections(self) -> &'static [SettingsSection] {
        macro_rules! s {
            ($id:literal, $label:literal) => {
                SettingsSection {
                    id: $id,
                    label: $label,
                }
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
            Self::Sounds => &[
                s!("sounds-overview", "Microphone"),
                s!("sounds-volume", "Volume"),
                s!("sounds-files", "Files"),
            ],
            Self::Overlay => &[
                s!("overlay-overview", "Overlay"),
                s!("overlay-appearance", "Appearance"),
                s!("overlay-behaviour", "Behaviour"),
            ],
            Self::TrayIcon => &[
                s!("tray-icon-overview", "Tray Icon"),
                s!("tray-icon-mic-in-use", "Mic in use"),
            ],
            Self::AutoMute => &[s!("auto-mute-overview", "Auto mute")],
            Self::About => &[s!("about-overview", "About")],
            Self::VolumeWheel => &[s!("volume-wheel", "Wheel & clicks")],
            Self::VolumeOsd => &[s!("volume-osd", "OSD")],
            Self::VolumeMusic => &[s!("volume-music", "Music")],
            Self::VolumeTray => &[s!("volume-tray-icon", "Tray icon")],
            Self::VolumeMediaKeys => &[s!("volume-media-keys", "Media keys")],
        }
    }

    pub fn first_section_id(self) -> &'static str {
        self.sections()
            .first()
            .map(|section| section.id)
            .unwrap_or("")
    }

    pub fn order(self) -> usize {
        Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0)
    }
}

/// Pages inside a sidebar group, top to bottom.
pub(crate) fn module_tabs(module: ModuleId) -> &'static [SettingsTab] {
    match module {
        ModuleId::Microphone => &[
            SettingsTab::Overlay,
            SettingsTab::TrayIcon,
            SettingsTab::HoldToMute,
            SettingsTab::AutoMute,
        ],
        ModuleId::Volume => &[
            SettingsTab::VolumeWheel,
            SettingsTab::VolumeOsd,
            SettingsTab::VolumeMusic,
            SettingsTab::VolumeTray,
            SettingsTab::VolumeMediaKeys,
        ],
    }
}

pub(crate) struct Nav {
    pub active_tab: SettingsTab,
    pub displayed_tab: SettingsTab,
    pub active_section: &'static str,
    /// Microphone or Volume rows currently expanded. At most one.
    pub expanded_module: Option<ModuleId>,
    pub transition: Option<TabTransition>,
    pub transition_id: u64,
    pub pending_tab: Option<SettingsTab>,
    /// Earliest moment a queued tab may start after a finished slide.
    pub handoff_at: Option<Instant>,
    /// Scroll position of the panel being shown; a fresh panel starts at the top.
    pub scroll: ScrollHandle,
    /// Scroll handle kept for the outgoing panel during a slide.
    pub outgoing_scroll: ScrollHandle,
    /// Animated scroll in progress (section jump or mouse wheel).
    pub smooth_scroll: Option<SmoothScroll>,
}

/// Scroll offset animating from `from` to `to` (content pixels, positive down).
#[derive(Clone, Copy)]
pub(crate) struct SmoothScroll {
    pub from: f32,
    pub to: f32,
    pub started: Instant,
    pub duration_ms: f32,
    pub easing: crate::settings_ui::anim::Easing,
}

impl SmoothScroll {
    pub fn value(&self) -> f32 {
        let t = self.progress();
        self.from + (self.to - self.from) * self.easing.apply(t)
    }

    pub fn progress(&self) -> f32 {
        (self.started.elapsed().as_secs_f32() * 1000. / self.duration_ms).min(1.)
    }
}

impl Nav {
    pub fn new(tab: SettingsTab) -> Self {
        Self {
            active_tab: tab,
            displayed_tab: tab,
            active_section: tab.first_section_id(),
            expanded_module: tab.module().or(Some(ModuleId::Microphone)),
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
        if let Some(module) = next.module() {
            self.expanded_module = Some(module);
        }
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

    /// Opens a group, or closes it when it is already the open one.
    /// The other group closes so the sidebar never stacks both lists.
    pub fn toggle_module_group(&mut self, module: ModuleId) {
        self.expanded_module = if self.expanded_module == Some(module) {
            None
        } else {
            Some(module)
        };
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
