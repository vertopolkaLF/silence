use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let input_devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
    let ungroup_tray_devices = snapshot.config.advanced.ungroup_tray_devices;
    let audio_device_name_display = snapshot.config.advanced.audio_device_name_display.clone();
    let rename_allowed = audio_device_name_display != crate::AUDIO_DEVICE_NAME_SYSTEM;

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
                    rename_allowed,
                    settings
                }

                DeviceList {
                    title: "Input",
                    empty: "No active input devices found",
                    devices: input_rows(input_devices, &audio_device_name_display),
                    flow: DeviceFlow::Input,
                    rename_allowed,
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
    rename_allowed: bool,
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
                            rename_allowed,
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
    rename_allowed: bool,
    mut settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let mut renaming = use_signal(|| false);
    let mut draft_name = use_signal(|| device.name.clone());
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
    let device_id_for_rename_key = device.id.clone();
    let device_id_for_rename_confirm = device.id.clone();
    let device_id_for_properties = device.id.clone();
    let current_name_for_rename_blur = device.name.clone();
    let current_name_for_rename_key = device.name.clone();
    let current_name_for_rename_click = device.name.clone();
    let current_name_for_rename_confirm = device.name.clone();
    let current_name_for_rename_cancel = device.name.clone();
    let rename_input_id = format!("device-rename-{}", sanitize_dom_id(&device.id));
    let rename_input_width = rename_input_width_ch(&draft_name());
    let item_class = match (device.is_default, renaming()) {
        (true, true) => "device-list-item active renaming",
        (true, false) => "device-list-item active",
        (false, true) => "device-list-item renaming",
        (false, false) => "device-list-item",
    };

    rsx! {
        div { class: "{item_class}",
            div { class: "device-leading",
                span { class: "solar-icon device-kind-icon {icon_class}" }
                div { class: "device-name-block",
                    if renaming() {
                        div { class: "device-rename-row",
                            input {
                                id: "{rename_input_id}",
                                class: "device-rename-input",
                                style: "width: {rename_input_width}ch;",
                                value: "{draft_name()}",
                                autofocus: true,
                                onmounted: move |_| {
                                    let input_id = rename_input_id.clone();
                                    document::eval(&format!(
                                        r#"setTimeout(() => {{
  const input = document.getElementById({input_id:?});
  if (input) {{
    input.focus();
    const end = input.value.length;
    input.setSelectionRange(end, end);
  }}
}}, 0);"#
                                    ));
                                },
                                oninput: move |evt| {
                                    draft_name.set(evt.value());
                                },
                                onblur: move |_| {
                                    if draft_name().trim().is_empty() {
                                        draft_name.set(current_name_for_rename_blur.clone());
                                    }
                                },
                                onkeydown: move |evt| {
                                    let key = evt.data().key().to_string();
                                    if key == "Enter" {
                                        evt.prevent_default();
                                        let next_name = draft_name().trim().to_string();
                                        if !next_name.is_empty() && next_name != current_name_for_rename_key {
                                            let _ = crate::rename_audio_device(&device_id_for_rename_key, &next_name);
                                            let next = settings.peek().clone().refresh(true);
                                            settings.set(next);
                                        } else {
                                            draft_name.set(current_name_for_rename_key.clone());
                                        }
                                        renaming.set(false);
                                    } else if key == "Escape" {
                                        evt.prevent_default();
                                        draft_name.set(current_name_for_rename_key.clone());
                                        renaming.set(false);
                                    }
                                }
                            }

                            button {
                                r#type: "button",
                                class: "device-inline-action rename-commit-button",
                                title: "Confirm rename",
                                onclick: move |_| {
                                    let next_name = draft_name().trim().to_string();
                                    if !next_name.is_empty() && next_name != current_name_for_rename_confirm {
                                        let _ = crate::rename_audio_device(&device_id_for_rename_confirm, &next_name);
                                        let next = settings.peek().clone().refresh(true);
                                        settings.set(next);
                                    } else {
                                        draft_name.set(current_name_for_rename_confirm.clone());
                                    }
                                    renaming.set(false);
                                },
                                span { class: "solar-icon icon-check" }
                            }

                            button {
                                r#type: "button",
                                class: "device-inline-action rename-cancel-button",
                                title: "Cancel rename",
                                onclick: move |_| {
                                    draft_name.set(current_name_for_rename_cancel.clone());
                                    renaming.set(false);
                                },
                                span { class: "solar-icon icon-close" }
                            }
                        }
                    } else {
                        span { class: "device-name", "{device.name}" }
                    }
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
                    span { class: "solar-icon icon-check" }
                }

                if rename_allowed && !renaming() {
                    button {
                        r#type: "button",
                        class: "device-action-button",
                        title: "Rename",
                        onclick: move |_| {
                            draft_name.set(current_name_for_rename_click.clone());
                            renaming.set(true);
                        },
                        span { class: "solar-icon icon-pen" }
                    }
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

fn sanitize_dom_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect()
}

fn rename_input_width_ch(value: &str) -> usize {
    value.chars().count().clamp(2, 32) + 2
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
