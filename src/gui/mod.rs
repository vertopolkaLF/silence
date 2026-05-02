#[cfg(target_os = "windows")]
use dioxus::desktop::tao::platform::windows::WindowExtWindows;
use dioxus::prelude::*;
use std::time::{Duration, Instant};

mod controls;
mod sections;
mod tabs;

use tabs::{SettingsTab, TabSlideDirection, TabTransition};

pub(crate) const APP_ICO: Asset = asset!("/assets/app.ico");
const ABOUT_CSS: Asset = asset!("/assets/styles/about.css", AssetOptions::css());
const CLOCK_CIRCLE_BOLD_ICON: Asset = asset!("/assets/icons/clock-circle-bold.svg");
const CLOCK_CIRCLE_LINEAR_ICON: Asset = asset!("/assets/icons/clock-circle-linear.svg");
const CLOSE_ICON: Asset = asset!("/assets/icons/codicon_close.svg");
const INFO_CIRCLE_BOLD_ICON: Asset = asset!("/assets/icons/info-circle-bold.svg");
const KEYBOARD_BOLD_ICON: Asset = asset!("/assets/icons/keyboard-bold.svg");
const KEYBOARD_LINEAR_ICON: Asset = asset!("/assets/icons/keyboard-linear.svg");
const MAGIC_STICK_3_BOLD_ICON: Asset = asset!("/assets/icons/magic-stick-3-bold.svg");
const BRICOLAGE_GROTESQUE_FONT: Asset = asset!("/assets/fonts/BricolageGrotesque-latin.woff2");
const PLUS_JAKARTA_SANS_FONT: Asset = asset!("/assets/fonts/PlusJakartaSans-latin.woff2");
const CONTROLS_CSS: Asset = asset!("/assets/styles/controls.css", AssetOptions::css());
const GENERAL_CSS: Asset = asset!("/assets/styles/general.css", AssetOptions::css());
const CONTRAST_ICON: Asset = asset!("/assets/icons/ic-baseline-contrast.svg");
const INTER_FONT: Asset = asset!("/assets/fonts/InterVariable.ttf");
const GLOBAL_CSS: Asset = asset!("/assets/styles/global.css", AssetOptions::css());
const HOTKEYS_CSS: Asset = asset!("/assets/styles/hotkeys.css", AssetOptions::css());
const LAYOUT_CSS: Asset = asset!("/assets/styles/layout.css", AssetOptions::css());
const MICROPHONE_3_BOLD_ICON: Asset = asset!("/assets/icons/microphone-3-bold.svg");
const MOON_LINEAR_ICON: Asset = asset!("/assets/icons/moon-linear.svg");
const MONITOR_BOLD_ICON: Asset = asset!("/assets/icons/monitor-bold.svg");
const OVEN_MITTS_BOLD_ICON: Asset = asset!("/assets/icons/oven-mitts-bold.svg");
const OVEN_MITTS_LINEAR_ICON: Asset = asset!("/assets/icons/oven-mitts-linear.svg");
const OVERLAY_CSS: Asset = asset!("/assets/styles/overlay.css", AssetOptions::css());
const PALLETE_2_LINEAR_ICON: Asset = asset!("/assets/icons/pallete-2-linear.svg");
const PAUSE_BOLD_ICON: Asset = asset!("/assets/icons/pause-bold.svg");
const PLAY_BOLD_ICON: Asset = asset!("/assets/icons/play-bold.svg");
const PLUS_LINEAR_ICON: Asset = asset!("/assets/icons/plus-linear.svg");
const RECORD_BOLD_ICON: Asset = asset!("/assets/icons/record-bold.svg");
const SOUNDS_CSS: Asset = asset!("/assets/styles/sounds.css", AssetOptions::css());
const SETTINGS_ICON: Asset = asset!("/assets/icons/codicon_settings-gear.svg");
const SETTINGS_BOLD_ICON: Asset = asset!("/assets/icons/settings-bold.svg");
const SUN_2_LINEAR_ICON: Asset = asset!("/assets/icons/sun-2-linear.svg");
const TABS_CSS: Asset = asset!("/assets/styles/tabs.css", AssetOptions::css());
const TITLEBAR_CSS: Asset = asset!("/assets/styles/titlebar.css", AssetOptions::css());
const TRASH_BIN_TRASH_LINEAR_ICON: Asset = asset!("/assets/icons/trash-bin-trash-linear.svg");
const VOLUME_LOUD_BOLD_ICON: Asset = asset!("/assets/icons/volume-loud-bold.svg");
const WIDGET_BOLD_ICON: Asset = asset!("/assets/icons/widget-bold.svg");
const DEVICE_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub config: crate::Config,
    pub devices: Vec<crate::MicDevice>,
    pub muted: bool,
}

#[derive(Clone, PartialEq)]
pub(crate) enum HotkeyModalRequest {
    Add {
        preset_action: Option<crate::HotkeyAction>,
    },
    Edit {
        binding: crate::HotkeyBinding,
    },
}

impl SettingsSnapshot {
    fn load() -> Self {
        Self {
            config: crate::load_config().unwrap_or_default(),
            devices: Vec::new(),
            muted: false,
        }
    }

    fn refresh(mut self, refresh_devices: bool) -> Self {
        self.config = crate::load_config().unwrap_or_else(|_| self.config);
        if refresh_devices {
            self.devices = crate::capture_devices().unwrap_or_default();
        }
        self.muted = crate::mic_mute_state(None).unwrap_or(self.muted);
        Self {
            config: self.config,
            devices: self.devices,
            muted: self.muted,
        }
    }
}

pub(crate) fn settings_startup_head() -> String {
    let theme_style = crate::WindowsAccent::load().css_vars();
    let icon_style = settings_icon_style();
    format!(
        r#"<link rel="icon" href="{APP_ICO}" type="image/x-icon">
<style>
html, body, #main {{
  margin: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: rgb(18, 18, 18);
  color: rgb(251, 251, 251);
}}
#main, .window {{
  width: 100vw;
  height: 100vh;
  overflow: hidden;
}}
</style>
<style>{}</style>
<link rel="stylesheet" href="{GLOBAL_CSS}">
<link rel="stylesheet" href="{CONTROLS_CSS}">
<link rel="stylesheet" href="{LAYOUT_CSS}">
<link rel="stylesheet" href="{TITLEBAR_CSS}">
<link rel="stylesheet" href="{TABS_CSS}">
<link rel="stylesheet" href="{GENERAL_CSS}">
<link rel="stylesheet" href="{ABOUT_CSS}">
<link rel="stylesheet" href="{SOUNDS_CSS}">
<link rel="stylesheet" href="{OVERLAY_CSS}">
<link rel="stylesheet" href="{HOTKEYS_CSS}">
<style>{theme_style}</style>
<style>{icon_style}</style>
<script>
(() => {{
  const isHotkeyRecording = () =>
    Boolean(document.querySelector('.hotkey-editor-panel .shortcut-display.recording'));

  const suppressRecordedShortcutDefaults = (event) => {{
    if (!isHotkeyRecording()) {{
      return;
    }}
    event.preventDefault();
  }};

  window.addEventListener('keydown', suppressRecordedShortcutDefaults, true);
  window.addEventListener('keyup', suppressRecordedShortcutDefaults, true);
  window.addEventListener('keypress', suppressRecordedShortcutDefaults, true);
}})();
</script>"#,
        settings_font_face()
    )
}

fn settings_font_face() -> String {
    format!(
        r#"@font-face {{
  font-family: "Bricolage Grotesque";
  src: url("{BRICOLAGE_GROTESQUE_FONT}") format("woff2");
  font-weight: 400 800;
  font-style: normal;
  font-display: block;
}}

@font-face {{
  font-family: "Plus Jakarta Sans";
  src: url("{PLUS_JAKARTA_SANS_FONT}") format("woff2");
  font-weight: 400 600;
  font-style: normal;
  font-display: block;
}}

@font-face {{
  font-family: "Inter";
  src: url("{INTER_FONT}") format("truetype");
  font-weight: 100 900;
  font-style: normal;
  font-display: swap;
}}"#
    )
}

fn settings_icon_style() -> String {
    format!(
        r#".titlebar-settings {{ --titlebar-icon: url("{SETTINGS_ICON}"); }}
.titlebar-close {{ --titlebar-icon: url("{CLOSE_ICON}"); }}
.icon-clock-circle {{ --icon: url("{CLOCK_CIRCLE_LINEAR_ICON}"); }}
.icon-close {{ --icon: url("{CLOSE_ICON}"); }}
.icon-keyboard {{ --icon: url("{KEYBOARD_LINEAR_ICON}"); }}
.icon-oven-mitts {{ --icon: url("{OVEN_MITTS_LINEAR_ICON}"); }}
.icon-clock-circle-bold {{ --icon: url("{CLOCK_CIRCLE_BOLD_ICON}"); }}
.icon-settings-bold {{ --icon: url("{SETTINGS_BOLD_ICON}"); }}
.icon-microphone-3-bold {{ --icon: url("{MICROPHONE_3_BOLD_ICON}"); }}
.icon-oven-mitts-bold {{ --icon: url("{OVEN_MITTS_BOLD_ICON}"); }}
.icon-volume-loud-bold {{ --icon: url("{VOLUME_LOUD_BOLD_ICON}"); }}
.icon-monitor-bold {{ --icon: url("{MONITOR_BOLD_ICON}"); }}
.icon-widget-bold {{ --icon: url("{WIDGET_BOLD_ICON}"); }}
.icon-magic-stick-3-bold {{ --icon: url("{MAGIC_STICK_3_BOLD_ICON}"); }}
.icon-info-circle-bold {{ --icon: url("{INFO_CIRCLE_BOLD_ICON}"); }}
.icon-keyboard-bold {{ --icon: url("{KEYBOARD_BOLD_ICON}"); }}
.icon-pause {{ --icon: url("{PAUSE_BOLD_ICON}"); }}
.icon-play {{ --icon: url("{PLAY_BOLD_ICON}"); }}
.icon-plus {{ --icon: url("{PLUS_LINEAR_ICON}"); }}
.icon-record {{ --icon: url("{RECORD_BOLD_ICON}"); }}
.icon-palette {{ --icon: url("{PALLETE_2_LINEAR_ICON}"); }}
.icon-contrast {{ --icon: url("{CONTRAST_ICON}"); }}
.icon-moon {{ --icon: url("{MOON_LINEAR_ICON}"); }}
.icon-sun {{ --icon: url("{SUN_2_LINEAR_ICON}"); }}
.icon-trash {{ --icon: url("{TRASH_BIN_TRASH_LINEAR_ICON}"); }}"#
    )
}

pub fn update_settings(
    mut settings: Signal<SettingsSnapshot>,
    update: impl FnOnce(&mut crate::Config),
) {
    let mut config = crate::load_config().unwrap_or_else(|_| settings().config);
    let startup_was_enabled = config.startup.launch_on_startup;
    update(&mut config);
    crate::normalize_hotkeys(&mut config.hotkeys);
    let _ = crate::save_config(&config);
    if config.startup.launch_on_startup != startup_was_enabled {
        let _ = crate::sync_startup_registration(config.startup.launch_on_startup);
    }
    crate::apply_live_config(&config, crate::config_modified_time());
    let next = SettingsSnapshot {
        config,
        devices: settings.peek().devices.clone(),
        muted: settings.peek().muted,
    };
    settings.set(next.refresh(false));
}

pub fn settings_app() -> Element {
    let desktop = dioxus::desktop::use_window();
    #[cfg(target_os = "windows")]
    use_hook({
        let desktop = desktop.clone();
        move || crate::install_settings_window_guard(desktop.hwnd())
    });
    let drag_desktop = desktop.clone();
    let devtools_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let reveal_desktop = desktop.clone();
    let mut settings = use_signal(SettingsSnapshot::load);
    let active_tab = use_signal(|| SettingsTab::General);
    let displayed_tab = use_signal(|| SettingsTab::General);
    let active_section = use_signal(|| SettingsTab::General.first_section_id().to_string());
    let tab_transition = use_signal(|| None::<TabTransition>);
    let tab_transition_id = use_signal(|| 0_u64);
    let pending_tab = use_signal(|| None::<SettingsTab>);
    let mut hotkey_modal_request = use_signal(|| None::<HotkeyModalRequest>);
    let mut pending_hotkey_modal_after_nav = use_signal(|| None::<HotkeyModalRequest>);
    let recording = use_signal(|| false);
    let open_select = use_signal(|| None::<String>);
    let mut closing = use_signal(|| false);
    use_context_provider(|| open_select);
    use_future(move || {
        let reveal_desktop = reveal_desktop.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(32)).await;
            reveal_desktop.set_visible(true);
            reveal_desktop.set_focus();
        }
    });
    use_future(move || async move {
        tokio::time::sleep(Duration::from_millis(16)).await;
        let mut last_device_refresh = Instant::now() - DEVICE_REFRESH_INTERVAL;
        loop {
            let refresh_devices = last_device_refresh.elapsed() >= DEVICE_REFRESH_INTERVAL;
            if refresh_devices {
                last_device_refresh = Instant::now();
            }
            let next = settings.peek().clone().refresh(refresh_devices);
            if *settings.peek() != next {
                settings.set(next);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    use_future(move || async move {
        loop {
            tabs::process_transition_tick(
                active_tab,
                active_section,
                displayed_tab,
                tab_transition,
                tab_transition_id,
                pending_tab,
            );

            if tab_transition().is_none() && displayed_tab() == SettingsTab::Hotkeys {
                if let Some(request) = pending_hotkey_modal_after_nav() {
                    hotkey_modal_request.set(Some(request));
                    pending_hotkey_modal_after_nav.set(None);
                }
            }

            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    });

    rsx! {
        style { {settings_font_face()} }
        div {
            class: if closing() { "window closing" } else { "window" },
            div {
                class: "titlebar",
                onmousedown: move |_| drag_desktop.drag(),
                div { class: "title-spacer" }
                if cfg!(debug_assertions) {
                    button {
                        class: "titlebar-button devtools-button",
                        id: "devtools",
                        title: "Open DevTools",
                        onmousedown: move |evt| evt.stop_propagation(),
                        onclick: move |_| devtools_desktop.devtool(),
                        span { class: "titlebar-glyph titlebar-settings" }
                    }
                }
                button {
                    class: "titlebar-button",
                    id: "close",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| {
                        if closing() {
                            return;
                        }
                        closing.set(true);
                        update_settings(settings, |config| {
                            config.hotkeys_paused = false;
                        });
                        let close_desktop = close_desktop.clone();
                        spawn(async move {
                            tokio::time::sleep(Duration::from_millis(120)).await;
                            close_desktop.set_visible(false);
                            close_desktop.close();
                        });
                    },
                    span { class: "titlebar-glyph titlebar-close" }
                }
            }

            div {
                class: "body",
                {tabs::render(
                    active_tab,
                    active_section,
                    displayed_tab,
                    tab_transition,
                    tab_transition_id,
                    pending_tab,
                )}
                main {
                    class: "content",
                    if let Some(transition) = tab_transition() {
                        ContentPanel {
                            key: "tab-outgoing-{transition.id}-{transition.from.label()}",
                            tab: transition.from,
                            panel_class: transition_panel_class("outgoing", transition.direction),
                            active_panel: false,
                            settings,
                            recording,
                            active_tab,
                            active_section,
                            displayed_tab,
                            tab_transition,
                            tab_transition_id,
                            pending_tab,
                            hotkey_modal_request,
                            pending_hotkey_modal_after_nav,
                        }
                        ContentPanel {
                            key: "tab-{transition.to.label()}",
                            tab: transition.to,
                            panel_class: transition_panel_class("incoming current", transition.direction),
                            active_panel: true,
                            settings,
                            recording,
                            active_tab,
                            active_section,
                            displayed_tab,
                            tab_transition,
                            tab_transition_id,
                            pending_tab,
                            hotkey_modal_request,
                            pending_hotkey_modal_after_nav,
                        }
                    } else {
                        ContentPanel {
                            key: "tab-{displayed_tab().label()}",
                            tab: displayed_tab(),
                            panel_class: "content-panel current resting".to_string(),
                            active_panel: true,
                            settings,
                            recording,
                            active_tab,
                            active_section,
                            displayed_tab,
                            tab_transition,
                            tab_transition_id,
                            pending_tab,
                            hotkey_modal_request,
                            pending_hotkey_modal_after_nav,
                        }
                    }
                }
            }
            {sections::hotkey_modal_host(settings, hotkey_modal_request)}
        }
    }
}

#[component]
fn ContentPanel(
    tab: SettingsTab,
    panel_class: String,
    active_panel: bool,
    settings: Signal<SettingsSnapshot>,
    recording: Signal<bool>,
    active_tab: Signal<SettingsTab>,
    active_section: Signal<String>,
    displayed_tab: Signal<SettingsTab>,
    tab_transition: Signal<Option<TabTransition>>,
    tab_transition_id: Signal<u64>,
    pending_tab: Signal<Option<SettingsTab>>,
    hotkey_modal_request: Signal<Option<HotkeyModalRequest>>,
    pending_hotkey_modal_after_nav: Signal<Option<HotkeyModalRequest>>,
) -> Element {
    rsx! {
        div { class: "{panel_class}",
            div {
                class: "content-scroll",
                "data-active-panel": if active_panel { "true" } else { "false" },
                onscroll: move |_| {
                    if active_panel {
                        update_active_section(active_section);
                    }
                },
                div { class: "content-inner",
                    {sections::render(
                        tab,
                        settings,
                        recording,
                        active_tab,
                        active_section,
                        displayed_tab,
                        tab_transition,
                        tab_transition_id,
                        pending_tab,
                        hotkey_modal_request,
                        pending_hotkey_modal_after_nav,
                    )}
                }
            }
        }
    }
}

fn transition_panel_class(role: &str, direction: TabSlideDirection) -> String {
    let movement = match (role, direction) {
        ("outgoing", TabSlideDirection::Left) => "exit-to-left",
        ("outgoing", TabSlideDirection::Right) => "exit-to-right",
        (_, TabSlideDirection::Left) => "enter-from-right",
        (_, TabSlideDirection::Right) => "enter-from-left",
    };
    format!("content-panel {role} {movement}")
}

fn update_active_section(mut active_section: Signal<String>) {
    spawn(async move {
        let script = r#"
        const content = document.querySelector('.content-scroll[data-active-panel="true"]');
        const sections = [...content?.querySelectorAll('[data-settings-section]') ?? []];
        if (!content || sections.length === 0) {
          return '';
        }

        const top = content.getBoundingClientRect().top;
        let active = sections[0];
        for (const section of sections) {
          if (section.getBoundingClientRect().top - top <= 96) {
            active = section;
          }
        }
        return active.id || '';
        "#;

        if let Ok(id) = dioxus::document::eval(script).await {
            if let Some(id) = id.as_str() {
                if !id.is_empty() {
                    active_section.set(id.to_string());
                }
            }
        }
    });
}
