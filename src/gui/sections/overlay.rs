use dioxus::prelude::*;

pub fn render(
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    sound_settings: Signal<crate::SoundSettings>,
    mut overlay: Signal<crate::OverlayConfig>,
) -> Element {
    let mut positioning = use_signal(|| false);
    let settings = overlay();
    let duration = format!("{:.1}", settings.duration_secs.clamp(0.1, 10.0));
    let x = format!("{:.0}", settings.position_x.clamp(0.0, 100.0));
    let y = format!("{:.0}", settings.position_y.clamp(0.0, 100.0));
    let scale = settings.scale.clamp(10, 400);
    let duration_progress = format!("{:.0}%", settings.duration_secs.clamp(0.1, 10.0) * 10.0);
    let x_progress = format!("{:.0}%", settings.position_x.clamp(0.0, 100.0));
    let y_progress = format!("{:.0}%", settings.position_y.clamp(0.0, 100.0));
    let scale_progress = format!("{:.0}%", (scale as f64 - 10.0) / 390.0 * 100.0);

    rsx! {
        section { class: "overlay-panel",
            div { class: "overlay-header",
                h1 { "Overlay" }
                p { "A small topmost window that passes clicks through to whatever is underneath." }
            }

            section { class: "sound-card",
                div { class: "sound-card-title",
                    div {
                        h2 { "Enable overlay" }
                        p { "Shows microphone state above other windows." }
                    }
                    super::Toggle {
                        checked: settings.enabled,
                        onchange: move |checked| {
                            let mut next = overlay();
                            next.enabled = checked;
                            overlay.set(next);
                            save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                        }
                    }
                }
            }

            section { class: "sound-card",
                div { class: "overlay-field",
                    label { "Visibility" }
                    div { class: "select-wrap",
                        select {
                            class: "select-like",
                            value: "{settings.visibility}",
                            onchange: move |evt| {
                                let mut next = overlay();
                                next.visibility = evt.value();
                                overlay.set(next);
                                save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                            },
                            option { value: "Always", selected: settings.visibility == "Always", "Always visible" }
                            option { value: "WhenMuted", selected: settings.visibility == "WhenMuted", "Visible when muted" }
                            option { value: "WhenUnmuted", selected: settings.visibility == "WhenUnmuted", "Visible when unmuted" }
                            option { value: "AfterToggle", selected: settings.visibility == "AfterToggle", "Show after toggle" }
                        }
                        span { class: "solar-icon select-icon icon-down" }
                    }
                }

                if settings.visibility == "AfterToggle" {
                    div { class: "overlay-range-row",
                        div {
                            label { "Duration" }
                            span { class: "sound-value", "{duration}s" }
                        }
                        input {
                            class: "volume-slider",
                            r#type: "range",
                            min: "0.1",
                            max: "10",
                            step: "0.1",
                            value: "{duration}",
                            style: "--range-progress: {duration_progress};",
                            oninput: move |evt| {
                                if let Ok(value) = evt.value().parse::<f64>() {
                                    let mut next = overlay();
                                    next.duration_secs = value.clamp(0.1, 10.0);
                                    overlay.set(next);
                                    save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                                }
                            }
                        }
                    }
                }
            }

            section { class: "sound-card",
                div { class: "overlay-range-row",
                    div {
                        label { "Horizontal position" }
                        span { class: "sound-value", "{x}%" }
                    }
                    input {
                        class: "volume-slider",
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{x}",
                        style: "--range-progress: {x_progress};",
                        oninput: move |evt| {
                            if let Ok(value) = evt.value().parse::<f64>() {
                                let mut next = overlay();
                                next.position_x = value.clamp(0.0, 100.0);
                                overlay.set(next);
                                save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                            }
                        }
                    }
                }

                div { class: "overlay-range-row",
                    div {
                        label { "Vertical position" }
                        span { class: "sound-value", "{y}%" }
                    }
                    input {
                        class: "volume-slider",
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{y}",
                        style: "--range-progress: {y_progress};",
                        oninput: move |evt| {
                            if let Ok(value) = evt.value().parse::<f64>() {
                                let mut next = overlay();
                                next.position_y = value.clamp(0.0, 100.0);
                                overlay.set(next);
                                save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                            }
                        }
                    }
                }

                div { class: "overlay-range-row",
                    div {
                        label { "Size" }
                        span { class: "sound-value", "{scale}%" }
                    }
                    input {
                        class: "volume-slider",
                        r#type: "range",
                        min: "10",
                        max: "400",
                        step: "5",
                        value: "{scale}",
                        style: "--range-progress: {scale_progress};",
                        oninput: move |evt| {
                            if let Ok(value) = evt.value().parse::<u32>() {
                                let mut next = overlay();
                                next.scale = value.clamp(10, 400);
                                overlay.set(next);
                                save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                            }
                        }
                    }
                }

                label {
                    class: "check-row",
                    input {
                        r#type: "checkbox",
                        checked: settings.show_text,
                        onchange: move |evt| {
                            let mut next = overlay();
                            next.show_text = evt.checked();
                            overlay.set(next);
                            save_overlay(shortcut, mic_device_id, sound_settings, overlay);
                        }
                    }
                    span { "Show text next to the icon" }
                }
            }

            section { class: "sound-card",
                div { class: "sound-card-title",
                    div {
                        h2 { "Move overlay" }
                        p { "Shows the overlay until this is turned off." }
                    }
                    super::Toggle {
                        checked: positioning(),
                        onchange: move |checked| {
                            positioning.set(checked);
                            crate::set_overlay_positioning(checked);
                        }
                    }
                }
            }
        }
    }
}

fn save_overlay(
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    sound_settings: Signal<crate::SoundSettings>,
    overlay: Signal<crate::OverlayConfig>,
) {
    let mut config = crate::load_config().unwrap_or_default();
    config.shortcut = shortcut();
    config.mic_device_id = mic_device_id();
    config.sound_settings = sound_settings();
    config.overlay = overlay();
    let _ = crate::save_config(&config);
}
