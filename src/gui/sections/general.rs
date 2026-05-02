use dioxus::prelude::*;

pub fn render(
    mut settings: Signal<super::super::SettingsSnapshot>,
    _recording: Signal<bool>,
) -> Element {
    let snapshot = settings();
    let advanced = snapshot.config.advanced.clone();

    rsx! {
        section {
            class: "general-panel",
            id: "general-status",
            "data-settings-section": "true",
            div { class: "auto-mute-header",
                h1 { "General" }
            }

            section { class: "sound-card startup-card",
                div { class: "sound-card-title startup-row",
                    div { class: "startup-copy",
                        h2 { "Launch with Windows" }
                    }
                    super::Toggle {
                        checked: snapshot.config.startup.launch_on_startup,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.startup.launch_on_startup = checked;
                            });
                        }
                    }
                }
            }

            section { class: "sound-card advanced-card",
                div { class: "sound-card-title advanced-row",
                    div { class: "startup-copy",
                        h2 { "Enable Mica background" }
                    }
                    super::Toggle {
                        checked: advanced.enable_mica,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.advanced.enable_mica = checked;
                            });
                            crate::set_settings_mica_enabled(checked);
                        }
                    }
                }
            }

            section { class: "sound-card advanced-card",
                div { class: "sound-card-title advanced-row",
                    div { class: "startup-copy",
                        h2 { "Disable Tray Icon double-click" }
                        p { "Delay for single click will be removed" }
                    }
                    super::Toggle {
                        checked: advanced.disable_tray_double_click_settings,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.advanced.disable_tray_double_click_settings = checked;
                            });
                        }
                    }
                }
            }

            section {
                class: "general-import-export-panel",
                id: "general-import-export",
                "data-settings-section": "true",
                div { class: "auto-mute-header",
                    h1 { "Import/Export" }
                    p { "Backup your settings" }
                }

                div { class: "general-import-export-grid",
                    button {
                        class: "general-import-export-button",
                        onclick: move |_| {
                            if crate::export_settings().is_ok() {
                                let next = settings.peek().clone().refresh(false);
                                settings.set(next);
                            }
                        },
                        span { class: "solar-icon general-import-export-icon icon-export" }
                        span { "Export" }
                    }
                    button {
                        class: "general-import-export-button",
                        onclick: move |_| {
                            if crate::import_settings().is_ok() {
                                let next = settings.peek().clone().refresh(true);
                                settings.set(next);
                            }
                        },
                        span { class: "solar-icon general-import-export-icon icon-import" }
                        span { "Import" }
                    }
                }

                section { class: "sound-card general-reset-card",
                    div { class: "sound-card-title general-reset-row",
                        div { class: "startup-copy",
                            h2 { "Reset settings" }
                        }
                        button {
                            class: "secondary general-reset-button",
                            onclick: move |_| {
                                if crate::reset_settings().is_ok() {
                                    let next = settings.peek().clone().refresh(true);
                                    settings.set(next);
                                }
                            },
                            "Reset settings"
                        }
                    }
                }
            }
        }
    }
}
