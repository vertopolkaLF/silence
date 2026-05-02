use dioxus::prelude::*;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

static NEXT_SELECT_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub group_label: Option<String>,
    pub detail: Option<String>,
    pub icon_class: Option<String>,
    pub end_icon_class: Option<String>,
}

impl SelectOption {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
            group_label: None,
            detail: None,
            icon_class: None,
            end_icon_class: None,
        }
    }

    pub fn group(mut self, group_label: impl Into<String>) -> Self {
        self.group_label = Some(group_label.into());
        self
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
    #[props(default = false)] disabled: bool,
    #[props(default)] class: String,
) -> Element {
    let root_class = match (checked, disabled) {
        (true, true) => merged_class("ui-checkbox checked disabled", &class),
        (true, false) => merged_class("ui-checkbox checked", &class),
        (false, true) => merged_class("ui-checkbox disabled", &class),
        (false, false) => merged_class("ui-checkbox", &class),
    };

    rsx! {
        label { class: root_class,
            input {
                r#type: "checkbox",
                checked,
                disabled,
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
    label: String,
    value_label: String,
    value: String,
    min: String,
    max: String,
    step: String,
    progress: String,
    oninput: EventHandler<FormEvent>,
    #[props(default)] class: String,
    #[props(default)] start_icon: Option<String>,
    #[props(default)] end_icon: Option<String>,
) -> Element {
    let icon_class = match (start_icon.is_some(), end_icon.is_some()) {
        (true, true) => "ui-range-control has-start-icon has-end-icon",
        (true, false) => "ui-range-control has-start-icon",
        (false, true) => "ui-range-control has-end-icon",
        (false, false) => "ui-range-control",
    };

    rsx! {
        div { class: merged_class(icon_class, &class),
            if let Some(icon) = start_icon {
                span { class: "solar-icon ui-range-icon {icon}" }
            }
            label { class: "ui-range-shell",
                span {
                    class: "ui-range-fill",
                    style: "--range-progress: {progress};"
                }
                input {
                    class: "ui-range",
                    r#type: "range",
                    min: "{min}",
                    max: "{max}",
                    step: "{step}",
                    value: "{value}",
                    oninput: move |evt| oninput.call(evt)
                }
                span {
                    class: "ui-range-dragger",
                    style: "--range-progress: {progress};"
                }
                span { class: "ui-range-copy",
                    span { class: "ui-range-label", "{label}" }
                    span { class: "ui-range-value", "{value_label}" }
                }
            }
            if let Some(icon) = end_icon {
                span { class: "solar-icon ui-range-icon {icon}" }
            }
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
    let mut animate_value = use_signal(|| false);
    let mut exiting_value = use_signal(|| None::<SelectOption>);
    let mut open_select = use_context::<Signal<Option<String>>>();
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

    let sync_select_id = select_id.clone();
    use_effect(move || {
        if open() && open_select().as_deref() != Some(sync_select_id.as_str()) {
            open.set(false);
        }
    });

    let mut rendered_options = Vec::new();
    let mut current_group = None::<String>;
    for option in options {
        if option.group_label != current_group {
            current_group = option.group_label.clone();
            if let Some(group_label) = current_group.clone() {
                rendered_options.push(rsx! {
                    div {
                        key: "select-group-{group_label}",
                        class: "ui-select-group",
                        "{group_label}"
                    }
                });
            }
        }

        let is_selected = option.value == value;
        let next_value = option.value.clone();
        let item_class = if is_selected {
            "ui-select-item selected"
        } else {
            "ui-select-item"
        };
        let action_value = option.value.clone();
        let close_select_id = select_id.clone();
        let should_animate = next_value != value;
        let previous_value = current.clone();

        rendered_options.push(rsx! {
            div {
                key: "select-option-{option.value}",
                class: "{item_class}",
                button {
                    r#type: "button",
                    class: "ui-select-item-button",
                    onclick: move |_| {
                        open.set(false);
                        if open_select().as_deref() == Some(close_select_id.as_str()) {
                            open_select.set(None);
                        }
                        if should_animate {
                            exiting_value.set(previous_value.clone());
                        }
                        animate_value.set(should_animate);
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
        });
    }

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
const desiredHeight = Math.min(list.scrollHeight, 320);
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

    let shadow_select_id = select_id.clone();
    use_effect(move || {
        let select_id = shadow_select_id.clone();
        if !open() || menu_style().is_empty() {
            return;
        }

        spawn(async move {
            let script = format!(
                r#"
const root = document.querySelector('[data-ui-select-id="{select_id}"]');
const menu = root?.querySelector('.ui-select-menu');
const list = root?.querySelector('.ui-select-list');
if (!menu || !list) {{
  return;
}}

const scrollSelectedIntoView = () => {{
  const selected = list.querySelector('.ui-select-item.selected');
  if (!selected) {{
    return;
  }}

  const listHeight = list.clientHeight;
  const selectedTop = selected.offsetTop;
  const selectedBottom = selectedTop + selected.offsetHeight;
  const visibleTop = list.scrollTop;
  const visibleBottom = visibleTop + listHeight;

  if (selectedTop >= visibleTop && selectedBottom <= visibleBottom) {{
    return;
  }}

  const centeredTop = selectedTop - (listHeight - selected.offsetHeight) / 2;
  const maxScroll = Math.max(0, list.scrollHeight - listHeight);
  list.scrollTop = Math.min(Math.max(0, centeredTop), maxScroll);
}};

const updateShadows = () => {{
  const maxScroll = Math.max(0, list.scrollHeight - list.clientHeight);
  const canScroll = maxScroll > 1;
  const showTop = canScroll && list.scrollTop > 1;
  const showBottom = canScroll && list.scrollTop < maxScroll - 1;

  menu.setAttribute('data-scroll-top', showTop ? 'true' : 'false');
  menu.setAttribute('data-scroll-bottom', showBottom ? 'true' : 'false');
}};

if (!list.__uiSelectShadowHandler) {{
  const handler = () => updateShadows();
  list.addEventListener('scroll', handler, {{ passive: true }});
  list.__uiSelectShadowHandler = handler;
}}

if (!list.__uiSelectShadowResizeObserver) {{
  const resizeObserver = new ResizeObserver(() => updateShadows());
  resizeObserver.observe(list);
  list.__uiSelectShadowResizeObserver = resizeObserver;
}}

scrollSelectedIntoView();
updateShadows();
requestAnimationFrame(() => {{
  scrollSelectedIntoView();
  updateShadows();
}});
"#
            );

            let _ = dioxus::document::eval(&script).await;
        });
    });

    use_effect(move || {
        if !animate_value() {
            return;
        }

        spawn(async move {
            tokio::time::sleep(Duration::from_millis(310)).await;
            animate_value.set(false);
            exiting_value.set(None);
        });
    });

    let trigger_select_id = select_id.clone();
    let dismiss_select_id = select_id.clone();

    rsx! {
        div { class: "{root_class}", "data-ui-select-id": "{select_id}",
            if open() {
                button {
                    r#type: "button",
                    class: "ui-select-dismiss",
                    tabindex: "-1",
                    aria_hidden: "true",
                    onclick: move |_| {
                        open.set(false);
                        if open_select().as_deref() == Some(dismiss_select_id.as_str()) {
                            open_select.set(None);
                        }
                    }
                }
            }

            button {
                r#type: "button",
                class: "ui-select-trigger",
                aria_expanded: if open() { "true" } else { "false" },
                onclick: move |_| {
                    if open() {
                        open.set(false);
                        open_select.set(None);
                    } else {
                        open_select.set(Some(trigger_select_id.clone()));
                        open.set(true);
                    }
                },
                div { class: "ui-select-current",
                    if current.as_ref().and_then(|option| option.icon_class.as_deref()).is_some()
                        || exiting_value().as_ref().and_then(|option| option.icon_class.as_deref()).is_some()
                    {
                        span { class: "ui-select-current-icon-stack",
                            if animate_value() {
                                if let Some(option) = exiting_value().as_ref() {
                                    if let Some(icon_class) = option.icon_class.as_deref() {
                                        span {
                                            key: "current-icon-exit-{option.value}",
                                            class: "solar-icon ui-select-current-icon ui-select-current-icon-exit {icon_class}"
                                        }
                                    }
                                }
                            }
                            if let Some(option) = current.as_ref() {
                                if let Some(icon_class) = option.icon_class.as_deref() {
                                    span {
                                        key: "current-icon-enter-{option.value}",
                                        class: if animate_value() {
                                            "solar-icon ui-select-current-icon ui-select-current-icon-enter {icon_class}"
                                        } else {
                                            "solar-icon ui-select-current-icon {icon_class}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    div { class: "ui-select-current-copy",
                        if animate_value() {
                            if let Some(option) = exiting_value().as_ref() {
                                div {
                                    key: "current-exit-{option.value}",
                                    class: "ui-select-current-text ui-select-current-text-exit",
                                    span { class: "ui-select-current-label", "{option.label}" }
                                    if let Some(detail) = option.detail.as_deref() {
                                        span { class: "ui-select-current-detail", "{detail}" }
                                    }
                                }
                            }
                        }
                        if let Some(option) = current.as_ref() {
                            div {
                                key: "current-enter-{option.value}",
                                class: if animate_value() { "ui-select-current-text ui-select-current-text-enter" } else { "ui-select-current-text" },
                                span {
                                    class: "ui-select-current-label",
                                    "{option.label}"
                                }
                                if let Some(detail) = option.detail.as_deref() {
                                    span {
                                        class: "ui-select-current-detail",
                                        "{detail}"
                                    }
                                }
                            }
                        }
                    }
                }
                span { class: "solar-icon ui-select-chevron icon-down" }
            }

            div { class: "{menu_class}", style: "{menu_style}",
                div { class: "ui-select-scroll-shadow top", aria_hidden: "true" }
                div { class: "ui-select-scroll-shadow bottom", aria_hidden: "true" }
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
