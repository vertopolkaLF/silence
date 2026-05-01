use dioxus::prelude::*;

pub fn render(
    settings: Signal<super::super::SettingsSnapshot>,
    _recording: Signal<bool>,
) -> Element {
    let snapshot = settings();
    let mic_muted = snapshot.muted;
    let mic_status = if mic_muted {
        "Microphone is muted"
    } else {
        "Microphone is unmuted"
    };
    let status_icon =
        crate::overlay_icons::overlay_icon_css_url(&snapshot.config.overlay.icon_pair, mic_muted);
    let mic_label = crate::default_mic_label(&snapshot.devices);

    rsx! {
        section {
            class: "general-panel",
            id: "general-status",
            "data-settings-section": "true",
            div {
                class: "status-row",
                div {
                    class: if mic_muted { "mic-dot muted" } else { "mic-dot" },
                    span {
                        class: "solar-icon",
                        style: "--icon: url('{status_icon}');"
                    }
                }
                div {
                    class: "status-copy",
                    h1 { "{mic_status}" }
                    p { "{mic_label}" }
                }
            }

            section { class: "sound-card startup-card",
                div { class: "sound-card-title startup-row",
                    div { class: "startup-copy",
                        h2 { "Launch at Windows startup" }
                        p { "Start silence! in the tray as soon as you sign in." }
                    }
                    super::Toggle {
                        checked: snapshot.config.startup.launch_on_startup,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.startup.launch_on_startup = checked;
                            });
                        }
                    }
                }
            }
        }
    }
}
