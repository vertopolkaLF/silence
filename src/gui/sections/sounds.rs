use dioxus::prelude::*;

use crate::gui::controls::{Range, Select, SelectOption};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let sound_settings = snapshot.config.sound_settings.clone();
    let volume = sound_settings.volume.min(100);
    let mute_theme = sound_settings.mute_theme.clone();
    let unmute_theme = sound_settings.unmute_theme.clone();
    let mute_label = crate::sound_theme_label(&mute_theme);
    let unmute_label = crate::sound_theme_label(&unmute_theme);

    rsx! {
        section { class: "sounds-panel",
            div { class: "sounds-header",
                id: "sounds-overview",
                "data-settings-section": "true",
                h1 { "Sounds" }
            }

            section { class: "sound-card",
                div { class: "sound-card-title",
                    h2 { "Enable sounds" }
                    super::Toggle {
                        checked: sound_settings.enabled,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.sound_settings.enabled = checked;
                            });
                        }
                    }
                }
            }

            section { class: "sound-card",
                div { class: "sound-row-head",
                    h2 { "Volume" }
                    span { class: "sound-value", "{volume}%" }
                }
                div { class: "volume-row",
                    span { class: "solar-icon icon-volume volume-low" }
                    Range {
                        value: volume.to_string(),
                        min: "0".to_string(),
                        max: "100".to_string(),
                        step: "1".to_string(),
                        progress: format!("{volume}%"),
                        oninput: move |evt: FormEvent| {
                            if let Ok(value) = evt.value().parse::<u8>() {
                                super::super::update_settings(settings, |config| {
                                    config.sound_settings.volume = value.min(100);
                                });
                            }
                        }
                    }
                    span { class: "solar-icon icon-volume volume-high" }
                }
            }

            section { class: "sound-card sound-picker-card",
                SoundPicker {
                    title: "Mute Sound",
                    value: mute_theme.clone(),
                    label: mute_label,
                    muted: true,
                    volume,
                    settings
                }
                SoundPicker {
                    title: "Unmute Sound",
                    value: unmute_theme.clone(),
                    label: unmute_label,
                    muted: false,
                    volume,
                    settings
                }
            }
        }
    }
}

#[component]
fn SoundPicker(
    title: &'static str,
    value: String,
    label: &'static str,
    muted: bool,
    volume: u8,
    settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let theme_options = crate::sound_themes()
        .iter()
        .map(|theme| {
            SelectOption::new(theme.id, theme.label)
                .icon("icon-volume")
                .end_icon("icon-play")
        })
        .collect::<Vec<_>>();

    rsx! {
        div { class: "sound-picker",
            label { "{title}" }
            div { class: "sound-select-row",
                Select {
                    class: "sound-select-wrap".to_string(),
                    value: value.clone(),
                    options: theme_options,
                    on_option_action: move |value: String| {
                        let _ = crate::preview_sound(&value, muted, volume);
                    },
                    onchange: move |value: String| {
                        super::super::update_settings(settings, |config| {
                            if muted {
                                config.sound_settings.mute_theme = value;
                            } else {
                                config.sound_settings.unmute_theme = value;
                            }
                        });
                    }
                }
                button {
                    class: "icon-button preview-button",
                    title: "Preview {label}",
                    onclick: move |_| {
                        let _ = crate::preview_sound(&value, muted, volume);
                    },
                    span { class: "solar-icon icon-play" }
                }
            }
        }
    }
}
