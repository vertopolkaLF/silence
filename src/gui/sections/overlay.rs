use std::time::Duration;

use dioxus::prelude::*;

use crate::gui::controls::{
    Checkbox, Range, SegmentedToggle, SegmentedToggleOption, Select, SelectOption,
};

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut positioning = use_signal(|| false);
    let mut icons_expanded = use_signal(|| false);
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

            section { class: "sound-card overlay-behaviour",
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

                div { class: "overlay-action-grid",
                    OverlayActionPicker {
                        label: "Single click",
                        value: overlay.single_click_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.single_click_action = action;
                            });
                        }
                    }
                    OverlayActionPicker {
                        label: "Double click",
                        value: overlay.double_click_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.double_click_action = action;
                            });
                        }
                    }
                    OverlayActionPicker {
                        label: "Middle click",
                        value: overlay.middle_click_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.middle_click_action = action;
                            });
                        }
                    }
                    OverlayActionPicker {
                        label: "Right click",
                        value: overlay.right_click_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.right_click_action = action;
                            });
                        }
                    }
                    OverlayActionPicker {
                        label: "Mouse wheel up",
                        value: overlay.wheel_up_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.wheel_up_action = action;
                            });
                        }
                    }
                    OverlayActionPicker {
                        label: "Mouse wheel down",
                        value: overlay.wheel_down_action,
                        onchange: move |action: Option<crate::HotkeyAction>| {
                            super::super::update_settings(settings, |config| {
                                config.overlay.wheel_down_action = action;
                            });
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn OverlayActionPicker(
    label: &'static str,
    value: Option<crate::HotkeyAction>,
    onchange: EventHandler<Option<crate::HotkeyAction>>,
) -> Element {
    let mut open = use_signal(|| false);
    let mut closing = use_signal(|| false);
    let options = overlay_action_options();
    let current_value = value
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
                                            OverlayActionCard {
                                                option: option.clone(),
                                                selected: option.value == current_value,
                                                onselect: move |value: String| {
                                                    let action = if value.is_empty() {
                                                        None
                                                    } else {
                                                        Some(crate::HotkeyAction::from_id(&value))
                                                    };
                                                    onchange.call(action);
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
