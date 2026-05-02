use dioxus::prelude::*;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmAction {
    ImportV1,
    Reset,
}

pub fn render(
    mut settings: Signal<super::super::SettingsSnapshot>,
    _recording: Signal<bool>,
) -> Element {
    let snapshot = settings();
    let advanced = snapshot.config.advanced.clone();
    let mut confirm_action = use_signal(|| None::<ConfirmAction>);
    let mut confirm_closing = use_signal(|| false);

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
                        h2 { "Double-click tray icon to open Settings" }
                        p { "Disable to remove delay from muting mic with single-click" }
                    }
                    super::Toggle {
                        checked: !advanced.disable_tray_double_click_settings,
                        onchange: move |checked: bool| {
                            super::super::update_settings(settings, |config| {
                                config.advanced.disable_tray_double_click_settings = !checked;
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
                            h2 { "Import from silence! v.1" }
                        }
                        button {
                            class: "secondary general-reset-button",
                            onclick: move |_| {
                                confirm_closing.set(false);
                                confirm_action.set(Some(ConfirmAction::ImportV1));
                            },
                            "Import"
                        }
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
                                confirm_closing.set(false);
                                confirm_action.set(Some(ConfirmAction::Reset));
                            },
                            "Reset settings"
                        }
                    }
                }
            }
        }

        if let Some(action) = confirm_action() {
            div {
                class: if confirm_closing() {
                    "general-confirm-backdrop exiting"
                } else {
                    "general-confirm-backdrop"
                },
                onclick: move |_| close_confirm_modal(confirm_action, confirm_closing),
                div {
                    class: if confirm_closing() {
                        "general-confirm-modal exiting"
                    } else {
                        "general-confirm-modal"
                    },
                    onclick: move |evt| evt.stop_propagation(),
                    div { class: "general-confirm-copy",
                        h2 {
                            match action {
                                ConfirmAction::ImportV1 => "Import from silence! v.1?",
                                ConfirmAction::Reset => "Reset all settings?",
                            }
                        }
                        p {
                            match action {
                                ConfirmAction::ImportV1 => "Current settings will be replaced by converted v1 settings from the old app data folder.",
                                ConfirmAction::Reset => "Current settings will be replaced with defaults immediately.",
                            }
                        }
                    }
                    div { class: "general-confirm-actions",
                        button {
                            class: "secondary",
                            onclick: move |_| close_confirm_modal(confirm_action, confirm_closing),
                            "Cancel"
                        }
                        button {
                            class: "secondary general-confirm-danger",
                            onclick: move |_| {
                                let result = match action {
                                    ConfirmAction::ImportV1 => crate::import_v1_settings(),
                                    ConfirmAction::Reset => crate::reset_settings(),
                                };
                                if result.is_ok() {
                                    let next = settings.peek().clone().refresh(true);
                                    settings.set(next);
                                    close_confirm_modal(confirm_action, confirm_closing);
                                }
                            },
                            match action {
                                ConfirmAction::ImportV1 => "Import",
                                ConfirmAction::Reset => "Reset settings",
                            }
                        }
                    }
                }
            }
        }
    }
}

fn close_confirm_modal(
    mut confirm_action: Signal<Option<ConfirmAction>>,
    mut confirm_closing: Signal<bool>,
) {
    if confirm_closing() {
        return;
    }
    confirm_closing.set(true);
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(170)).await;
        confirm_action.set(None);
        confirm_closing.set(false);
    });
}
