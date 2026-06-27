use std::time::Duration;

use dioxus::prelude::*;

use crate::gui::controls::{
    Checkbox, Range, SegmentedToggle, SegmentedToggleOption, Select, SelectOption,
};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut positioning = use_signal(|| false);
    let mut icons_expanded = use_signal(|| false);
    let mut action_editor = use_signal(|| None::<OverlayActionEditorMode>);
    let snapshot = settings();
    let overlay = snapshot.config.overlay.clone();
    let duration = format!("{:.1}", overlay.duration_secs.clamp(0.5, 10.0));
    let x = format!("{:.0}", overlay.position_x.clamp(0.0, 100.0));
    let y = format!("{:.0}", overlay.position_y.clamp(0.0, 100.0));
    let scale = overlay.scale.clamp(10, 400);
    let content_opacity = overlay.content_opacity.clamp(20, 100);
    let background_opacity = overlay.background_opacity.min(100);
    let border_radius = overlay.border_radius.min(24);
    let text_font_weight = overlay.text_font_weight.clamp(100, 900);
    let has_icon = matches!(overlay.variant.as_str(), "MicIcon" | "IconText");
    let has_text = matches!(overlay.variant.as_str(), "IconText" | "Text");
    let icon_controls_open = has_icon;
    let text_controls_open = has_text;
    let duration_controls_open = overlay.visibility == "AfterToggle";
    let preview_muted = snapshot.muted;
    let devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
    let hotkeys = snapshot.config.hotkeys.clone();
    let audio_device_name_display = snapshot.config.advanced.audio_device_name_display.clone();
    let preview_tone_class = if preview_muted { "muted" } else { "live" };
    let duration_progress = format!(
        "{:.0}%",
        (overlay.duration_secs.clamp(0.5, 10.0) - 0.5) / 9.5 * 100.0
    );
    let x_progress = format!("{:.0}%", overlay.position_x.clamp(0.0, 100.0));
    let y_progress = format!("{:.0}%", overlay.position_y.clamp(0.0, 100.0));
    let scale_progress = format!("{:.0}%", (scale as f64 - 10.0) / 390.0 * 100.0);
    let content_opacity_progress =
        format!("{:.0}%", (content_opacity as f64 - 20.0) / 80.0 * 100.0);
    let background_opacity_progress = format!("{background_opacity}%");
    let border_radius_progress = format!("{:.0}%", border_radius as f64 / 24.0 * 100.0);
    let text_font_weight_progress =
        format!("{:.0}%", (text_font_weight as f64 - 100.0) / 800.0 * 100.0);
    let visibility_options = vec![
        SelectOption::new("Always", "Always visible").icon("icon-widget"),
        SelectOption::new("WhenMuted", "Visible when muted").icon("icon-mic-muted"),
        SelectOption::new("WhenUnmuted", "Visible when unmuted").icon("icon-mic-lucide"),
        SelectOption::new("WhenMicInUse", "Visible while mic is used").icon("icon-record"),
        SelectOption::new("AfterToggle", "Show after toggle").icon("icon-clock-circle"),
    ];
    let display_options = snapshot
        .overlay_displays
        .iter()
        .map(|display| {
            SelectOption::new(display.id.clone(), display.label.clone())
                .detail(display.detail.clone())
                .icon("icon-monitor")
        })
        .collect::<Vec<_>>();
    let icon_style_options = vec![
        SelectOption::new("Colored", "Colored").icon("icon-palette"),
        SelectOption::new("Monochrome", "Monochrome").icon("icon-contrast"),
        SelectOption::new("SystemColor", "System color").icon("icon-widget"),
    ];
    let background_options = vec![
        SelectOption::new("Dark", "Dark").icon("icon-moon"),
        SelectOption::new("Light", "Light").icon("icon-sun"),
    ];
    let mut font_options = snapshot
        .system_fonts
        .iter()
        .map(|font| {
            SelectOption::new(font.family.clone(), font.family.clone())
                .font_family(font.family.clone())
        })
        .collect::<Vec<_>>();
    if !overlay.text_font.is_empty()
        && !font_options
            .iter()
            .any(|option| option.value.eq_ignore_ascii_case(&overlay.text_font))
    {
        font_options.insert(
            0,
            SelectOption::new(overlay.text_font.clone(), overlay.text_font.clone())
                .font_family(overlay.text_font.clone()),
        );
    }

    rsx! {
        section {
            class: "overlay-panel",
            id: "overlay-overview",
            "data-settings-section": "true",
            div { class: "overlay-header section-head-row",
                h1 { "Overlay" }
                super::Toggle {
                    checked: overlay.enabled,
                    onchange: move |checked| {
                        super::super::update_settings(settings, |config| {
                            config.overlay.enabled = checked;
                        });
                    }
                }
            }

            section { class: "sound-card",
                div { class: "overlay-field",
                    label { "Visibility" }
                    Select {
                        value: overlay.visibility.clone(),
                        options: visibility_options,
                        onchange: move |value: String| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.visibility = value;
                            });
                        }
                    }
                }

                div {
                    class: if duration_controls_open { "overlay-collapse open" } else { "overlay-collapse" },
                    div { class: "overlay-collapse-inner",
                        Range {
                            label: "Duration".to_string(),
                            value_label: format!("{duration}s"),
                            value: duration.clone(),
                            min: "0.5".to_string(),
                            max: "10".to_string(),
                            step: "0.5".to_string(),
                            progress: duration_progress.clone(),
                            oninput: move |evt: FormEvent| {
                                if let Ok(value) = evt.value().parse::<f64>() {
                                    super::super::update_settings(settings, |config| {
                                        config.overlay.duration_secs = value.clamp(0.5, 10.0);
                                    });
                                }
                            }
                        }
                    }
                }

                div { class: "overlay-field",
                    label { "Display" }
                    Select {
                        value: overlay.display.clone(),
                        options: display_options,
                        onchange: move |value: String| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.display = value;
                            });
                        }
                    }
                }

                Range {
                    label: "Horizontal position".to_string(),
                    value_label: format!("{x}%"),
                    value: x.clone(),
                    min: "0".to_string(),
                    max: "100".to_string(),
                    step: "1".to_string(),
                    progress: x_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<f64>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.position_x = value.clamp(0.0, 100.0);
                            });
                        }
                    }
                }

                Range {
                    label: "Vertical position".to_string(),
                    value_label: format!("{y}%"),
                    value: y.clone(),
                    min: "0".to_string(),
                    max: "100".to_string(),
                    step: "1".to_string(),
                    progress: y_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<f64>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.position_y = value.clamp(0.0, 100.0);
                            });
                        }
                    }
                }

                div { class: "sound-card-title",
                    div {
                        h2 { "Move overlay" }
                        p { "Shows the overlay until this is turned off." }
                    }
                    super::Toggle {
                        checked: positioning(),
                        onchange: move |checked| {
                            positioning.set(checked);
                            if let Some(next) = crate::set_overlay_positioning(checked) {
                                super::super::update_settings(settings, |config| {
                                    config.overlay = next;
                                });
                            }
                        }
                    }
                }
            }

            section { class: "sound-card overlay-appearance",
                id: "overlay-appearance",
                "data-settings-section": "true",
                div { class: "section-head section-head-row", h1 { "Appearance" } }

                div { class: "overlay-field",
                    label { "Overlay style" }
                    div { class: "overlay-variant-grid",
                        button {
                            class: if overlay.variant == "MicIcon" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "MicIcon".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview live",
                                span { class: "solar-icon icon-mic" }
                            }
                            span { "Mic Icon" }
                        }
                        button {
                            class: if overlay.variant == "IconText" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "IconText".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview icon-text live",
                                span { class: "solar-icon icon-mic" }
                                span { "On" }
                            }
                            span { "Icon + Text" }
                        }
                        button {
                            class: if overlay.variant == "Text" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "Text".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview text-only",
                                span { "On" }
                            }
                            span { "Text" }
                        }
                        button {
                            class: if overlay.variant == "Dot" {
                                "overlay-icon-option overlay-variant-option active"
                            } else {
                                "overlay-icon-option overlay-variant-option"
                            },
                            onclick: move |_| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.variant = "Dot".to_string();
                                });
                            },
                            span { class: "overlay-icon-preview overlay-variant-preview dot",
                                span {}
                            }
                            span { "Dot" }
                        }
                    }
                }

                div {
                    class: if icon_controls_open { "overlay-collapse open" } else { "overlay-collapse" },
                    div { class: "overlay-collapse-inner",
                        div { class: "overlay-field overlay-icon-field",
                            label { "Mic icons" }
                            div { class: "overlay-icon-grid overlay-icon-grid-primary",
                                for pair in crate::overlay_icons::featured_overlay_icon_pairs().iter() {
                                    button {
                                        class: if overlay.icon_pair == pair.id {
                                            "overlay-icon-option active"
                                        } else {
                                            "overlay-icon-option"
                                        },
                                        onclick: {
                                            let id = pair.id.to_string();
                                            move |_| {
                                                let next_id = id.clone();
                                                super::super::update_settings(settings, move |config| {
                                                    config.overlay.icon_pair = next_id;
                                                });
                                            }
                                        },
                                        title: "{pair.label}",
                                        span { class: "overlay-icon-preview {preview_tone_class}",
                                            span {
                                                class: "solar-icon",
                                                style: format!(
                                                    "--icon: url('{}');",
                                                    crate::overlay_icons::overlay_icon_css_url(
                                                        pair.id,
                                                        preview_muted,
                                                    ),
                                                )
                                            }
                                        }
                                        span { "{pair.label}" }
                                    }
                                }
                                button {
                                    class: if icons_expanded() {
                                        "overlay-icon-option overlay-icon-toggle expanded"
                                    } else {
                                        "overlay-icon-option overlay-icon-toggle"
                                    },
                                    title: if icons_expanded() { "Collapse icons" } else { "Expand icons" },
                                    onclick: move |_| icons_expanded.set(!icons_expanded()),
                                    span { class: "overlay-icon-preview",
                                        span { class: "solar-icon icon-down overlay-icon-toggle-glyph" }
                                    }
                                    span { if icons_expanded() { "Collapse" } else { "Expand" } }
                                }
                            }
                            div {
                                class: if icons_expanded() {
                                    "overlay-collapse open overlay-icon-extra-wrap"
                                } else {
                                    "overlay-collapse overlay-icon-extra-wrap"
                                },
                                div { class: "overlay-collapse-inner",
                                    div { class: "overlay-icon-grid overlay-icon-grid-extra",
                                        for pair in crate::overlay_icons::extra_overlay_icon_pairs().iter() {
                                            button {
                                                class: if overlay.icon_pair == pair.id {
                                                    "overlay-icon-option active"
                                                } else {
                                                    "overlay-icon-option"
                                                },
                                                onclick: {
                                                    let id = pair.id.to_string();
                                                    move |_| {
                                                        let next_id = id.clone();
                                                        super::super::update_settings(settings, move |config| {
                                                            config.overlay.icon_pair = next_id;
                                                        });
                                                    }
                                                },
                                                title: "{pair.label}",
                                                span { class: "overlay-icon-preview {preview_tone_class}",
                                                    span {
                                                        class: "solar-icon",
                                                        style: format!(
                                                            "--icon: url('{}');",
                                                            crate::overlay_icons::overlay_icon_css_url(
                                                                pair.id,
                                                                preview_muted,
                                                            ),
                                                        )
                                                    }
                                                }
                                                span { "{pair.label}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                        div {
                    class: if text_controls_open {
                                "overlay-collapse open overlay-label-collapse"
                            } else {
                                "overlay-collapse overlay-label-collapse"
                            },
                            div { class: "overlay-collapse-inner",
                                div { class: "overlay-label-fields",
                                    label { class: "overlay-text-field",
                                        span { "Muted label" }
                                        input {
                                            class: "overlay-text-input",
                                            r#type: "text",
                                            value: "{overlay.muted_label}",
                                            oninput: move |evt| {
                                                let next_label = evt.value();
                                                super::super::update_settings(settings, move |config| {
                                                    config.overlay.muted_label = next_label;
                                                });
                                            }
                                        }
                                    }
                                    label { class: "overlay-text-field",
                                        span { "Unmuted label" }
                                        input {
                                            class: "overlay-text-input",
                                            r#type: "text",
                                            value: "{overlay.unmuted_label}",
                                            oninput: move |evt| {
                                                let next_label = evt.value();
                                                super::super::update_settings(settings, move |config| {
                                                    config.overlay.unmuted_label = next_label;
                                                });
                                            }
                                        }
                                    }
                                }
                                div { class: "overlay-font-controls",
                                    div { class: "overlay-field",
                                        label { "Font" }
                                        Select {
                                            value: overlay.text_font.clone(),
                                            options: font_options,
                                            searchable: true,
                                            onchange: move |value: String| {
                                                super::super::update_settings(settings, |config| {
                                                    config.overlay.text_font = value;
                                                });
                                            }
                                        }
                                    }
                                    Range {
                                        label: "Font weight".to_string(),
                                        value_label: text_font_weight.to_string(),
                                        value: text_font_weight.to_string(),
                                        min: "100".to_string(),
                                        max: "900".to_string(),
                                        step: "100".to_string(),
                                        progress: text_font_weight_progress.clone(),
                                        oninput: move |evt: FormEvent| {
                                            if let Ok(value) = evt.value().parse::<u16>() {
                                                super::super::update_settings(settings, |config| {
                                                    config.overlay.text_font_weight = value.clamp(100, 900);
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }

                div { class: "overlay-select-grid",
                    div { class: if has_icon { "overlay-field" } else { "overlay-field disabled" },
                        label { "Icon style" }
                        Select {
                            value: overlay.icon_style.clone(),
                            options: icon_style_options,
                            disabled: !has_icon,
                            onchange: move |value: String| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.icon_style = value;
                                });
                            }
                        }
                    }
                    div { class: "overlay-field",
                        label { "Background" }
                        Select {
                            value: overlay.background_style.clone(),
                            options: background_options,
                            onchange: move |value: String| {
                                super::super::update_settings(settings, |config| {
                                    config.overlay.background_style = value;
                                });
                            }
                        }
                    }
                }

                Range {
                    label: "Background opacity".to_string(),
                    value_label: format!("{background_opacity}%"),
                    value: background_opacity.to_string(),
                    min: "0".to_string(),
                    max: "100".to_string(),
                    step: "5".to_string(),
                    progress: background_opacity_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<u8>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.background_opacity = value.min(100);
                            });
                        }
                    }
                }

                Checkbox {
                    class: "overlay-checkbox".to_string(),
                    checked: overlay.show_border,
                    label: "Show border".to_string(),
                    onchange: move |checked: bool| {
                        super::super::update_settings(settings, |config| {
                            config.overlay.show_border = checked;
                        });
                    }
                }

                Range {
                    label: "Border radius".to_string(),
                    value_label: format!("{border_radius}px"),
                    value: border_radius.to_string(),
                    min: "0".to_string(),
                    max: "24".to_string(),
                    step: "1".to_string(),
                    progress: border_radius_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<u8>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.border_radius = value.min(24);
                            });
                        }
                    }
                }

                Range {
                    label: "Size scale".to_string(),
                    value_label: format!("{scale}%"),
                    value: scale.to_string(),
                    min: "10".to_string(),
                    max: "400".to_string(),
                    step: "5".to_string(),
                    progress: scale_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<u32>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.scale = value.clamp(10, 400);
                            });
                        }
                    }
                }

                Range {
                    label: "Opacity".to_string(),
                    value_label: format!("{content_opacity}%"),
                    value: content_opacity.to_string(),
                    min: "20".to_string(),
                    max: "100".to_string(),
                    step: "5".to_string(),
                    progress: content_opacity_progress.clone(),
                    oninput: move |evt: FormEvent| {
                        if let Ok(value) = evt.value().parse::<u8>() {
                            super::super::update_settings(settings, |config| {
                                config.overlay.content_opacity = value.clamp(20, 100);
                            });
                        }
                    }
                }
            }

            section {
                class: if overlay.behaviour == "PassThrough" {
                    "sound-card overlay-behaviour pass-through"
                } else {
                    "sound-card overlay-behaviour"
                },
                id: "overlay-behaviour",
                "data-settings-section": "true",
                div { class: "section-head section-head-row", h1 { "Behaviour" } }

                div { class: "overlay-field",
                    label { "Click mode" }
                    SegmentedToggle {
                        value: overlay.behaviour.clone(),
                        options: vec![
                            SegmentedToggleOption::new("PassThrough", "Pass-through").icon("icon-widget"),
                            SegmentedToggleOption::new("Button", "Button").icon("icon-record"),
                        ],
                        onchange: move |value: String| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.behaviour = value;
                            });
                        }
                    }
                }

                div { class: "overlay-action-list",
                    if overlay_action_entries(&overlay).is_empty() {
                        div { class: "hotkey-empty overlay-action-empty",
                            span { class: "solar-icon icon-widget" }
                            p { "No overlay actions configured." }
                        }
                    }
                    for entry in overlay_action_entries(&overlay) {
                        OverlayActionRow {
                            key: "{entry.slot.id()}",
                            slot: entry.slot,
                            binding: entry.binding.clone(),
                            devices: devices.clone(),
                            output_devices: output_devices.clone(),
                            name_display: audio_device_name_display.clone(),
                            onedit: move |slot| {
                                action_editor.set(Some(OverlayActionEditorMode {
                                    slot,
                                    closing: false,
                                }));
                            },
                            onremove: move |slot| {
                                super::super::update_settings(settings, move |config| {
                                    set_overlay_slot_binding(
                                        &mut config.overlay,
                                        slot,
                                        crate::OverlayActionBinding::default(),
                                    );
                                });
                            }
                        }
                    }
                }

                button {
                    class: "secondary add-hotkey-button overlay-add-action-button",
                    onclick: move |_| {
                        action_editor.set(Some(OverlayActionEditorMode {
                            slot: first_empty_overlay_action_slot(&settings().config.overlay),
                            closing: false,
                        }));
                    },
                    span { class: "solar-icon button-icon icon-plus" }
                    "Add action"
                }

                if let Some(mode) = action_editor() {
                    OverlayActionEditorPanel {
                        mode,
                        settings,
                        devices,
                        output_devices,
                        hotkeys,
                        name_display: audio_device_name_display,
                        modal: action_editor
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct OverlayActionEditorMode {
    slot: OverlayActionSlot,
    closing: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OverlayActionSlot {
    SingleClick,
    DoubleClick,
    MiddleClick,
    RightClick,
    WheelUp,
    WheelDown,
}

impl OverlayActionSlot {
    fn id(self) -> &'static str {
        match self {
            Self::SingleClick => "single-click",
            Self::DoubleClick => "double-click",
            Self::MiddleClick => "middle-click",
            Self::RightClick => "right-click",
            Self::WheelUp => "wheel-up",
            Self::WheelDown => "wheel-down",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SingleClick => "Single click",
            Self::DoubleClick => "Double click",
            Self::MiddleClick => "Middle click",
            Self::RightClick => "Right click",
            Self::WheelUp => "Mouse wheel up",
            Self::WheelDown => "Mouse wheel down",
        }
    }
}

#[derive(Clone, PartialEq)]
struct OverlayActionEntry {
    slot: OverlayActionSlot,
    binding: crate::OverlayActionBinding,
}

const OVERLAY_ACTION_SLOTS: &[OverlayActionSlot] = &[
    OverlayActionSlot::SingleClick,
    OverlayActionSlot::DoubleClick,
    OverlayActionSlot::MiddleClick,
    OverlayActionSlot::RightClick,
    OverlayActionSlot::WheelUp,
    OverlayActionSlot::WheelDown,
];

fn overlay_action_entries(overlay: &crate::OverlayConfig) -> Vec<OverlayActionEntry> {
    OVERLAY_ACTION_SLOTS
        .iter()
        .copied()
        .filter_map(|slot| {
            let binding = overlay_slot_binding(overlay, slot);
            binding.action.map(|_| OverlayActionEntry { slot, binding })
        })
        .collect()
}

fn first_empty_overlay_action_slot(overlay: &crate::OverlayConfig) -> OverlayActionSlot {
    OVERLAY_ACTION_SLOTS
        .iter()
        .copied()
        .find(|slot| overlay_slot_binding(overlay, *slot).action.is_none())
        .unwrap_or(OverlayActionSlot::SingleClick)
}

fn overlay_slot_binding(
    overlay: &crate::OverlayConfig,
    slot: OverlayActionSlot,
) -> crate::OverlayActionBinding {
    match slot {
        OverlayActionSlot::SingleClick => overlay.single_click.clone(),
        OverlayActionSlot::DoubleClick => overlay.double_click.clone(),
        OverlayActionSlot::MiddleClick => overlay.middle_click.clone(),
        OverlayActionSlot::RightClick => overlay.right_click.clone(),
        OverlayActionSlot::WheelUp => overlay.wheel_up.clone(),
        OverlayActionSlot::WheelDown => overlay.wheel_down.clone(),
    }
}

fn set_overlay_slot_binding(
    overlay: &mut crate::OverlayConfig,
    slot: OverlayActionSlot,
    binding: crate::OverlayActionBinding,
) {
    match slot {
        OverlayActionSlot::SingleClick => overlay.single_click = binding,
        OverlayActionSlot::DoubleClick => overlay.double_click = binding,
        OverlayActionSlot::MiddleClick => overlay.middle_click = binding,
        OverlayActionSlot::RightClick => overlay.right_click = binding,
        OverlayActionSlot::WheelUp => overlay.wheel_up = binding,
        OverlayActionSlot::WheelDown => overlay.wheel_down = binding,
    }
}

#[component]
fn OverlayActionRow(
    slot: OverlayActionSlot,
    binding: crate::OverlayActionBinding,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    name_display: String,
    onedit: EventHandler<OverlayActionSlot>,
    onremove: EventHandler<OverlayActionSlot>,
) -> Element {
    let action = binding.action.unwrap_or(crate::HotkeyAction::ToggleMute);
    let option = super::hotkeys::action_options()
        .into_iter()
        .find(|option| option.value == action.id());
    let label = option
        .as_ref()
        .map(|option| option.label.clone())
        .unwrap_or_else(|| action.label().to_string());
    let target = action.needs_target().then(|| {
        overlay_target_label(
            action,
            binding.target.as_deref(),
            binding.target_2.as_deref(),
            &devices,
            &output_devices,
            &name_display,
        )
    });

    rsx! {
        div { class: "hotkey-entry overlay-action-entry",
            div { class: "hotkey-main-row overlay-action-main-row",
                div { class: "hotkey-action-cell",
                    h3 { "{label}" }
                    if let Some(target) = target.as_ref() {
                        span { class: "hotkey-target-label", "{target}" }
                    }
                }
                div { class: "overlay-action-gesture-cell",
                    span { "{slot.label()}" }
                }
                div { class: "hotkey-row-actions",
                    button {
                        class: "icon-button",
                        title: "Edit overlay action",
                        onclick: move |_| onedit.call(slot),
                        span { class: "solar-icon icon-pen" }
                    }
                    button {
                        class: "icon-button danger-button",
                        title: "Remove overlay action",
                        onclick: move |_| onremove.call(slot),
                        span { class: "solar-icon icon-trash" }
                    }
                }
            }
        }
    }
}

#[component]
fn OverlayActionEditorPanel(
    mode: OverlayActionEditorMode,
    settings: Signal<super::super::SettingsSnapshot>,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    hotkeys: Vec<crate::HotkeyBinding>,
    name_display: String,
    modal: Signal<Option<OverlayActionEditorMode>>,
) -> Element {
    let mut modal = modal;
    let overlay = settings().config.overlay.clone();
    let binding = overlay_slot_binding(&overlay, mode.slot);
    let gesture_options = OVERLAY_ACTION_SLOTS
        .iter()
        .map(|slot| SelectOption::new(slot.id(), slot.label()))
        .collect::<Vec<_>>();
    let title = if binding.action.is_some() {
        "Edit overlay action"
    } else {
        "Add overlay action"
    };

    rsx! {
        div {
            class: if mode.closing { "hotkey-panel-backdrop exiting" } else { "hotkey-panel-backdrop" },
            onclick: move |_| close_overlay_action_editor(modal, mode),
            aside {
                class: if mode.closing { "hotkey-editor-panel exiting" } else { "hotkey-editor-panel" },
                tabindex: "0",
                onclick: move |evt| evt.stop_propagation(),
                onkeydown: move |evt| {
                    if evt.data().key().to_string() == "Escape" {
                        evt.prevent_default();
                        close_overlay_action_editor(modal, mode);
                    }
                },
            div { class: "hotkey-panel-head",
                div { class: "hotkey-panel-title",
                    h2 { "{title}" }
                    p { "Changes apply immediately." }
                }
                button {
                    class: "icon-button",
                    title: "Close",
                    onclick: move |_| close_overlay_action_editor(modal, mode),
                    span { class: "solar-icon icon-close" }
                }
            }
            div { class: "hotkey-panel-body",
                div { class: "hotkey-panel-body-inner",
                    div { class: "overlay-field",
                        label { "Gesture" }
                        Select {
                            value: mode.slot.id().to_string(),
                            options: gesture_options,
                            onchange: move |value: String| {
                                if let Some(slot) = overlay_action_slot_from_id(&value) {
                                    modal.set(Some(OverlayActionEditorMode {
                                        slot,
                                        closing: false,
                                    }));
                                }
                            }
                        }
                    }

                    OverlayActionPicker {
                        label: "Action",
                        binding: binding.clone(),
                        devices: devices.clone(),
                        output_devices: output_devices.clone(),
                        hotkeys: hotkeys.clone(),
                        name_display: name_display.clone(),
                        onchange: move |binding: crate::OverlayActionBinding| {
                            super::super::update_settings(settings, move |config| {
                                set_overlay_slot_binding(&mut config.overlay, mode.slot, binding);
                            });
                        }
                    }
                }
            }
            }
        }
    }
}

fn overlay_action_slot_from_id(id: &str) -> Option<OverlayActionSlot> {
    OVERLAY_ACTION_SLOTS
        .iter()
        .copied()
        .find(|slot| slot.id() == id)
}

fn close_overlay_action_editor(
    mut modal: Signal<Option<OverlayActionEditorMode>>,
    mode: OverlayActionEditorMode,
) {
    if mode.closing {
        return;
    }
    modal.set(Some(OverlayActionEditorMode {
        closing: true,
        ..mode
    }));
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(180)).await;
        modal.set(None);
    });
}

fn overlay_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    target_2: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> String {
    if action.needs_second_target() {
        let first =
            overlay_single_target_label(action, target, devices, output_devices, name_display);
        let second =
            overlay_single_target_label(action, target_2, devices, output_devices, name_display);
        return format!("{first} / {second}");
    }

    overlay_single_target_label(action, target, devices, output_devices, name_display)
}

fn overlay_single_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    name_display: &str,
) -> String {
    if super::hotkeys::action_uses_volume_target(action) {
        let value = super::hotkeys::volume_target_value(target.unwrap_or_default(), action);
        return format!("{value}%");
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultInputDevice | crate::HotkeyAction::ToggleDefaultInputDevice
    ) {
        return target
            .and_then(|target| devices.iter().find(|device| device.id == target))
            .map(|device| device.display_name(name_display))
            .unwrap_or_else(|| "Choose an input device".to_string());
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultOutputDevice
            | crate::HotkeyAction::ToggleDefaultOutputDevice
    ) {
        return target
            .and_then(|target| output_devices.iter().find(|device| device.id == target))
            .map(|device| device.display_name(name_display))
            .unwrap_or_else(|| "Choose an output device".to_string());
    }

    if matches!(target, Some(crate::HOTKEY_TARGET_ALL_MICROPHONES)) {
        return "All microphones".to_string();
    }

    target
        .filter(|target| !target.is_empty())
        .and_then(|target| devices.iter().find(|device| device.id == target))
        .map(|device| device.display_name(name_display))
        .unwrap_or_else(|| "Default".to_string())
}

#[component]
fn OverlayActionPicker(
    label: &'static str,
    binding: crate::OverlayActionBinding,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    hotkeys: Vec<crate::HotkeyBinding>,
    name_display: String,
    onchange: EventHandler<crate::OverlayActionBinding>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut closing = use_signal(|| false);
    let options = overlay_action_options();
    let current_value = binding
        .action
        .map(|action| action.id().to_string())
        .unwrap_or_default();
    let current = options
        .iter()
        .find(|option| option.value == current_value)
        .cloned()
        .or_else(|| options.first().cloned());
    let close_button_id = use_hook(|| {
        static NEXT_OVERLAY_ACTION_PICKER_ID: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(1);
        format!(
            "overlay-action-close-{}",
            NEXT_OVERLAY_ACTION_PICKER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )
    });
    let close_button_id_for_effect = close_button_id.clone();

    use_effect(use_reactive!(|open| {
        if !open() {
            return;
        }

        let close_button_id = close_button_id_for_effect.clone();
        spawn(async move {
            let script = format!(
                r#"
window.__silenceActionPickerEscHandler ??= (event) => {{
  if (event.key !== 'Escape') {{
    return;
  }}
  const closeButton = [...document.querySelectorAll('.hotkey-action-close-proxy[data-open="true"]')].at(-1);
  if (closeButton) {{
    event.preventDefault();
    closeButton.click();
  }}
}};
document.removeEventListener('keydown', window.__silenceActionPickerEscHandler, true);
document.addEventListener('keydown', window.__silenceActionPickerEscHandler, true);
document.getElementById('{close_button_id}')?.focus({{ preventScroll: true }});
"#
            );
            let _ = dioxus::document::eval(&script).await;
        });
    }));

    let selected_action = binding.action;
    let target = binding.target.clone().unwrap_or_default();
    let target_2 = binding.target_2.clone().unwrap_or_default();

    rsx! {
        div { class: "overlay-field",
            label { "{label}" }
            div { class: "hotkey-action-picker overlay-action-picker",
                button {
                    r#type: "button",
                    class: if open() { "hotkey-action-trigger open" } else { "hotkey-action-trigger" },
                    aria_expanded: if open() { "true" } else { "false" },
                    onclick: move |_| {
                        closing.set(false);
                        open.set(true);
                    },
                    div { class: "ui-select-current",
                        if let Some(option) = current.as_ref() {
                            if let Some(icon_class) = option.icon_class.as_deref() {
                                span { class: "solar-icon ui-select-current-icon {icon_class}" }
                            }
                            div { class: "ui-select-current-copy",
                                div { class: "ui-select-current-text",
                                    span { class: "ui-select-current-label", "{option.label}" }
                                }
                            }
                        }
                    }
                    span { class: "solar-icon ui-select-chevron icon-down" }
                }

                if open() {
                    button {
                        id: "{close_button_id}",
                        r#type: "button",
                        class: "hotkey-action-close-proxy",
                        "data-open": if closing() { "false" } else { "true" },
                        tabindex: "-1",
                        aria_label: "Close action picker",
                        onclick: move |_| close_overlay_action_picker(open, closing)
                    }
                    button {
                        r#type: "button",
                        class: if closing() { "hotkey-action-backdrop exiting" } else { "hotkey-action-backdrop" },
                        tabindex: "-1",
                        aria_label: "Close action picker",
                        onclick: move |_| close_overlay_action_picker(open, closing)
                    }
                    div {
                        class: if closing() { "hotkey-action-sidepanel exiting" } else { "hotkey-action-sidepanel" },
                        onclick: move |evt| evt.stop_propagation(),
                        div { class: "hotkey-action-groups",
                            for group in overlay_action_groups(options.clone()) {
                                section { class: "hotkey-action-group",
                                    h3 { "{group.label}" }
                                    div { class: "hotkey-action-card-grid",
                                        for option in group.options {
                                            {
                                                let binding_for_card = binding.clone();
                                                let devices_for_card = devices.clone();
                                                let output_devices_for_card = output_devices.clone();
                                                let hotkeys_for_card = hotkeys.clone();
                                                rsx! {
                                                    OverlayActionCard {
                                                        option: option.clone(),
                                                        selected: option.value == current_value,
                                                        onselect: move |value: String| {
                                                            let action = if value.is_empty() {
                                                                None
                                                            } else {
                                                                Some(crate::HotkeyAction::from_id(&value))
                                                            };
                                                            let next = overlay_binding_for_action(
                                                                binding_for_card.clone(),
                                                                action,
                                                                &devices_for_card,
                                                                &output_devices_for_card,
                                                                &hotkeys_for_card,
                                                            );
                                                            onchange.call(next);
                                                            close_overlay_action_picker(open, closing);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if let Some(action) = selected_action {
                if action.needs_target() && super::hotkeys::action_uses_volume_target(action) {
                    {
                        let binding_for_target = binding.clone();
                        rsx! {
                    OverlayVolumeTargetInput {
                        action,
                        value: target.clone(),
                        onchange: move |value: String| {
                            let mut next = binding_for_target.clone();
                            next.target = Some(value);
                            onchange.call(next);
                        }
                    }
                        }
                    }
                } else if action.needs_target() {
                    {
                        let binding_for_target = binding.clone();
                        let devices_for_target = devices.clone();
                        let output_devices_for_target = output_devices.clone();
                        rsx! {
                    OverlayTargetSelect {
                        action,
                        label: "Target 1",
                        value: target.clone(),
                        devices: devices.clone(),
                        output_devices: output_devices.clone(),
                        name_display: name_display.clone(),
                        onchange: move |value: String| {
                            let mut next = binding_for_target.clone();
                            next.target = Some(value.clone());
                            if action.needs_second_target() {
                                let second = next.target_2.clone().unwrap_or_default();
                                if !super::hotkeys::target_is_valid_for_action(
                                    action,
                                    &second,
                                    &devices_for_target,
                                    &output_devices_for_target,
                                ) || second == value {
                                    next.target_2 = Some(super::hotkeys::default_second_target_for_action(
                                        action,
                                        &value,
                                        &devices_for_target,
                                        &output_devices_for_target,
                                    ));
                                }
                            }
                            onchange.call(next);
                        }
                    }
                        }
                    }
                    if action.needs_second_target() {
                        {
                            let binding_for_target_2 = binding.clone();
                            rsx! {
                        OverlayTargetSelect {
                            action,
                            label: "Target 2",
                            value: target_2.clone(),
                            devices: devices.clone(),
                            output_devices: output_devices.clone(),
                            name_display: name_display.clone(),
                            onchange: move |value: String| {
                                let mut next = binding_for_target_2.clone();
                                next.target_2 = Some(value);
                                onchange.call(next);
                            }
                        }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn close_overlay_action_picker(mut open: Signal<bool>, mut closing: Signal<bool>) {
    if closing() {
        return;
    }
    closing.set(true);
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(180)).await;
        open.set(false);
        closing.set(false);
    });
}

fn overlay_binding_for_action(
    mut binding: crate::OverlayActionBinding,
    action: Option<crate::HotkeyAction>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
    hotkeys: &[crate::HotkeyBinding],
) -> crate::OverlayActionBinding {
    let previous_action = binding.action;
    binding.action = action;
    let Some(action) = action else {
        binding.target = None;
        binding.target_2 = None;
        return binding;
    };

    let mut target = binding.target.clone().unwrap_or_default();
    if super::hotkeys::action_uses_volume_target(action) && previous_action != Some(action) {
        target =
            super::hotkeys::default_target_for_action(action, devices, output_devices, hotkeys);
    } else if !action.needs_target()
        || !super::hotkeys::target_is_valid_for_action(action, &target, devices, output_devices)
    {
        target =
            super::hotkeys::default_target_for_action(action, devices, output_devices, hotkeys);
    }
    binding.target = action.needs_target().then_some(target.clone());

    let mut target_2 = binding.target_2.clone().unwrap_or_default();
    if !action.needs_second_target()
        || !super::hotkeys::target_is_valid_for_action(action, &target_2, devices, output_devices)
    {
        target_2 = super::hotkeys::default_second_target_for_action(
            action,
            &target,
            devices,
            output_devices,
        );
    }
    binding.target_2 = action.needs_second_target().then_some(target_2);
    binding
}

#[component]
fn OverlayTargetSelect(
    action: crate::HotkeyAction,
    label: &'static str,
    value: String,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    name_display: String,
    onchange: EventHandler<String>,
) -> Element {
    let options = super::hotkeys::target_options(action, devices, output_devices, &name_display);

    rsx! {
        div { class: "overlay-field overlay-action-target-field",
            label { "{label}" }
            Select {
                value,
                options,
                onchange: move |value: String| onchange.call(value)
            }
        }
    }
}

#[component]
fn OverlayVolumeTargetInput(
    action: crate::HotkeyAction,
    value: String,
    onchange: EventHandler<String>,
) -> Element {
    let label = match action {
        crate::HotkeyAction::SetVolume => "Volume",
        crate::HotkeyAction::IncreaseVolume | crate::HotkeyAction::DecreaseVolume => "Amount",
        _ => "Target",
    };
    let value = super::hotkeys::volume_target_value(&value, action).to_string();
    let value_label = format!("{value}%");

    rsx! {
        Range {
            label: label.to_string(),
            value_label,
            value: value.clone(),
            min: "0".to_string(),
            max: "100".to_string(),
            step: "1".to_string(),
            progress: format!("{value}%"),
            label_icon: Some("icon-volume".to_string()),
            class: "overlay-action-target-field".to_string(),
            oninput: move |evt: FormEvent| {
                if let Ok(value) = evt.value().parse::<u8>() {
                    onchange.call(value.min(100).to_string());
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
struct OverlayActionGroup {
    label: String,
    options: Vec<SelectOption>,
}

fn overlay_action_groups(options: Vec<SelectOption>) -> Vec<OverlayActionGroup> {
    let mut groups = Vec::<OverlayActionGroup>::new();
    for option in options {
        let label = option
            .group_label
            .clone()
            .unwrap_or_else(|| "Other".to_string());
        if let Some(group) = groups.iter_mut().find(|group| group.label == label) {
            group.options.push(option);
        } else {
            groups.push(OverlayActionGroup {
                label,
                options: vec![option],
            });
        }
    }
    groups
}

fn overlay_action_options() -> Vec<SelectOption> {
    std::iter::once(
        SelectOption::new("", "Empty")
            .group("Other")
            .icon("icon-close"),
    )
    .chain(super::hotkeys::action_options())
    .collect()
}

#[component]
fn OverlayActionCard(
    option: SelectOption,
    selected: bool,
    onselect: EventHandler<String>,
) -> Element {
    rsx! {
        button {
            r#type: "button",
            class: if selected { "hotkey-action-card selected" } else { "hotkey-action-card" },
            onclick: move |_| onselect.call(option.value.clone()),
            if let Some(icon_class) = option.icon_class.as_deref() {
                span { class: "solar-icon hotkey-action-card-icon {icon_class}" }
            }
            span { class: "hotkey-action-card-label", "{option.label}" }
        }
    }
}
