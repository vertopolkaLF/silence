use dioxus::prelude::*;

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let advanced = snapshot.config.advanced.clone();

    rsx! {
        section {
            class: "advanced-panel",
            id: "advanced-overview",
            "data-settings-section": "true",
            div { class: "auto-mute-header",
                h1 { "Advanced" }
            }

            section { class: "sound-card advanced-card",
                div { class: "sound-card-title advanced-row",
                    div { class: "startup-copy",
                        h2 { "Enable Mica background" }
                        p { "Uses Windows backdrop material behind the settings window." }
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
                        h2 { "Disable double-click to open settings" }
                        p { "this will remove delay between single click and mic toggle" }
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
