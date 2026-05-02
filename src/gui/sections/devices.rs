use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

pub fn render(mut settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let input_devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
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

            div { class: "devices-grid",
                DeviceCard {
                    title: "Input",
                    description: None,
                    empty: "No active input devices found",
                    value: selected_input,
                    options: input_options(input_devices),
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

                DeviceCard {
                    title: "Output",
                    description: None,
                    empty: "No active output devices found",
                    value: selected_output,
                    options: output_options(output_devices),
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
            }
        }
    }
}

#[component]
fn DeviceCard(
    title: &'static str,
    description: Option<&'static str>,
    empty: &'static str,
    value: String,
    options: Vec<SelectOption>,
    #[props(default = true)] show_current_detail: bool,
    onchange: EventHandler<String>,
) -> Element {
    rsx! {
        section { class: "sound-card device-card",
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

fn input_options(devices: Vec<crate::MicDevice>) -> Vec<SelectOption> {
    devices
        .into_iter()
        .map(|device| device_option(device.id, device.name, device.is_default, "icon-microphone"))
        .collect()
}

fn output_options(devices: Vec<crate::AudioDevice>) -> Vec<SelectOption> {
    devices
        .into_iter()
        .map(|device| device_option(device.id, device.name, device.is_default, "icon-volume"))
        .collect()
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
