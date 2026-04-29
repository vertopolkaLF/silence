use dioxus::prelude::*;

use crate::gui::controls::{Checkbox, Range, Select, SelectOption};

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
    let visibility_options = vec![
        SelectOption::new("Always", "Always visible")
            .detail("Keep the overlay on screen at all times")
            .icon("icon-widget"),
        SelectOption::new("WhenMuted", "Visible when muted")
            .detail("Only show the overlay while the microphone is muted")
            .icon("icon-mic"),
        SelectOption::new("WhenUnmuted", "Visible when unmuted")
            .detail("Only show the overlay while the microphone is live")
            .icon("icon-mic"),
        SelectOption::new("AfterToggle", "Show after toggle")
            .detail("Appear briefly after a mute state change")
            .icon("icon-record"),
    ];
    let icon_style_options = vec![
        SelectOption::new("Colored", "Colored")
            .detail("Use red and green states")
            .icon("icon-mic"),
        SelectOption::new("Monochrome", "Monochrome")
            .detail("Keep the icon neutral and let the shape carry meaning")
            .icon("icon-mic"),
    ];
    let background_options = vec![
        SelectOption::new("Dark", "Dark")
            .detail("Blends into most apps with low contrast")
            .icon("icon-widget"),
        SelectOption::new("Light", "Light")
            .detail("Stays readable on darker workspaces")
            .icon("icon-widget"),
    ];

    rsx! {
        section {
            class: "overlay-panel",
            id: "overlay-overview",
            "data-settings-section": "true",
            div { class: "overlay-header section-head-row",
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
                    Select {
                        value: overlay.visibility.clone(),
                        options: visibility_options,
                        onchange: move |value: String| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.visibility = value;
                            });
                        }
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
                            Range {
                                value: duration.clone(),
                                min: "0.1".to_string(),
                                max: "10".to_string(),
                                step: "0.1".to_string(),
                                progress: duration_progress.clone(),
                                oninput: move |evt: FormEvent| {
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
                    Range {
                        value: x.clone(),
                        min: "0".to_string(),
                        max: "100".to_string(),
                        step: "1".to_string(),
                        progress: x_progress.clone(),
                        oninput: move |evt: FormEvent| {
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
                    Range {
                        value: y.clone(),
                        min: "0".to_string(),
                        max: "100".to_string(),
                        step: "1".to_string(),
                        progress: y_progress.clone(),
                        oninput: move |evt: FormEvent| {
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
                div { class: "section-head section-head-row", h1 { "Appearance" } }

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
                        Checkbox {
                            class: "overlay-checkbox".to_string(),
                            checked: overlay.show_text,
                            label: "Show text next to the icon".to_string(),
                            onchange: move |checked: bool| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.show_text = checked;
                                });
                            }
                        }
                        div { class: "overlay-field",
                            label { "Icon style" }
                            Select {
                                value: overlay.icon_style.clone(),
                                options: icon_style_options,
                                onchange: move |value: String| {
                                    super::super::update_settings(settings, |config| {
                                        config.overlay.icon_style = value;
                                    });
                                }
                            }
                        }
                        div { class: "overlay-field",
                            label { "Background" }
                            Select {
                                value: overlay.background_style.clone(),
                                options: background_options,
                                onchange: move |value: String| {
                                    super::super::update_settings(settings, |config| {
                                        config.overlay.background_style = value;
                                    });
                                }
                            }
                        }
                        div { class: "overlay-range-row",
                            div {
                                label { "Background opacity" }
                                span { class: "sound-value", "{background_opacity}%" }
                            }
                            Range {
                                value: background_opacity.to_string(),
                                min: "0".to_string(),
                                max: "100".to_string(),
                                step: "5".to_string(),
                                progress: background_opacity_progress.clone(),
                                oninput: move |evt: FormEvent| {
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
                            Range {
                                value: border_radius.to_string(),
                                min: "0".to_string(),
                                max: "24".to_string(),
                                step: "1".to_string(),
                                progress: border_radius_progress.clone(),
                                oninput: move |evt: FormEvent| {
                                    if let Ok(value) = evt.value().parse::<u8>() {
                                        super::super::update_settings(settings, |config| {
                                            config.overlay.border_radius = value.min(24);
                                        });
                                    }
                                }
                            }
                        }
                        Checkbox {
                            class: "overlay-checkbox".to_string(),
                            checked: overlay.show_border,
                            label: "Show border".to_string(),
                            onchange: move |checked: bool| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.show_border = checked;
                                });
                            }
                        }
                    }
                }

                div { class: "overlay-range-row",
                    div {
                        label { "Size scale" }
                        span { class: "sound-value", "{scale}%" }
                    }
                    Range {
                        value: scale.to_string(),
                        min: "10".to_string(),
                        max: "400".to_string(),
                        step: "5".to_string(),
                        progress: scale_progress.clone(),
                        oninput: move |evt: FormEvent| {
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
                    Range {
                        value: content_opacity.to_string(),
                        min: "20".to_string(),
                        max: "100".to_string(),
                        step: "5".to_string(),
                        progress: content_opacity_progress.clone(),
                        oninput: move |evt: FormEvent| {
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
