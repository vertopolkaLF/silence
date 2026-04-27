use dioxus::prelude::*;

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut positioning = use_signal(|| false);
    let mut icons_expanded = use_signal(|| false);
    let snapshot = settings();
    let overlay = snapshot.config.overlay.clone();
    let duration = format!("{:.1}", overlay.duration_secs.clamp(0.1, 10.0));
    let x = format!("{:.0}", overlay.position_x.clamp(0.0, 100.0));
    let y = format!("{:.0}", overlay.position_y.clamp(0.0, 100.0));
    let scale = overlay.scale.clamp(10, 400);
    let content_opacity = overlay.content_opacity.clamp(20, 100);
    let background_opacity = overlay.background_opacity.min(100);
    let border_radius = overlay.border_radius.min(24);
    let icon_controls_open = overlay.variant != "Dot";
    let duration_controls_open = overlay.visibility == "AfterToggle";
    let preview_muted = snapshot.muted;
    let preview_tone_class = if preview_muted { "muted" } else { "live" };
    let duration_progress = format!("{:.0}%", overlay.duration_secs.clamp(0.1, 10.0) * 10.0);
    let x_progress = format!("{:.0}%", overlay.position_x.clamp(0.0, 100.0));
    let y_progress = format!("{:.0}%", overlay.position_y.clamp(0.0, 100.0));
    let scale_progress = format!("{:.0}%", (scale as f64 - 10.0) / 390.0 * 100.0);
    let content_opacity_progress =
        format!("{:.0}%", (content_opacity as f64 - 20.0) / 80.0 * 100.0);
    let background_opacity_progress = format!("{background_opacity}%");
    let border_radius_progress = format!("{:.0}%", border_radius as f64 / 24.0 * 100.0);

    rsx! {
        section {
            class: "overlay-panel",
            id: "overlay-overview",
            "data-settings-section": "true",
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

                div {
                    class: if duration_controls_open { "overlay-collapse open" } else { "overlay-collapse" },
                    div { class: "overlay-collapse-inner",
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

            section { class: "sound-card overlay-appearance",
                id: "overlay-appearance",
                "data-settings-section": "true",
                div { class: "section-head", h1 { "Appearance" } }

                div { class: "overlay-field",
                    label { "Overlay style" }
                    div { class: "overlay-variant-grid",
                        button {
                            class: if overlay.variant == "MicIcon" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "MicIcon".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview live",
                                span { class: "solar-icon icon-mic" }
                            }
                            span { "Mic Icon" }
                        }
                        button {
                            class: if overlay.variant == "Dot" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "Dot".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview dot",
                                span {}
                            }
                            span { "Dot" }
                        }
                    }
                }

                div {
                    class: if icon_controls_open { "overlay-collapse open" } else { "overlay-collapse" },
                    div { class: "overlay-collapse-inner",
                        div { class: "overlay-field overlay-icon-field",
                            label { "Mic icons" }
                            div { class: "overlay-icon-grid overlay-icon-grid-primary",
                                for pair in crate::overlay_icons::featured_overlay_icon_pairs().iter() {
                                    button {
                                        class: if overlay.icon_pair == pair.id {
                                            "overlay-icon-option active"
                                        } else {
                                            "overlay-icon-option"
                                        },
                                        onclick: {
                                            let id = pair.id.to_string();
                                            move |_| {
                                                let next_id = id.clone();
                                                super::super::update_settings(settings, move |config| {
                                                    config.overlay.icon_pair = next_id;
                                                });
                                            }
                                        },
                                        title: "{pair.label}",
                                        span { class: "overlay-icon-preview {preview_tone_class}",
                                            span {
                                                class: "solar-icon",
                                                style: format!(
                                                    "--icon: url('{}');",
                                                    crate::overlay_icons::overlay_icon_css_url(
                                                        pair.id,
                                                        preview_muted,
                                                    ),
                                                )
                                            }
                                        }
                                        span { "{pair.label}" }
                                    }
                                }
                                button {
                                    class: if icons_expanded() {
                                        "overlay-icon-option overlay-icon-toggle expanded"
                                    } else {
                                        "overlay-icon-option overlay-icon-toggle"
                                    },
                                    title: if icons_expanded() { "Collapse icons" } else { "Expand icons" },
                                    onclick: move |_| icons_expanded.set(!icons_expanded()),
                                    span { class: "overlay-icon-preview",
                                        span { class: "solar-icon icon-down overlay-icon-toggle-glyph" }
                                    }
                                    span { if icons_expanded() { "Collapse" } else { "Expand" } }
                                }
                            }
                            div {
                                class: if icons_expanded() {
                                    "overlay-collapse open overlay-icon-extra-wrap"
                                } else {
                                    "overlay-collapse overlay-icon-extra-wrap"
                                },
                                div { class: "overlay-collapse-inner",
                                    div { class: "overlay-icon-grid overlay-icon-grid-extra",
                                        for pair in crate::overlay_icons::extra_overlay_icon_pairs().iter() {
                                            button {
                                                class: if overlay.icon_pair == pair.id {
                                                    "overlay-icon-option active"
                                                } else {
                                                    "overlay-icon-option"
                                                },
                                                onclick: {
                                                    let id = pair.id.to_string();
                                                    move |_| {
                                                        let next_id = id.clone();
                                                        super::super::update_settings(settings, move |config| {
                                                            config.overlay.icon_pair = next_id;
                                                        });
                                                    }
                                                },
                                                title: "{pair.label}",
                                                span { class: "overlay-icon-preview {preview_tone_class}",
                                                    span {
                                                        class: "solar-icon",
                                                        style: format!(
                                                            "--icon: url('{}');",
                                                            crate::overlay_icons::overlay_icon_css_url(
                                                                pair.id,
                                                                preview_muted,
                                                            ),
                                                        )
                                                    }
                                                }
                                                span { "{pair.label}" }
                                            }
                                        }
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
                        div { class: "overlay-field",
                            label { "Icon style" }
                            div { class: "select-wrap",
                                select {
                                    class: "select-like",
                                    value: "{overlay.icon_style}",
                                    onchange: move |evt| {
                                        super::super::update_settings(settings, |config| {
                                            config.overlay.icon_style = evt.value();
                                        });
                                    },
                                    option { value: "Colored", selected: overlay.icon_style == "Colored", "Colored (red/green)" }
                                    option { value: "Monochrome", selected: overlay.icon_style == "Monochrome", "Monochrome" }
                                }
                                span { class: "solar-icon select-icon icon-down" }
                            }
                        }
                        div { class: "overlay-field",
                            label { "Background" }
                            div { class: "select-wrap",
                                select {
                                    class: "select-like",
                                    value: "{overlay.background_style}",
                                    onchange: move |evt| {
                                        super::super::update_settings(settings, |config| {
                                            config.overlay.background_style = evt.value();
                                        });
                                    },
                                    option { value: "Dark", selected: overlay.background_style == "Dark", "Dark" }
                                    option { value: "Light", selected: overlay.background_style == "Light", "Light" }
                                }
                                span { class: "solar-icon select-icon icon-down" }
                            }
                        }
                        div { class: "overlay-range-row",
                            div {
                                label { "Background opacity" }
                                span { class: "sound-value", "{background_opacity}%" }
                            }
                            input {
                                class: "volume-slider",
                                r#type: "range",
                                min: "0",
                                max: "100",
                                step: "5",
                                value: "{background_opacity}",
                                style: "--range-progress: {background_opacity_progress};",
                                oninput: move |evt| {
                                    if let Ok(value) = evt.value().parse::<u8>() {
                                        super::super::update_settings(settings, |config| {
                                            config.overlay.background_opacity = value.min(100);
                                        });
                                    }
                                }
                            }
                        }
                        div { class: "overlay-range-row",
                            div {
                                label { "Border radius" }
                                span { class: "sound-value", "{border_radius}px" }
                            }
                            input {
                                class: "volume-slider",
                                r#type: "range",
                                min: "0",
                                max: "24",
                                step: "1",
                                value: "{border_radius}",
                                style: "--range-progress: {border_radius_progress};",
                                oninput: move |evt| {
                                    if let Ok(value) = evt.value().parse::<u8>() {
                                        super::super::update_settings(settings, |config| {
                                            config.overlay.border_radius = value.min(24);
                                        });
                                    }
                                }
                            }
                        }
                        label {
                            class: "check-row",
                            input {
                                r#type: "checkbox",
                                checked: overlay.show_border,
                                onchange: move |evt| {
                                    super::super::update_settings(settings, |config| {
                                        config.overlay.show_border = evt.checked();
                                    });
                                }
                            }
                            span { "Show border" }
                        }
                    }
                }

                div { class: "overlay-range-row",
                    div {
                        label { "Size scale" }
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

                div { class: "overlay-range-row",
                    div {
                        label { "Opacity" }
                        span { class: "sound-value", "{content_opacity}%" }
                    }
                    input {
                        class: "volume-slider",
                        r#type: "range",
                        min: "20",
                        max: "100",
                        step: "5",
                        value: "{content_opacity}",
                        style: "--range-progress: {content_opacity_progress};",
                        oninput: move |evt| {
                            if let Ok(value) = evt.value().parse::<u8>() {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.content_opacity = value.clamp(20, 100);
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
