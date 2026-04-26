use dioxus::prelude::*;

pub fn render(
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    mut sound_settings: Signal<crate::SoundSettings>,
    overlay: Signal<crate::OverlayConfig>,
) -> Element {
    let settings = sound_settings();
    let volume = settings.volume.min(100);
    let mute_theme = settings.mute_theme.clone();
    let unmute_theme = settings.unmute_theme.clone();
    let mute_label = crate::sound_theme_label(&mute_theme);
    let unmute_label = crate::sound_theme_label(&unmute_theme);

    rsx! {
        section { class: "sounds-panel",
            div { class: "sounds-header",
                h1 { "Sounds" }
            }

            section { class: "sound-card",
                div { class: "sound-card-title",
                    h2 { "Enable sounds" }
                    super::Toggle {
                        checked: settings.enabled,
                        onchange: move |checked| {
                            let mut next = sound_settings();
                            next.enabled = checked;
                            sound_settings.set(next);
                            save_sounds(shortcut, mic_device_id, sound_settings, overlay);
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
                                let mut next = sound_settings();
                                next.volume = value.min(100);
                                sound_settings.set(next);
                                save_sounds(shortcut, mic_device_id, sound_settings, overlay);
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
                    shortcut,
                    mic_device_id,
                    sound_settings,
                    overlay
                }
                SoundPicker {
                    title: "Unmute Sound",
                    value: unmute_theme.clone(),
                    label: unmute_label,
                    muted: false,
                    volume,
                    shortcut,
                    mic_device_id,
                    sound_settings,
                    overlay
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
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    mut sound_settings: Signal<crate::SoundSettings>,
    overlay: Signal<crate::OverlayConfig>,
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
                            let mut next = sound_settings();
                            let value = evt.value();
                            if muted {
                                next.mute_theme = value;
                            } else {
                                next.unmute_theme = value;
                            }
                            sound_settings.set(next);
                            save_sounds(shortcut, mic_device_id, sound_settings, overlay);
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

fn save_sounds(
    shortcut: Signal<crate::Shortcut>,
    mic_device_id: Signal<Option<String>>,
    sound_settings: Signal<crate::SoundSettings>,
    overlay: Signal<crate::OverlayConfig>,
) {
    let mut config = crate::load_config().unwrap_or_default();
    config.shortcut = shortcut();
    config.mic_device_id = mic_device_id();
    config.sound_settings = sound_settings();
    config.overlay = overlay();
    let _ = crate::save_config(&config);
}
