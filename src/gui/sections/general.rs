use dioxus::prelude::*;

pub fn render(
    settings: Signal<super::super::SettingsSnapshot>,
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
        }
    }
}
