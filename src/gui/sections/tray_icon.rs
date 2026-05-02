use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

const APP_IMAGE: Asset = asset!("/assets/app.png");

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut icons_expanded = use_signal(|| false);
    let snapshot = settings();
    let tray_icon = snapshot.config.tray_icon.clone();
    let muted = snapshot.muted;
    let preview_tone_class = if muted { "muted" } else { "live" };
    let status_controls_open = tray_icon.variant == "StatusMic";
    let status_style_options = vec![
        SelectOption::new("Colored", "Colored").icon("icon-mic"),
        SelectOption::new("Monochrome", "Monochrome").icon("icon-mic"),
        SelectOption::new("SystemColor", "System color").icon("icon-widget"),
    ];

    rsx! {
        section {
            class: "overlay-panel",
            id: "tray-icon-overview",
            "data-settings-section": "true",
            div { class: "overlay-header section-head-row",
                h1 { "Tray icon" }
            }

            section { class: "sound-card overlay-appearance",
                div { class: "overlay-field",
                    label { "Tray icon style" }
                    div { class: "overlay-variant-grid tray-icon-variant-grid",
                        button {
                            class: if tray_icon.variant == "Logo" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.tray_icon.variant = "Logo".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview tray-logo-preview",
                                img { src: APP_IMAGE, alt: "" }
                            }
                            span { "Logo" }
                        }
                        button {
                            class: if tray_icon.variant == "StatusMic" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.tray_icon.variant = "StatusMic".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview {preview_tone_class}",
                                span { class: "solar-icon icon-mic" }
                            }
                            span { "Mic status" }
                        }
                        button {
                            class: if tray_icon.variant == "ColorDot" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.tray_icon.variant = "ColorDot".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview dot",
                                span {}
                            }
                            span { "Color dot" }
                        }
                    }
                }

                div {
                    class: if status_controls_open { "overlay-collapse open" } else { "overlay-collapse" },
                    div { class: "overlay-collapse-inner",
                        div { class: "overlay-field overlay-icon-field",
                            label { "Mic icons" }
                            div { class: "overlay-icon-grid overlay-icon-grid-primary",
                                for pair in crate::overlay_icons::featured_overlay_icon_pairs().iter() {
                                    button {
                                        class: if tray_icon.icon_pair == pair.id {
                                            "overlay-icon-option active"
                                        } else {
                                            "overlay-icon-option"
                                        },
                                        onclick: {
                                            let id = pair.id.to_string();
                                            move |_| {
                                                let next_id = id.clone();
                                                super::super::update_settings(settings, move |config| {
                                                    config.tray_icon.icon_pair = next_id;
                                                });
                                            }
                                        },
                                        title: "{pair.label}",
                                        span { class: "overlay-icon-preview {preview_tone_class}",
                                            span {
                                                class: "solar-icon",
                                                style: format!(
                                                    "--icon: url('{}');",
                                                    crate::overlay_icons::overlay_icon_css_url(pair.id, muted),
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
                                                class: if tray_icon.icon_pair == pair.id {
                                                    "overlay-icon-option active"
                                                } else {
                                                    "overlay-icon-option"
                                                },
                                                onclick: {
                                                    let id = pair.id.to_string();
                                                    move |_| {
                                                        let next_id = id.clone();
                                                        super::super::update_settings(settings, move |config| {
                                                            config.tray_icon.icon_pair = next_id;
                                                        });
                                                    }
                                                },
                                                title: "{pair.label}",
                                                span { class: "overlay-icon-preview {preview_tone_class}",
                                                    span {
                                                        class: "solar-icon",
                                                        style: format!(
                                                            "--icon: url('{}');",
                                                            crate::overlay_icons::overlay_icon_css_url(pair.id, muted),
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

                        div { class: "overlay-field",
                            label { "Mic icon color" }
                            Select {
                                value: tray_icon.status_style.clone(),
                                options: status_style_options,
                                onchange: move |value: String| {
                                    super::super::update_settings(settings, |config| {
                                        config.tray_icon.status_style = value;
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
