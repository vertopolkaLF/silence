use dioxus::prelude::*;

mod sections;
mod styles;
mod tabs;

use tabs::SettingsTab;

pub fn settings_app() -> Element {
    let desktop = dioxus::desktop::use_window();
    let drag_desktop = desktop.clone();
    let minimize_desktop = desktop.clone();
    let maximize_desktop = desktop.clone();
    let close_desktop = desktop.clone();
    let initial = crate::load_config().unwrap_or_default().shortcut;
    let shortcut = use_signal(|| initial);
    let active_tab = use_signal(|| SettingsTab::General);
    let recording = use_signal(|| false);
    let saved = use_signal(|| false);

    rsx! {
        style { {styles::SETTINGS_CSS} }
        div {
            class: "window",
            div {
                class: "titlebar",
                onmousedown: move |_| drag_desktop.drag(),
                div { class: "hamburger", span {} span {} span {} }
                div { class: "brandmark", "S" }
                div { class: "title", "silence!" }
                div { class: "title-spacer" }
                button {
                    class: "window-button",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| minimize_desktop.window.set_minimized(true),
                    "-"
                }
                button {
                    class: "window-button",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| maximize_desktop.toggle_maximized(),
                    "□"
                }
                button {
                    class: "window-button close",
                    onmousedown: move |evt| evt.stop_propagation(),
                    onclick: move |_| close_desktop.close(),
                    "×"
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
