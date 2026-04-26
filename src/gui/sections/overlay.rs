use dioxus::prelude::*;

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut positioning = use_signal(|| false);
    let snapshot = settings();
    let overlay = snapshot.config.overlay.clone();
    let duration = format!("{:.1}", overlay.duration_secs.clamp(0.1, 10.0));
    let x = format!("{:.0}", overlay.position_x.clamp(0.0, 100.0));
    let y = format!("{:.0}", overlay.position_y.clamp(0.0, 100.0));
    let scale = overlay.scale.clamp(10, 400);
    let duration_progress = format!("{:.0}%", overlay.duration_secs.clamp(0.1, 10.0) * 10.0);
    let x_progress = format!("{:.0}%", overlay.position_x.clamp(0.0, 100.0));
    let y_progress = format!("{:.0}%", overlay.position_y.clamp(0.0, 100.0));
    let scale_progress = format!("{:.0}%", (scale as f64 - 10.0) / 390.0 * 100.0);

    rsx! {
        section { class: "overlay-panel",
            div { class: "overlay-header",
                h1 { "Overlay" }
            }

            section { class: "sound-card",
                div { class: "sound-card-title",
                    div {
                        h2 { "Enable overlay" }
                    }
                    super::Toggle {
                        checked: overlay.enabled,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.enabled = checked;
                            });
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
                            value: "{overlay.visibility}",
                            onchange: move |evt| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.visibility = evt.value();
                                });
                            },
                            option { value: "Always", selected: overlay.visibility == "Always", "Always visible" }
                            option { value: "WhenMuted", selected: overlay.visibility == "WhenMuted", "Visible when muted" }
                            option { value: "WhenUnmuted", selected: overlay.visibility == "WhenUnmuted", "Visible when unmuted" }
                            option { value: "AfterToggle", selected: overlay.visibility == "AfterToggle", "Show after toggle" }
                        }
                        span { class: "solar-icon select-icon icon-down" }
                    }
                }

                if overlay.visibility == "AfterToggle" {
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
                                    super::super::update_settings(settings, |config| {
                                        config.overlay.duration_secs = value.clamp(0.1, 10.0);
                                    });
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
                                super::super::update_settings(settings, |config| {
                                    config.overlay.position_x = value.clamp(0.0, 100.0);
                                });
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
                                super::super::update_settings(settings, |config| {
                                    config.overlay.position_y = value.clamp(0.0, 100.0);
                                });
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
                                super::super::update_settings(settings, |config| {
                                    config.overlay.scale = value.clamp(10, 400);
                                });
                            }
                        }
                    }
                }

                label {
                    class: "check-row",
                    input {
                        r#type: "checkbox",
                        checked: overlay.show_text,
                        onchange: move |evt| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.show_text = evt.checked();
                            });
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
                            if let Some(next) = crate::set_overlay_positioning(checked) {
                                super::super::update_settings(settings, |config| {
                                    config.overlay = next;
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
