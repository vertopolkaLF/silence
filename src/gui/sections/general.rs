use dioxus::prelude::*;

pub fn render(
    settings: Signal<super::super::SettingsSnapshot>,
    _recording: Signal<bool>,
) -> Element {
    let snapshot = settings();
    let selected_value = snapshot.config.mic_device_id.clone().unwrap_or_default();
    let mic_muted = snapshot.muted;
    let mic_status = if mic_muted {
        "Microphone is muted"
    } else {
        "Microphone is unmuted"
    };
    let status_icon =
        crate::overlay_icons::overlay_icon_css_url(&snapshot.config.overlay.icon_pair, mic_muted);
    let mic_label =
        crate::selected_mic_label(snapshot.config.mic_device_id.as_deref(), &snapshot.devices);

    rsx! {
        section {
            class: "status-row",
            id: "general-status",
            "data-settings-section": "true",
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

        section {
            class: "field-group",
            label { "Microphone" }
            div { class: "select-wrap",
                select {
                    class: "select-like",
                    value: "{selected_value}",
                    onchange: move |evt| {
                        let value = evt.value();
                        super::super::update_settings(settings, |config| {
                            config.mic_device_id = if value.is_empty() { None } else { Some(value) };
                        });
                    },
                    option { value: "", "Default input device" }
                    for device in snapshot.devices {
                        option {
                            value: "{device.id}",
                            selected: selected_value == device.id,
                            if device.is_default {
                                "{device.name} (default)"
                            } else {
                                "{device.name}"
                            }
                        }
                    }
                }
                span { class: "solar-icon select-icon icon-down" }
            }
        }

    }
}
