use dioxus::prelude::*;
use std::time::Duration;

mod sections;
mod tabs;

use tabs::SettingsTab;

const APP_ICO: Asset = asset!("/assets/app.ico");
const CLOSE_ICON: Asset = asset!("/assets/icons/codicon_close.svg");
const GENERAL_CSS: Asset = asset!("/assets/styles/general.css", AssetOptions::css());
const GEIST_FONT: Asset = asset!("/assets/fonts/Geist-VariableFont_wght.ttf");
const GLOBAL_CSS: Asset = asset!("/assets/styles/global.css", AssetOptions::css());
const LAYOUT_CSS: Asset = asset!("/assets/styles/layout.css", AssetOptions::css());
const SOUNDS_CSS: Asset = asset!("/assets/styles/sounds.css", AssetOptions::css());
const SETTINGS_ICON: Asset = asset!("/assets/icons/codicon_settings-gear.svg");
const TABS_CSS: Asset = asset!("/assets/styles/tabs.css", AssetOptions::css());
const TITLEBAR_CSS: Asset = asset!("/assets/styles/titlebar.css", AssetOptions::css());

#[derive(Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub config: crate::Config,
    pub devices: Vec<crate::MicDevice>,
    pub muted: bool,
}

impl SettingsSnapshot {
    fn load() -> Self {
        Self::from_config(crate::load_config().unwrap_or_default())
    }

    fn from_config(config: crate::Config) -> Self {
        let devices = crate::capture_devices().unwrap_or_default();
        let muted = crate::mic_mute_state(config.mic_device_id.as_deref()).unwrap_or(false);
        Self {
            config,
            devices,
            muted,
        }
    }
}

pub fn update_settings(
    mut settings: Signal<SettingsSnapshot>,
    update: impl FnOnce(&mut crate::Config),
) {
    let mut config = crate::load_config().unwrap_or_else(|_| settings().config);
    update(&mut config);
    let _ = crate::save_config(&config);
    settings.set(SettingsSnapshot::from_config(config));
}

pub fn settings_app() -> Element {
    let desktop = dioxus::desktop::use_window();
    let drag_desktop = desktop.clone();
    let devtools_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let mut settings = use_signal(SettingsSnapshot::load);
    let active_tab = use_signal(|| SettingsTab::General);
    let active_section = use_signal(|| SettingsTab::General.first_section_id().to_string());
    let recording = use_signal(|| false);
    use_future(move || async move {
        loop {
            let next = SettingsSnapshot::load();
            if *settings.peek() != next {
                settings.set(next);
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    });
    let theme_style = crate::WindowsAccent::load().css_vars();
    let titlebar_icon_style = format!(
        r#".titlebar-settings {{ --titlebar-icon: url("{SETTINGS_ICON}"); }}
.titlebar-close {{ --titlebar-icon: url("{CLOSE_ICON}"); }}"#
    );
    let font_face = format!(
        r#"@font-face {{
  font-family: "Geist";
  src: url("{GEIST_FONT}") format("truetype");
  font-weight: 400;
  font-style: normal;
  font-display: swap;
}}"#
    );

    rsx! {
        link { rel: "icon", href: APP_ICO, r#type: "image/x-icon" }
        style { {font_face} }
        link { rel: "stylesheet", href: GLOBAL_CSS }
        link { rel: "stylesheet", href: LAYOUT_CSS }
        link { rel: "stylesheet", href: TITLEBAR_CSS }
        link { rel: "stylesheet", href: TABS_CSS }
        link { rel: "stylesheet", href: GENERAL_CSS }
        link { rel: "stylesheet", href: SOUNDS_CSS }
        style { {theme_style} }
        style { {titlebar_icon_style} }
        div {
            class: "window",
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
                    onclick: move |_| close_desktop.close(),
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
                    {sections::render(active_tab(), settings, recording)}
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
