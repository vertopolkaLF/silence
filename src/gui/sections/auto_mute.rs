use dioxus::prelude::*;

use crate::gui::controls::Checkbox;

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let auto_mute = snapshot.config.auto_mute.clone();
    let inactivity_enabled = auto_mute.after_inactivity_enabled;
    let inactivity_minutes = auto_mute.after_inactivity_minutes.clamp(1, 1440);
    let inactivity_group_class = if inactivity_enabled {
        "auto-mute-subgroup"
    } else {
        "auto-mute-subgroup disabled"
    };

    rsx! {
        section {
            class: "auto-mute-panel",
            id: "auto-mute-overview",
            "data-settings-section": "true",

            div { class: "auto-mute-header",
                h1 { "Auto-Mute" }
                p { "Automatically mute the microphone on startup or after inactivity." }
            }

            section { class: "sound-card auto-mute-card",
                div { class: "section-head",
                    h2 { "Startup" }
                }
                Checkbox {
                    checked: auto_mute.mute_on_startup,
                    label: "Mute microphone on app startup".to_string(),
                    onchange: move |checked| {
                        super::super::update_settings(settings, |config| {
                            config.auto_mute.mute_on_startup = checked;
                        });
                    }
                }
            }

            section { class: "sound-card auto-mute-card",
                div { class: "section-head",
                    h2 { "Inactivity" }
                }
                div { class: "auto-mute-group",
                    Checkbox {
                        checked: inactivity_enabled,
                        label: "Mute microphone after inactivity".to_string(),
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.auto_mute.after_inactivity_enabled = checked;
                            });
                        }
                    }

                    div { class: "{inactivity_group_class}",
                        label { class: "auto-mute-number-label", "Minutes without keyboard or mouse activity" }
                        input {
                            class: "auto-mute-number",
                            r#type: "number",
                            min: "1",
                            max: "1440",
                            step: "1",
                            value: "{inactivity_minutes}",
                            disabled: !inactivity_enabled,
                            oninput: move |evt| {
                                if let Ok(value) = evt.value().parse::<u16>() {
                                    super::super::update_settings(settings, |config| {
                                        config.auto_mute.after_inactivity_minutes = value.clamp(1, 1440);
                                    });
                                }
                            }
                        }
                        p { class: "auto-mute-note", "If no keyboard or mouse activity is detected for this long, the microphone will be muted." }
                        Checkbox {
                            checked: auto_mute.unmute_on_activity,
                            label: "Unmute on activity".to_string(),
                            disabled: !inactivity_enabled,
                            onchange: move |checked| {
                                super::super::update_settings(settings, |config| {
                                    config.auto_mute.unmute_on_activity = checked;
                                });
                            }
                        }
                    }
                }
            }

            section { class: "sound-card auto-mute-card",
                div { class: "section-head",
                    h2 { "Options" }
                }
                div { class: "auto-mute-group",
                    Checkbox {
                        checked: auto_mute.play_sounds,
                        label: "Play sounds on auto-mute".to_string(),
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.auto_mute.play_sounds = checked;
                            });
                        }
                    }
                    p { class: "auto-mute-note", "If disabled, auto-mute stays silent. Overlay behavior still follows your overlay settings." }
                }
            }
        }
    }
}
