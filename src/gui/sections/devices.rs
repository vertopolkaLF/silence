use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

pub fn render(mut settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let input_devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
    let ungroup_tray_devices = snapshot.config.advanced.ungroup_tray_devices;
    let audio_device_name_display = snapshot.config.advanced.audio_device_name_display.clone();
    let selected_input = default_input_value(&input_devices);
    let selected_output = default_output_value(&output_devices);

    rsx! {
        section {
            class: "devices-panel",
            id: "devices-overview",
            "data-settings-section": "true",

            div { class: "devices-header",
                h1 { "Devices" }
            }

            section { class: "sound-card device-card devices-select-card",
                // Keep the actual device selectors first; this tab is primarily for switching devices.
                DeviceField {
                    title: "Output",
                    description: None,
                    empty: "No active output devices found",
                    value: selected_output,
                    options: output_options(output_devices, &audio_device_name_display),
                    show_current_detail: false,
                    onchange: move |device_id: String| {
                        if device_id.is_empty() {
                            return;
                        }
                        let _ = crate::set_default_render_device(&device_id);
                        let next = settings.peek().clone().refresh(true);
                        settings.set(next);
                    }
                }

                DeviceField {
                    title: "Input",
                    description: None,
                    empty: "No active input devices found",
                    value: selected_input,
                    options: input_options(input_devices, &audio_device_name_display),
                    show_current_detail: false,
                    onchange: move |device_id: String| {
                        if device_id.is_empty() {
                            return;
                        }
                        let _ = crate::set_default_capture_device(&device_id);
                        let next = settings.peek().clone().refresh(true);
                        settings.set(next);
                    }
                }
            }

            section { class: "sound-card device-card device-settings-card",
                div { class: "device-field",
                    div { class: "device-card-copy",
                        h2 { "Display audio devices names" }
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
fn DeviceField(
    title: &'static str,
    description: Option<&'static str>,
    empty: &'static str,
    value: String,
    options: Vec<SelectOption>,
    #[props(default = true)] show_current_detail: bool,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "device-field",
            div { class: "device-card-copy",
                h2 { "{title}" }
                if let Some(desc) = description {
                    p { "{desc}" }
                }
            }

            if options.is_empty() {
                div { class: "device-empty", "{empty}" }
            } else {
                Select {
                    value,
                    options,
                    onchange: move |device_id| onchange.call(device_id),
                    show_current_detail,
                    class: "device-select"
                }
            }
        }
    }
}

fn input_options(devices: Vec<crate::MicDevice>, name_display: &str) -> Vec<SelectOption> {
    devices
        .into_iter()
        .map(|device| {
            let name = device.display_name(name_display);
            device_option(device.id, name, device.is_default, "icon-microphone")
        })
        .collect()
}

fn output_options(devices: Vec<crate::AudioDevice>, name_display: &str) -> Vec<SelectOption> {
    devices
        .into_iter()
        .map(|device| {
            let name = device.display_name(name_display);
            device_option(device.id, name, device.is_default, "icon-volume")
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

fn device_option(id: String, name: String, is_default: bool, icon: &str) -> SelectOption {
    let option = SelectOption::new(id, name).icon(icon);
    if is_default {
        option.detail("Windows default")
    } else {
        option
    }
}

fn default_input_value(devices: &[crate::MicDevice]) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first())
        .map(|device| device.id.clone())
        .unwrap_or_default()
}

fn default_output_value(devices: &[crate::AudioDevice]) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .or_else(|| devices.first())
        .map(|device| device.id.clone())
        .unwrap_or_default()
}
