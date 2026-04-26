use dioxus::prelude::*;

mod sections;
mod tabs;

use tabs::SettingsTab;

const APP_ICO: Asset = asset!("/assets/app.ico");
const CLOSE_ICON: Asset = asset!("/assets/icons/codicon_close.svg");
const GENERAL_CSS: Asset = asset!("/assets/styles/general.css", AssetOptions::css());
const GEIST_FONT: Asset = asset!("/assets/fonts/Geist-VariableFont_wght.ttf");
const GLOBAL_CSS: Asset = asset!("/assets/styles/global.css", AssetOptions::css());
const LAYOUT_CSS: Asset = asset!("/assets/styles/layout.css", AssetOptions::css());
const SETTINGS_ICON: Asset = asset!("/assets/icons/codicon_settings-gear.svg");
const TABS_CSS: Asset = asset!("/assets/styles/tabs.css", AssetOptions::css());
const TITLEBAR_CSS: Asset = asset!("/assets/styles/titlebar.css", AssetOptions::css());

pub fn settings_app() -> Element {
    let desktop = dioxus::desktop::use_window();
    let drag_desktop = desktop.clone();
    let devtools_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let initial = crate::load_config().unwrap_or_default();
    let shortcut = use_signal(move || initial.shortcut);
    let mic_device_id = use_signal(move || initial.mic_device_id.clone());
    let active_tab = use_signal(|| SettingsTab::General);
    let recording = use_signal(|| false);
    let saved = use_signal(|| false);
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
                {tabs::render(active_tab)}
                main {
                    class: "content",
                    {sections::render(active_tab(), shortcut, mic_device_id, recording, saved)}
                }
            }
        }
    }
}
