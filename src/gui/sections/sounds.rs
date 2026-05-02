use dioxus::prelude::*;

use crate::gui::controls::{Range, Select, SelectOption};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let snapshot = settings();
    let sound_settings = snapshot.config.sound_settings.clone();
    let volume = sound_settings.volume.min(100);
    let mute_theme = sound_settings.mute_theme.clone();
    let unmute_theme = sound_settings.unmute_theme.clone();
    let mute_label = crate::sound_selection_label(&mute_theme, &sound_settings).to_string();
    let unmute_label = crate::sound_selection_label(&unmute_theme, &sound_settings).to_string();
    let custom_sounds = sound_settings.custom_sounds.clone();
    let playing_preview = use_signal(|| None::<String>);

    rsx! {
        section { class: "sounds-panel",
            div { class: "sounds-header section-head-row",
                id: "sounds-overview",
                "data-settings-section": "true",
                h1 { "Sounds" }
                    super::Toggle {
                        checked: sound_settings.enabled,
                        onchange: move |checked| {
                            super::super::update_settings(settings, |config| {
                                config.sound_settings.enabled = checked;
                            });
                        }
                    }
                }

            section { class: "sound-card",
                Range {
                    label: "Volume".to_string(),
                    value_label: format!("{volume}%"),
                    value: volume.to_string(),
                    min: "0".to_string(),
                    max: "100".to_string(),
                    step: "1".to_string(),
                    progress: format!("{volume}%"),
                    label_icon: Some("icon-volume".to_string()),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<u8>() {
                            super::super::update_settings(settings, |config| {
                                config.sound_settings.volume = value.min(100);
                            });
                        }
                    }
                }
            }

            section { class: "sound-card sound-picker-card",
                SoundPicker {
                    title: "Mute Sound",
                    value: mute_theme.clone(),
                    label: mute_label,
                    custom_sounds: custom_sounds.clone(),
                    muted: true,
                    volume,
                    playing_preview,
                    settings
                }
                SoundPicker {
                    title: "Unmute Sound",
                    value: unmute_theme.clone(),
                    label: unmute_label,
                    custom_sounds: custom_sounds.clone(),
                    muted: false,
                    volume,
                    playing_preview,
                    settings
                }
            }

            section { class: "sound-card sound-files-card",
                div { class: "sound-row-head",
                    div {
                        h2 { "Files" }
                        p { class: "sound-card-copy", "Add custom sounds" }
                    }
                    button {
                        class: "secondary small-button",
                        onclick: move |_| {
                            match crate::choose_custom_sounds() {
                                Ok(custom_sounds) if !custom_sounds.is_empty() => {
                                    super::super::update_settings(settings, |config| {
                                        config.sound_settings.custom_sounds.extend(custom_sounds);
                                    });
                                }
                                Ok(_) => {}
                                Err(err) => eprintln!("failed to add custom sounds: {err:?}"),
                            }
                        },
                        span { class: "solar-icon button-icon icon-plus" }
                        "Add sounds"
                    }
                }
                div { class: "custom-sound-library",
                        for custom_sound in custom_sounds {
                            CustomSoundFile {
                                custom_sound,
                                settings,
                                volume,
                                playing_preview
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
    label: String,
    custom_sounds: Vec<crate::CustomSound>,
    muted: bool,
    volume: u8,
    playing_preview: Signal<Option<String>>,
    settings: Signal<super::super::SettingsSnapshot>,
) -> Element {
    let theme_options = sound_options(&custom_sounds)
        .into_iter()
        .map(|option| option.end_icon("icon-play"))
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
                        toggle_preview(
                            preview_key(&value, muted),
                            value,
                            muted,
                            volume,
                            playing_preview,
                        );
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
                PreviewButton {
                    title: format!("Preview {label}"),
                    preview_key: preview_key(&value, muted),
                    selection: value,
                    muted,
                    volume,
                    playing_preview
                }
            }
        }
    }
}

#[component]
fn CustomSoundFile(
    custom_sound: crate::CustomSound,
    settings: Signal<super::super::SettingsSnapshot>,
    volume: u8,
    playing_preview: Signal<Option<String>>,
) -> Element {
    let id = custom_sound.id.clone();
    let value = format!("custom:{id}");
    let file_name = custom_sound.original_file_name.clone();

    rsx! {
        div { class: "custom-sound-file",
            div { class: "custom-sound-copy",
                span { class: "custom-sound-name", "{file_name}" }
            }
            div { class: "custom-sound-actions",
                PreviewButton {
                    title: format!("Preview {file_name}"),
                    preview_key: preview_key(&value, true),
                    selection: value,
                    muted: true,
                    volume,
                    playing_preview
                }
                button {
                    class: "icon-button custom-sound-remove",
                    title: "Remove {file_name}",
                    onclick: move |_| {
                        super::super::update_settings(settings, |config| {
                            config.sound_settings.custom_sounds.retain(|sound| sound.id != id);
                            let removed_value = format!("custom:{id}");
                            if config.sound_settings.mute_theme == removed_value {
                                config.sound_settings.mute_theme = "8bit".to_string();
                            }
                            if config.sound_settings.unmute_theme == removed_value {
                                config.sound_settings.unmute_theme = "8bit".to_string();
                            }
                            if config.hold_to_mute.mute_theme_override.as_deref() == Some(removed_value.as_str()) {
                                config.hold_to_mute.mute_theme_override = None;
                            }
                            if config.hold_to_mute.unmute_theme_override.as_deref() == Some(removed_value.as_str()) {
                                config.hold_to_mute.unmute_theme_override = None;
                            }
                        });
                    },
                    span { class: "solar-icon icon-trash" }
                }
            }
        }
    }
}

#[component]
pub(crate) fn PreviewButton(
    title: String,
    preview_key: String,
    selection: String,
    muted: bool,
    volume: u8,
    playing_preview: Signal<Option<String>>,
) -> Element {
    let is_playing = playing_preview().as_deref() == Some(preview_key.as_str());
    let icon_class = if is_playing {
        "icon-pause"
    } else {
        "icon-play"
    };

    rsx! {
        button {
            class: "icon-button preview-button",
            title,
            onclick: move |_| {
                toggle_preview(
                    preview_key.clone(),
                    selection.clone(),
                    muted,
                    volume,
                    playing_preview,
                );
            },
            span { class: "solar-icon {icon_class}" }
        }
    }
}

pub(crate) fn toggle_preview(
    preview_key: String,
    selection: String,
    muted: bool,
    volume: u8,
    mut playing_preview: Signal<Option<String>>,
) {
    if playing_preview.peek().as_deref() == Some(preview_key.as_str()) {
        crate::stop_preview_sound();
        playing_preview.set(None);
        return;
    }

    match crate::preview_sound(&selection, muted, volume) {
        Ok(duration_ms) => {
            playing_preview.set(Some(preview_key.clone()));
            spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(duration_ms)).await;
                if playing_preview.peek().as_deref() == Some(preview_key.as_str()) {
                    playing_preview.set(None);
                }
            });
        }
        Err(err) => eprintln!("failed to preview sound: {err:?}"),
    }
}

pub(crate) fn preview_key(selection: &str, muted: bool) -> String {
    format!("{}:{selection}", if muted { "mute" } else { "unmute" })
}

pub(crate) fn sound_options(custom_sounds: &[crate::CustomSound]) -> Vec<SelectOption> {
    custom_sounds
        .iter()
        .map(|sound| {
            SelectOption::new(
                format!("custom:{}", sound.id),
                sound.original_file_name.clone(),
            )
            .group("Custom Sounds")
            .icon("icon-volume")
        })
        .chain(crate::sound_themes().iter().map(|theme| {
            SelectOption::new(theme.id, theme.label)
                .group("Built-in Sounds")
                .icon("icon-volume")
        }))
        .collect::<Vec<_>>()
}
