use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let input_devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
    let ungroup_tray_devices = snapshot.config.advanced.ungroup_tray_devices;
    let audio_device_name_display = snapshot.config.advanced.audio_device_name_display.clone();

    rsx! {
        section {
            class: "devices-panel",
            id: "devices-overview",
            "data-settings-section": "true",

            div { class: "devices-header",
                h1 { "Devices" }
            }

            section { class: "sound-card device-card devices-select-card",
                DeviceList {
                    title: "Output",
                    empty: "No active output devices found",
                    devices: output_rows(output_devices, &audio_device_name_display),
                    flow: DeviceFlow::Output,
                    settings
                }

                DeviceList {
                    title: "Input",
                    empty: "No active input devices found",
                    devices: input_rows(input_devices, &audio_device_name_display),
                    flow: DeviceFlow::Input,
                    settings
                }
            }

            section { class: "sound-card device-card device-settings-card",
                div { class: "device-field",
                    div { class: "device-card-copy",
                        h2 { "Displayed audio device name" }
                    }
                    Select {
                        value: audio_device_name_display.clone(),
                        options: audio_device_name_options(),
                        onchange: move |value: String| {
                            super::super::update_settings(settings, |config| {
                                config.advanced.audio_device_name_display = value;
                            });
                        },
                        show_current_detail: false,
                        class: "device-select"
                    }
                }

                div { class: "sound-card-title advanced-row device-toggle-row",
                    div { class: "device-card-copy",
                        h2 { "Ungroup devices in tray menu" }
                    }
                    super::Toggle {
                        checked: ungroup_tray_devices,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.advanced.ungroup_tray_devices = checked;
                            });
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeviceList(
    title: &'static str,
    empty: &'static str,
    devices: Vec<DeviceRow>,
    flow: DeviceFlow,
    mut settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    rsx! {
        div { class: "device-field",
            div { class: "device-card-copy",
                h2 { "{title}" }
            }

            if devices.is_empty() {
                div { class: "device-empty", "{empty}" }
            } else {
                div { class: "device-list",
                    for device in devices {
                        DeviceListItem {
                            key: "{device.id}",
                            device,
                            flow,
                            settings
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DeviceListItem(
    device: DeviceRow,
    flow: DeviceFlow,
    mut settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let default_class = if device.is_default {
        "device-action-button default active"
    } else {
        "device-action-button default"
    };
    let status_text = if device.is_default {
        "Windows default"
    } else {
        "Available"
    };
    let icon_class = flow.icon_class();
    let device_id_for_default = device.id.clone();
    let device_id_for_rename = device.id.clone();
    let device_id_for_properties = device.id.clone();

    rsx! {
        div { class: if device.is_default { "device-list-item active" } else { "device-list-item" },
            div { class: "device-leading",
                span { class: "solar-icon device-kind-icon {icon_class}" }
                div { class: "device-name-block",
                    span { class: "device-name", "{device.name}" }
                    span { class: "device-status", "{status_text}" }
                }
            }

            div { class: "device-actions",
                button {
                    r#type: "button",
                    class: "{default_class}",
                    title: "Set as default",
                    disabled: device.is_default,
                    onclick: move |_| {
                        if device_id_for_default.is_empty() {
                            return;
                        }
                        match flow {
                            DeviceFlow::Input => {
                                let _ = crate::set_default_capture_device(&device_id_for_default);
                            }
                            DeviceFlow::Output => {
                                let _ = crate::set_default_render_device(&device_id_for_default);
                            }
                        }
                        let next = settings.peek().clone().refresh(true);
                        settings.set(next);
                    },
                    span { class: "solar-icon icon-shield-check" }
                }

                button {
                    r#type: "button",
                    class: "device-action-button",
                    title: "Rename",
                    onclick: move |_| {
                        let _ = crate::open_audio_device_rename(&device_id_for_rename, flow.is_input());
                    },
                    span { class: "solar-icon icon-pen" }
                }

                button {
                    r#type: "button",
                    class: "device-action-button",
                    title: "Open properties",
                    onclick: move |_| {
                        let _ = crate::open_audio_device_properties(&device_id_for_properties, flow.is_input());
                    },
                    span { class: "solar-icon icon-settings" }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct DeviceRow {
    id: String,
    name: String,
    is_default: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DeviceFlow {
    Input,
    Output,
}

impl DeviceFlow {
    fn icon_class(self) -> &'static str {
        match self {
            Self::Input => "icon-microphone",
            Self::Output => "icon-volume",
        }
    }

    fn is_input(self) -> bool {
        matches!(self, Self::Input)
    }
}

fn input_rows(devices: Vec<crate::MicDevice>, name_display: &str) -> Vec<DeviceRow> {
    devices
        .into_iter()
        .map(|device| {
            let name = device.display_name(name_display);
            device_row(device.id, name, device.is_default)
        })
        .collect()
}

fn output_rows(devices: Vec<crate::AudioDevice>, name_display: &str) -> Vec<DeviceRow> {
    devices
        .into_iter()
        .map(|device| {
            let name = device.display_name(name_display);
            device_row(device.id, name, device.is_default)
        })
        .collect()
}

fn audio_device_name_options() -> Vec<SelectOption> {
    vec![
        SelectOption::new(crate::AUDIO_DEVICE_NAME_PRETTY, "Pretty Name").icon("icon-microphone"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_SYSTEM, "System Name").icon("icon-widget"),
        SelectOption::new(crate::AUDIO_DEVICE_NAME_BOTH, "Both").icon("icon-volume"),
    ]
}

fn device_row(id: String, name: String, is_default: bool) -> DeviceRow {
    DeviceRow {
        id,
        name,
        is_default,
    }
}
