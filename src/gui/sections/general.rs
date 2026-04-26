use dioxus::prelude::*;

pub fn render(
    mut shortcut: Signal<crate::Shortcut>,
    mut recording: Signal<bool>,
    mut saved: Signal<bool>,
) -> Element {
    let current = shortcut().display();
    let mic_muted = crate::current_mute_state().unwrap_or(false);
    let mic_status = if mic_muted {
        "Microphone is muted"
    } else {
        "Microphone is unmuted"
    };

    rsx! {
        section {
            class: "status-row",
            div {
                class: if mic_muted { "mic-dot muted" } else { "mic-dot" },
                span { class: "solar-icon icon-mic" }
            }
            div {
                class: "status-copy",
                h1 { "{mic_status}" }
                p { "Default input device" }
            }
        }

        section {
            class: "field-group",
            label { "Microphone" }
            div {
                class: "select-like",
                span { "All Microphones" }
                span { class: "solar-icon select-icon icon-down" }
            }
        }

        section {
            class: "hotkeys",
            div { class: "section-head", h2 { "Hotkeys" } }
            div {
                class: "hotkey-title-row",
                h3 { "Toggle microphone" }
                button { class: "secondary", "Add hotkey" }
            }
            div {
                class: "hotkey-row",
                input {
                    class: if recording() { "recorder recording" } else { "recorder" },
                    readonly: true,
                    value: "{current}",
                    placeholder: "Click and press keys",
                    onfocus: move |_| recording.set(true),
                    onkeydown: move |evt| {
                        evt.prevent_default();
                        if let Some(next) = shortcut_from_keyboard_data(&evt.data()) {
                            shortcut.set(next);
                            recording.set(false);
                            saved.set(false);
                        }
                    }
                }
                button {
                    class: "icon-button",
                    onclick: move |_| {
                        shortcut.set(crate::Shortcut::default());
                        saved.set(false);
                    },
                    title: "Reset shortcut",
                    span { class: "solar-icon icon-reset" }
                }
                button {
                    class: "secondary",
                    onclick: move |_| recording.set(true),
                    span { class: "solar-icon button-icon icon-record" }
                    "Record"
                }
            }
            label {
                class: "check-row",
                input { r#type: "checkbox" }
                span { "Ignore modifiers (Shift, Ctrl, Alt, Win)" }
            }

            div {
                class: "hotkey-title-row lower",
                h3 { "Mute microphone" }
                button { class: "secondary", "Add hotkey" }
            }
        }

        footer {
            button {
                class: "save",
                onclick: move |_| {
                    if crate::save_config(&crate::Config { shortcut: shortcut() }).is_ok() {
                        saved.set(true);
                    }
                },
                span { class: "solar-icon button-icon icon-shield" }
                "Save"
            }
            span {
                class: if saved() { "status visible" } else { "status" },
                "Saved"
            }
        }
    }
}

fn shortcut_from_keyboard_data(data: &dioxus::events::KeyboardData) -> Option<crate::Shortcut> {
    let code = format!("{:?}", data.code());
    let vk = vk_from_code(&code)?;
    if crate::is_modifier(vk) {
        return None;
    }
    let modifiers = data.modifiers();
    Some(crate::Shortcut {
        ctrl: modifiers.ctrl(),
        alt: modifiers.alt(),
        shift: modifiers.shift(),
        win: modifiers.meta(),
        vk,
    })
}

fn vk_from_code(code: &str) -> Option<u32> {
    if let Some(letter) = code.strip_prefix("Key") {
        return letter
            .as_bytes()
            .first()
            .map(|byte| byte.to_ascii_uppercase() as u32);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.as_bytes().first().map(|byte| *byte as u32);
    }
    if let Some(digit) = code.strip_prefix("Numpad") {
        return digit
            .as_bytes()
            .first()
            .filter(|byte| byte.is_ascii_digit())
            .map(|byte| crate::VK_NUMPAD0 + (*byte - b'0') as u32);
    }
    if let Some(number) = code.strip_prefix('F') {
        let n = number.parse::<u32>().ok()?;
        if (1..=24).contains(&n) {
            return Some(crate::VK_F1 + n - 1);
        }
    }
    match code {
        "Space" => Some(0x20),
        "Backspace" => Some(0x08),
        "Tab" => Some(0x09),
        "Enter" => Some(0x0D),
        "Escape" => Some(0x1B),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "End" => Some(0x23),
        "Home" => Some(0x24),
        "ArrowLeft" => Some(0x25),
        "ArrowUp" => Some(0x26),
        "ArrowRight" => Some(0x27),
        "ArrowDown" => Some(0x28),
        _ => None,
    }
}
