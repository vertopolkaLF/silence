use dioxus::prelude::*;

use crate::gui::controls::{Checkbox, Range, Select, SelectOption};

use super::super::tabs::SettingsTab;

const DEFAULT_SOUND_OPTION: &str = "__default__";

pub fn render(
    settings: Signal<super::super::SettingsSnapshot>,
    mut active_tab: Signal<SettingsTab>,
    mut active_section: Signal<String>,
    mut hotkey_modal_request: Signal<Option<super::super::HotkeyModalRequest>>,
) -> Element {
    let snapshot = settings();
    let sound_settings = snapshot.config.sound_settings.clone();
    let hold_settings = snapshot.config.hold_to_mute.clone();

    let volume = hold_settings
        .volume_override
        .unwrap_or(sound_settings.volume)
        .min(100);
    let play_sounds_enabled = hold_settings.play_sounds;
    let volume_label = if hold_settings.volume_override.is_some() {
        format!("{volume}%")
    } else {
        "Default".to_string()
    };
    let mute_value = select_theme_value(hold_settings.mute_theme_override.as_deref());
    let unmute_value = select_theme_value(hold_settings.unmute_theme_override.as_deref());
    let mute_preview_theme = hold_settings
        .mute_theme_override
        .as_deref()
        .unwrap_or(sound_settings.mute_theme.as_str())
        .to_string();
    let unmute_preview_theme = hold_settings
        .unmute_theme_override
        .as_deref()
        .unwrap_or(sound_settings.unmute_theme.as_str())
        .to_string();

    rsx! {
        section { class: "sounds-panel hold-to-mute-panel",
            div {
                class: "sounds-header section-head-row",
                id: "hold-to-mute-overview",
                "data-settings-section": "true",
                h1 { "Hold to Mute" }
                button {
                    class: "secondary configure-hotkey-button",
                    onclick: move |_| {
                        let section_id = SettingsTab::Hotkeys.first_section_id().to_string();
                        hotkey_modal_request.set(Some(super::super::HotkeyModalRequest {
                            action: crate::HotkeyAction::HoldToMute,
                        }));
                        active_tab.set(SettingsTab::Hotkeys);
                        active_section.set(section_id.clone());
                        super::super::tabs::scroll_to_section(&section_id);
                    },
                    span { class: "solar-icon button-icon icon-keyboard" }
                    "Configure hotkeys"
                }
            }

            section { class: "sound-card",
                div { class: "section-head",
                    h2 { "Options" }
                }
                div { class: "hold-option-list",
                    Checkbox {
                        checked: hold_settings.play_sounds,
                        label: "Play sounds".to_string(),
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.hold_to_mute.play_sounds = checked;
                            });
                        }
                    }
                    Checkbox {
                        checked: hold_settings.show_overlay,
                        label: "Show overlay".to_string(),
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.hold_to_mute.show_overlay = checked;
                            });
                        }
                    }
                }
            }

            div {
                class: if play_sounds_enabled {
                    "sound-settings-collapse open"
                } else {
                    "sound-settings-collapse"
                },
                div { class: "sound-settings-collapse-inner",
                    section { class: "sound-card sound-picker-card",
                        div { class: "section-head",
                            h2 { "Sound Settings" }
                            p { "Configure separate sounds for hold-to-mute or use defaults from the Sounds tab." }
                        }

                        div { class: "sound-picker",
                            div { class: "sound-row-head",
                                h3 { "Volume" }
                                div { class: "sound-meta-row",
                                    span { class: "sound-value", "{volume_label}" }
                                    button {
                                        class: "secondary small-button",
                                        disabled: hold_settings.volume_override.is_none(),
                                        onclick: move |_| {
                                            super::super::update_settings(settings, |config| {
                                                config.hold_to_mute.volume_override = None;
                                            });
                                        },
                                        "Reset"
                                    }
                                }
                            }
                            div { class: "volume-row sound-reset-row",
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
                                                config.hold_to_mute.volume_override = Some(value.min(100));
                                            });
                                        }
                                    }
                                }
                                span { class: "solar-icon icon-volume volume-high" }
                            }
                        }

                        SoundPicker {
                            title: "Mute Sound",
                            value: mute_value,
                            preview_theme: mute_preview_theme,
                            muted: true,
                            volume,
                            has_override: hold_settings.mute_theme_override.is_some(),
                            settings,
                        }

                        SoundPicker {
                            title: "Unmute Sound",
                            value: unmute_value,
                            preview_theme: unmute_preview_theme,
                            muted: false,
                            volume,
                            has_override: hold_settings.unmute_theme_override.is_some(),
                            settings,
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SoundPicker(
    title: &'static str,
    value: String,
    preview_theme: String,
    muted: bool,
    volume: u8,
    has_override: bool,
    settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let theme_options = std::iter::once(SelectOption::new(
        DEFAULT_SOUND_OPTION,
        "Default (from Sounds tab)",
    ))
    .chain(
        crate::sound_themes()
            .iter()
            .map(|theme| SelectOption::new(theme.id, theme.label).icon("icon-volume")),
    )
    .collect::<Vec<_>>();

    rsx! {
        div { class: "sound-picker",
            label { "{title}" }
            div { class: "sound-select-row sound-reset-row",
                Select {
                    class: "sound-select-wrap".to_string(),
                    value: value.clone(),
                    options: theme_options,
                    onchange: move |value: String| {
                        let next_value = if value == DEFAULT_SOUND_OPTION {
                            None
                        } else {
                            Some(value)
                        };
                        super::super::update_settings(settings, |config| {
                            if muted {
                                config.hold_to_mute.mute_theme_override = next_value;
                            } else {
                                config.hold_to_mute.unmute_theme_override = next_value;
                            }
                        });
                    }
                }
                button {
                    class: "icon-button preview-button",
                    title: "Preview {title}",
                    onclick: move |_| {
                        let _ = crate::preview_sound(&preview_theme, muted, volume);
                    },
                    span { class: "solar-icon icon-play" }
                }
                button {
                    class: "secondary small-button",
                    disabled: !has_override,
                    onclick: move |_| {
                        super::super::update_settings(settings, |config| {
                            if muted {
                                config.hold_to_mute.mute_theme_override = None;
                            } else {
                                config.hold_to_mute.unmute_theme_override = None;
                            }
                        });
                    },
                    "Reset"
                }
            }
        }
    }
}

fn select_theme_value(theme: Option<&str>) -> String {
    theme.unwrap_or(DEFAULT_SOUND_OPTION).to_string()
}
