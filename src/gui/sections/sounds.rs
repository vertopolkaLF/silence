use dioxus::prelude::*;

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
                    input {
                        class: "volume-slider",
                        r#type: "range",
                        min: "0",
                        max: "100",
                        step: "1",
                        value: "{volume}",
                        style: "--range-progress: {volume}%;",
                        oninput: move |evt| {
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
    rsx! {
        div { class: "sound-picker",
            label { "{title}" }
            div { class: "sound-select-row",
                div { class: "select-wrap sound-select-wrap",
                    select {
                        class: "select-like sound-select",
                        value: "{value}",
                        onchange: move |evt| {
                            let value = evt.value();
                            super::super::update_settings(settings, |config| {
                                if muted {
                                    config.sound_settings.mute_theme = value;
                                } else {
                                    config.sound_settings.unmute_theme = value;
                                }
                            });
                        },
                        for theme in crate::sound_themes() {
                            option {
                                value: "{theme.id}",
                                selected: value == theme.id,
                                "{theme.label}"
                            }
                        }
                    }
                    span { class: "solar-icon select-icon icon-down" }
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
