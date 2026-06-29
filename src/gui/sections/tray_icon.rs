use dioxus::prelude::*;

use crate::gui::controls::{Select, SelectOption};

const APP_IMAGE: Asset = asset!("/assets/app.png");

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut icons_expanded = use_signal(|| false);
    let snapshot = settings();
    let tray_icon = snapshot.config.tray_icon.clone();
    let mic_using_apps = snapshot.mic_using_apps.clone();
    let muted = snapshot.muted;
    let preview_tone_class = if muted { "muted" } else { "live" };
    let status_controls_open = tray_icon.variant == "StatusMic";
    let status_style_options = vec![
        SelectOption::new("Colored", "Colored").icon("icon-palette"),
        SelectOption::new("Monochrome", "Monochrome").icon("icon-contrast"),
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

            section {
                class: "sound-card overlay-appearance tray-mic-in-use-card",
                id: "tray-icon-mic-in-use",
                "data-settings-section": "true",
                div { class: "sound-card-title advanced-row device-toggle-row",
                    div { class: "device-card-copy",
                        h2 { "Show when mic is in use by an app" }
                    }
                    super::Toggle {
                        checked: tray_icon.show_mic_in_use,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.tray_icon.show_mic_in_use = checked;
                            });
                        }
                    }
                }

                div { class: "sound-card-title advanced-row device-toggle-row tray-mic-hide-row",
                    div { class: "device-card-copy",
                        h2 { "Hide blacklisted apps from tray menu" }
                    }
                    super::Toggle {
                        checked: tray_icon.hide_mic_in_use_ignored_apps,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.tray_icon.hide_mic_in_use_ignored_apps = checked;
                            });
                        }
                    }
                }

                div { class: "tray-mic-app-groups",
                    div { class: "tray-mic-app-group",
                        div { class: "device-card-copy",
                            h2 { "Currently using mic" }
                        }
                        if mic_using_apps.is_empty() {
                            div { class: "device-empty", "No apps are using the microphone" }
                        } else {
                            div { class: "device-list tray-mic-app-list",
                                for app in mic_using_apps {
                                    MicAppRow {
                                        key: "{app.exe_name}-{app.name}",
                                        app,
                                        ignored_apps: tray_icon.mic_in_use_ignored_apps.clone(),
                                        settings
                                    }
                                }
                            }
                        }
                    }

                    div { class: "tray-mic-app-group",
                        div { class: "device-card-copy",
                            h2 { "Ignored apps" }
                        }
                        if tray_icon.mic_in_use_ignored_apps.is_empty() {
                            div { class: "device-empty", "No ignored apps" }
                        } else {
                            div { class: "device-list tray-mic-app-list",
                                for exe_name in tray_icon.mic_in_use_ignored_apps.clone() {
                                    IgnoredAppRow {
                                        key: "{exe_name}",
                                        exe_name,
                                        settings
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn MicAppRow(
    app: crate::MicUsingApp,
    ignored_apps: Vec<String>,
    settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let ignored = ignored_apps
        .iter()
        .any(|ignored| app.exe_name.eq_ignore_ascii_case(ignored));
    let button_label = if ignored { "Include" } else { "Ignore" };
    let button_class = if ignored {
        "device-action-button default active tray-mic-app-toggle"
    } else {
        "device-action-button tray-mic-app-toggle"
    };
    let exe_name = app.exe_name.clone();

    rsx! {
        div { class: if ignored { "device-list-item tray-mic-app-row ignored" } else { "device-list-item tray-mic-app-row" },
            div { class: "device-leading",
                span { class: "solar-icon device-kind-icon icon-record" }
                div { class: "device-name-block",
                    span { class: "device-name", "{app.name}" }
                    span { class: "device-status", "{app.exe_name}" }
                }
            }
            div { class: "device-actions",
                button {
                    r#type: "button",
                    class: "{button_class}",
                    title: "{button_label}",
                    onclick: move |_| {
                        let target = exe_name.clone();
                        super::super::update_settings(settings, move |config| {
                            if config
                                .tray_icon
                                .mic_in_use_ignored_apps
                                .iter()
                                .any(|ignored| target.eq_ignore_ascii_case(ignored))
                            {
                                config
                                    .tray_icon
                                    .mic_in_use_ignored_apps
                                    .retain(|ignored| !target.eq_ignore_ascii_case(ignored));
                            } else {
                                config.tray_icon.mic_in_use_ignored_apps.push(target);
                            }
                        });
                    },
                    span { "{button_label}" }
                }
            }
        }
    }
}

#[component]
fn IgnoredAppRow(exe_name: String, settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let target = exe_name.clone();

    rsx! {
        div { class: "device-list-item tray-mic-app-row ignored",
            div { class: "device-leading",
                span { class: "solar-icon device-kind-icon icon-record" }
                div { class: "device-name-block",
                    span { class: "device-name", "{exe_name}" }
                }
            }
            div { class: "device-actions",
                button {
                    r#type: "button",
                    class: "device-action-button default active tray-mic-app-toggle",
                    title: "Include",
                    onclick: move |_| {
                        let target = target.clone();
                        super::super::update_settings(settings, move |config| {
                            config
                                .tray_icon
                                .mic_in_use_ignored_apps
                                .retain(|ignored| !target.eq_ignore_ascii_case(ignored));
                        });
                    },
                    span { "Include" }
                }
            }
        }
    }
}
