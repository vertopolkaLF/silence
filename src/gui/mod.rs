use dioxus::prelude::*;
use std::time::{Duration, Instant};

mod controls;
mod sections;
mod tabs;

use tabs::SettingsTab;

pub(crate) const APP_ICO: Asset = asset!("/assets/app.ico");
const CLOSE_ICON: Asset = asset!("/assets/icons/codicon_close.svg");
const INFO_CIRCLE_BOLD_ICON: Asset = asset!("/assets/icons/info-circle-bold.svg");
const KEYBOARD_BOLD_ICON: Asset = asset!("/assets/icons/keyboard-bold.svg");
const KEYBOARD_LINEAR_ICON: Asset = asset!("/assets/icons/keyboard-linear.svg");
const MAGIC_STICK_3_BOLD_ICON: Asset = asset!("/assets/icons/magic-stick-3-bold.svg");
const CONTROLS_CSS: Asset = asset!("/assets/styles/controls.css", AssetOptions::css());
const GENERAL_CSS: Asset = asset!("/assets/styles/general.css", AssetOptions::css());
const GEIST_FONT: Asset = asset!("/assets/fonts/Geist-VariableFont_wght.ttf");
const GLOBAL_CSS: Asset = asset!("/assets/styles/global.css", AssetOptions::css());
const HOTKEYS_CSS: Asset = asset!("/assets/styles/hotkeys.css", AssetOptions::css());
const LAYOUT_CSS: Asset = asset!("/assets/styles/layout.css", AssetOptions::css());
const MICROPHONE_3_BOLD_ICON: Asset = asset!("/assets/icons/microphone-3-bold.svg");
const MONITOR_BOLD_ICON: Asset = asset!("/assets/icons/monitor-bold.svg");
const OVERLAY_CSS: Asset = asset!("/assets/styles/overlay.css", AssetOptions::css());
const SOUNDS_CSS: Asset = asset!("/assets/styles/sounds.css", AssetOptions::css());
const SETTINGS_ICON: Asset = asset!("/assets/icons/codicon_settings-gear.svg");
const SETTINGS_BOLD_ICON: Asset = asset!("/assets/icons/settings-bold.svg");
const TABS_CSS: Asset = asset!("/assets/styles/tabs.css", AssetOptions::css());
const TITLEBAR_CSS: Asset = asset!("/assets/styles/titlebar.css", AssetOptions::css());
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
pub(crate) struct HotkeyModalRequest {
    pub action: crate::HotkeyAction,
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
<link rel="stylesheet" href="{SOUNDS_CSS}">
<link rel="stylesheet" href="{OVERLAY_CSS}">
<link rel="stylesheet" href="{HOTKEYS_CSS}">
<style>{theme_style}</style>
<style>{icon_style}</style>"#,
        settings_font_face()
    )
}

fn settings_font_face() -> String {
    format!(
        r#"@font-face {{
  font-family: "Geist";
  src: url("{GEIST_FONT}") format("truetype");
  font-weight: 400;
  font-style: normal;
  font-display: optional;
}}"#
    )
}

fn settings_icon_style() -> String {
    format!(
        r#".titlebar-settings {{ --titlebar-icon: url("{SETTINGS_ICON}"); }}
.titlebar-close {{ --titlebar-icon: url("{CLOSE_ICON}"); }}
.icon-close {{ --icon: url("{CLOSE_ICON}"); }}
.icon-keyboard {{ --icon: url("{KEYBOARD_LINEAR_ICON}"); }}
.icon-settings-bold {{ --icon: url("{SETTINGS_BOLD_ICON}"); }}
.icon-microphone-3-bold {{ --icon: url("{MICROPHONE_3_BOLD_ICON}"); }}
.icon-volume-loud-bold {{ --icon: url("{VOLUME_LOUD_BOLD_ICON}"); }}
.icon-monitor-bold {{ --icon: url("{MONITOR_BOLD_ICON}"); }}
.icon-widget-bold {{ --icon: url("{WIDGET_BOLD_ICON}"); }}
.icon-magic-stick-3-bold {{ --icon: url("{MAGIC_STICK_3_BOLD_ICON}"); }}
.icon-info-circle-bold {{ --icon: url("{INFO_CIRCLE_BOLD_ICON}"); }}
.icon-keyboard-bold {{ --icon: url("{KEYBOARD_BOLD_ICON}"); }}"#
    )
}

pub fn update_settings(
    mut settings: Signal<SettingsSnapshot>,
    update: impl FnOnce(&mut crate::Config),
) {
    let mut config = crate::load_config().unwrap_or_else(|_| settings().config);
    let startup_was_enabled = config.startup.launch_on_startup;
    update(&mut config);
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
    let drag_desktop = desktop.clone();
    let devtools_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let reveal_desktop = desktop.clone();
    let mut settings = use_signal(SettingsSnapshot::load);
    let active_tab = use_signal(|| SettingsTab::General);
    let active_section = use_signal(|| SettingsTab::General.first_section_id().to_string());
    let hotkey_modal_request = use_signal(|| None::<HotkeyModalRequest>);
    let recording = use_signal(|| false);
    let mut closing = use_signal(|| false);
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

    rsx! {
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
                {tabs::render(active_tab, active_section)}
                main {
                    class: "content",
                    onscroll: move |_| {
                        update_active_section(active_section);
                    },
                    {sections::render(
                        active_tab(),
                        settings,
                        recording,
                        active_tab,
                        active_section,
                        hotkey_modal_request,
                    )}
                }
            }
        }
    }
}

fn update_active_section(mut active_section: Signal<String>) {
    spawn(async move {
        let script = r#"
        const content = document.querySelector('.content');
        const sections = [...document.querySelectorAll('[data-settings-section]')];
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
