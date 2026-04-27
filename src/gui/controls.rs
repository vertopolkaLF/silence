use dioxus::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_SELECT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub detail: Option<String>,
    pub icon_class: Option<String>,
    pub end_icon_class: Option<String>,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            detail: None,
            icon_class: None,
            end_icon_class: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn icon(mut self, icon_class: impl Into<String>) -> Self {
        self.icon_class = Some(icon_class.into());
        self
    }

    pub fn end_icon(mut self, icon_class: impl Into<String>) -> Self {
        self.end_icon_class = Some(icon_class.into());
        self
    }
}

#[component]
pub fn Checkbox(
    checked: bool,
    label: String,
    onchange: EventHandler<bool>,
    #[props(default)] class: String,
) -> Element {
    rsx! {
        label { class: merged_class("ui-checkbox", &class),
            input {
                r#type: "checkbox",
                checked,
                onchange: move |evt| onchange.call(evt.checked())
            }
            span { class: "ui-checkbox-box",
                span { class: "ui-checkbox-mark" }
            }
            span { class: "ui-checkbox-label", "{label}" }
        }
    }
}

#[component]
pub fn Range(
    value: String,
    min: String,
    max: String,
    step: String,
    progress: String,
    oninput: EventHandler<FormEvent>,
    #[props(default)] class: String,
) -> Element {
    rsx! {
        input {
            class: merged_class("ui-range", &class),
            r#type: "range",
            min: "{min}",
            max: "{max}",
            step: "{step}",
            value: "{value}",
            style: "--range-progress: {progress};",
            oninput: move |evt| oninput.call(evt)
        }
    }
}

#[component]
pub fn Select(
    value: String,
    options: Vec<SelectOption>,
    onchange: EventHandler<String>,
    #[props(default)] on_option_action: Option<EventHandler<String>>,
    #[props(default)] class: String,
) -> Element {
    let mut open = use_signal(|| false);
    let mut menu_style = use_signal(String::new);
    let select_id = use_hook(|| {
        format!(
            "ui-select-{}",
            NEXT_SELECT_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let current = options
        .iter()
        .find(|option| option.value == value)
        .cloned()
        .or_else(|| options.first().cloned());
    let root_class = if open() {
        merged_class("ui-select open", &class)
    } else {
        merged_class("ui-select", &class)
    };
    let menu_class = if menu_style().is_empty() {
        "ui-select-menu"
    } else {
        "ui-select-menu ready"
    };
    let rendered_options = options.into_iter().map(|option| {
        let is_selected = option.value == value;
        let next_value = option.value.clone();
        let item_class = if is_selected {
            "ui-select-item selected"
        } else {
            "ui-select-item"
        };
        let action_value = option.value.clone();

        rsx! {
            div {
                key: "select-option-{option.value}",
                class: "{item_class}",
                button {
                    r#type: "button",
                    class: "ui-select-item-button",
                    onclick: move |_| {
                        open.set(false);
                        onchange.call(next_value.clone());
                    },
                    div { class: "ui-select-item-main",
                        if let Some(icon_class) = option.icon_class.as_deref() {
                            span { class: "solar-icon ui-select-item-icon {icon_class}" }
                        }
                        div { class: "ui-select-item-copy",
                            span { class: "ui-select-item-label", "{option.label}" }
                            if let Some(detail) = option.detail.as_deref() {
                                span { class: "ui-select-item-detail", "{detail}" }
                            }
                        }
                    }
                }
                if let Some(icon_class) = option.end_icon_class.as_deref() {
                    div { class: "ui-select-item-end",
                        if let Some(on_option_action) = on_option_action.clone() {
                            button {
                                r#type: "button",
                                class: "ui-select-item-action",
                                title: "Preview",
                                onclick: move |evt| {
                                    evt.stop_propagation();
                                    on_option_action.call(action_value.clone());
                                },
                                span { class: "solar-icon ui-select-item-end-icon {icon_class}" }
                            }
                        } else {
                            span { class: "solar-icon ui-select-item-end-icon {icon_class}" }
                        }
                    }
                }
            }
        }
    });

    let position_select_id = select_id.clone();
    use_effect(move || {
        let select_id = position_select_id.clone();
        if !open() {
            menu_style.set(String::new());
            return;
        }

        spawn(async move {
            let script = format!(
                r#"
const root = document.querySelector('[data-ui-select-id="{select_id}"]');
const trigger = root?.querySelector('.ui-select-trigger');
const list = root?.querySelector('.ui-select-list');
if (!trigger || !list) {{
  return '';
}}

const rect = trigger.getBoundingClientRect();
const viewportWidth = window.innerWidth;
const viewportHeight = window.innerHeight;
const gutter = 12;
const gap = 8;
const width = Math.min(rect.width, viewportWidth - gutter * 2);
const left = Math.min(
  Math.max(gutter, rect.left),
  viewportWidth - gutter - width
);
const desiredHeight = Math.min(list.scrollHeight + 12, 320);
const spaceAbove = Math.max(0, rect.top - gutter);
const spaceBelow = Math.max(0, viewportHeight - rect.bottom - gutter);
const minComfortHeight = Math.min(desiredHeight, 140);

let placeBelow =
  spaceBelow >= desiredHeight ||
  (spaceBelow >= minComfortHeight && spaceBelow >= spaceAbove);

let height = desiredHeight;
let top = 0;
let shift = 6;
let origin = 'top center';

if (placeBelow) {{
  height = Math.min(desiredHeight, Math.max(spaceBelow, 96));
  top = rect.bottom + gap;
  shift = -6;
  if (top + height > viewportHeight - gutter) {{
    height = Math.max(96, viewportHeight - top - gutter);
  }}
  if (height < minComfortHeight && spaceAbove > spaceBelow) {{
    placeBelow = false;
  }}
}}

if (!placeBelow) {{
  height = Math.min(desiredHeight, Math.max(spaceAbove, 96));
  top = rect.top - gap - height;
  shift = 6;
  origin = 'bottom center';
  if (top < gutter) {{
    top = Math.max(gutter, rect.top - height + 14);
    height = Math.max(96, rect.bottom - top - 14);
  }}
}}

return `left:${{left}}px;top:${{top}}px;width:${{width}}px;--ui-select-max-height:${{height}}px;--ui-select-shift:${{shift}}px;--ui-select-origin:${{origin}};`;
"#
            );

            if let Ok(result) = dioxus::document::eval(&script).await {
                if let Some(style) = result.as_str() {
                    menu_style.set(style.to_string());
                }
            }
        });
    });

    rsx! {
        div { class: "{root_class}", "data-ui-select-id": "{select_id}",
            if open() {
                button {
                    r#type: "button",
                    class: "ui-select-dismiss",
                    tabindex: "-1",
                    aria_hidden: "true",
                    onclick: move |_| open.set(false)
                }
            }

            button {
                r#type: "button",
                class: "ui-select-trigger",
                aria_expanded: if open() { "true" } else { "false" },
                onclick: move |_| open.set(!open()),
                div { class: "ui-select-current",
                    if let Some(icon_class) = current.as_ref().and_then(|option| option.icon_class.as_deref()) {
                        span { class: "solar-icon ui-select-current-icon {icon_class}" }
                    }
                    div { class: "ui-select-current-copy",
                        if let Some(option) = current.as_ref() {
                            span {
                                key: "current-label-{option.value}",
                                class: "ui-select-current-label",
                                "{option.label}"
                            }
                            if let Some(detail) = option.detail.as_deref() {
                                span {
                                    key: "current-detail-{option.value}",
                                    class: "ui-select-current-detail",
                                    "{detail}"
                                }
                            }
                        }
                    }
                }
                span { class: "solar-icon ui-select-chevron icon-down" }
            }

            div { class: "{menu_class}", style: "{menu_style}",
                div { class: "ui-select-list",
                    for item in rendered_options {
                        {item}
                    }
                }
            }
        }
    }
}

fn merged_class(base: &str, extra: &str) -> String {
    if extra.trim().is_empty() {
        base.to_string()
    } else {
        format!("{base} {extra}")
    }
}
