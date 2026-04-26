use dioxus::prelude::*;

mod sections;
mod tabs;

use tabs::SettingsTab;

const APP_ICON: Asset = asset!("/assets/app.png");
const APP_ICO: Asset = asset!("/assets/app.ico");
const CLOSE_ICON: Asset = asset!("/assets/icons/codicon_close.svg");
const GENERAL_CSS: Asset = asset!("/assets/styles/general.css", AssetOptions::css());
const GEIST_FONT: Asset = asset!("/assets/fonts/Geist-VariableFont_wght.ttf");
const GLOBAL_CSS: Asset = asset!("/assets/styles/global.css", AssetOptions::css());
const LAYOUT_CSS: Asset = asset!("/assets/styles/layout.css", AssetOptions::css());
const MAXIMIZE_ICON: Asset = asset!("/assets/icons/codicon_chrome-maximize.svg");
const MINIMIZE_ICON: Asset = asset!("/assets/icons/codicon_chrome-minimize.svg");
const SETTINGS_ICON: Asset = asset!("/assets/icons/codicon_settings-gear.svg");
const TABS_CSS: Asset = asset!("/assets/styles/tabs.css", AssetOptions::css());
const TITLEBAR_CSS: Asset = asset!("/assets/styles/titlebar.css", AssetOptions::css());

pub fn settings_app() -> Element {
    let desktop = dioxus::desktop::use_window();
    let drag_desktop = desktop.clone();
    let devtools_desktop = desktop.clone();
    let minimize_desktop = desktop.clone();
    let maximize_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let initial = crate::load_config().unwrap_or_default().shortcut;
    let shortcut = use_signal(|| initial);
    let active_tab = use_signal(|| SettingsTab::General);
    let recording = use_signal(|| false);
    let saved = use_signal(|| false);
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
        div {
            class: "window",
            div {
                class: "titlebar",
                onmousedown: move |_| drag_desktop.drag(),
                img {
                    class: "titlebar-icon",
                    src: APP_ICON,
                    alt: "silence!"
                }
                div { class: "title", "silence!" }
                div { class: "title-spacer" }
                if cfg!(debug_assertions) {
                    button {
                        class: "titlebar-button devtools-button",
                        id: "devtools",
                        title: "Open DevTools",
                        onmousedown: move |evt| evt.stop_propagation(),
                        onclick: move |_| devtools_desktop.devtool(),
                        img {
                            src: SETTINGS_ICON,
                            alt: "DevTools"
                        }
                    }
                }
                button {
                    class: "titlebar-button",
                    id: "minimize",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| minimize_desktop.window.set_minimized(true),
                    img {
                        src: MINIMIZE_ICON,
                        alt: "Minimize"
                    }
                }
                button {
                    class: "titlebar-button",
                    id: "maximize",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| maximize_desktop.toggle_maximized(),
                    img {
                        src: MAXIMIZE_ICON,
                        alt: "Maximize"
                    }
                }
                button {
                    class: "titlebar-button",
                    id: "close",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| close_desktop.close(),
                    img {
                        src: CLOSE_ICON,
                        alt: "Close"
                    }
                }
            }

            div {
                class: "body",
                {tabs::render(active_tab)}
                main {
                    class: "content",
                    {sections::render(active_tab(), shortcut, recording, saved)}
                }
            }
        }
    }
}
