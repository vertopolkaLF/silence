use std::{
    io::ErrorKind,
    time::{Duration, Instant},
};

use dioxus::prelude::*;

use super::SettingsSnapshot;

const WELCOME_CSS: Asset = asset!("/assets/styles/welcome.css", AssetOptions::css());

#[component]
pub(super) fn WelcomeSequence(mut settings: Signal<SettingsSnapshot>) -> Element {
    let mut step = use_signal(|| 0_usize);
    let mut returning_user = use_signal(|| false);
    let mut import_error = use_signal(String::new);
    let mut captured_shortcut = use_signal(|| {
        settings()
            .config
            .hotkeys
            .iter()
            .find(|binding| {
                binding.action == crate::HotkeyAction::ToggleMute && binding.gamepad.is_none()
            })
            .map(|binding| binding.shortcut.clone())
            .unwrap_or_default()
    });
    let mut modifier_hold_started = use_signal(|| None::<Instant>);
    let mut modifier_hold_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut capture_progress = use_signal(|| 0.0_f64);
    let mut capture_completed = use_signal(|| false);

    use_effect(move || {
        crate::set_settings_hotkey_recording(step() == 1);
    });

    use_future(move || async move {
        loop {
            if step() == 1 {
                if let Some(shortcut) = crate::take_settings_mouse_pressed_shortcut() {
                    captured_shortcut.set(shortcut.clone());
                    let _ = crate::set_welcome_toggle_shortcut(shortcut);
                    modifier_hold_started.set(None);
                    modifier_hold_shortcut.set(None);
                    complete_capture_progress(capture_progress, capture_completed);
                } else if let Some(started) = modifier_hold_started() {
                    let current = welcome_current_modifier_shortcut();
                    if current != modifier_hold_shortcut() {
                        modifier_hold_shortcut.set(current.clone());
                        modifier_hold_started.set(current.as_ref().map(|_| Instant::now()));
                        capture_progress.set(0.0);
                        capture_completed.set(false);
                    } else if let Some(shortcut) = current {
                        let progress = (started.elapsed().as_secs_f64()).clamp(0.0, 1.0);
                        capture_completed.set(false);
                        capture_progress.set(progress);
                        if progress >= 1.0 {
                            captured_shortcut.set(shortcut.clone());
                            let _ = crate::set_welcome_toggle_shortcut(shortcut);
                            modifier_hold_started.set(None);
                            modifier_hold_shortcut.set(None);
                            complete_capture_progress(capture_progress, capture_completed);
                        }
                    } else if !capture_completed() {
                        modifier_hold_started.set(None);
                        modifier_hold_shortcut.set(None);
                        capture_progress.set(0.0);
                    }
                }
            } else if !capture_completed() {
                modifier_hold_started.set(None);
                modifier_hold_shortcut.set(None);
                capture_progress.set(0.0);
            }
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    });

    let finish = move |_| {
        if crate::complete_welcome().is_ok() {
            let next = settings.peek().clone().refresh(false);
            settings.set(next);
        }
    };
    let welcome_progress = format!("{:.2}%", capture_progress() * 100.0);

    rsx! {
        link { rel: "stylesheet", href: "{WELCOME_CSS}" }
        main {
            class: if capture_completed() { "welcome-shell capture-completed" } else { "welcome-shell" },
            tabindex: "0",
            style: "--welcome-progress: {welcome_progress};",
            onkeydown: move |evt| {
                if step() != 1 {
                    return;
                }
                evt.prevent_default();
                if let Some(shortcut) = welcome_shortcut_from_keyboard_data(&evt.data()) {
                    if shortcut.vk == 0 {
                        if modifier_hold_shortcut() != Some(shortcut.clone()) {
                            modifier_hold_started.set(Some(Instant::now()));
                            capture_progress.set(0.0);
                            capture_completed.set(false);
                        }
                        modifier_hold_shortcut.set(Some(shortcut));
                    } else {
                        captured_shortcut.set(shortcut.clone());
                        let _ = crate::set_welcome_toggle_shortcut(shortcut);
                        modifier_hold_started.set(None);
                        modifier_hold_shortcut.set(None);
                        complete_capture_progress(capture_progress, capture_completed);
                    }
                }
            },
            onkeyup: move |_| {
                if step() == 1 && welcome_current_modifier_shortcut().is_none() && !capture_completed() {
                    modifier_hold_started.set(None);
                    modifier_hold_shortcut.set(None);
                    capture_progress.set(0.0);
                }
            },
            div {
                class: "welcome-logo",
                span { "silence" }
                strong { "!" }
            }
            div { class: "welcome-stage",
                if step() == 0 {
                    section { class: "welcome-screen",
                        div { class: "welcome-kicker", "silence! v2" }
                        h1 { "Welcome" }
                        div { class: "welcome-feature-list",
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-keyboard-bold" }
                                h3 { "Global hotkeys" }
                                p { "Mute from games, calls, or whatever fullscreen nonsense is eating your keyboard." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-widget-bold" }
                                h3 { "Live overlay" }
                                p { "Your mic state updates everywhere immediately, because stale UI is bullshit." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-volume-loud-bold" }
                                h3 { "Sound feedback" }
                                p { "Hear the mute change before you start talking into the void like a professional idiot." }
                            }
                        }
                        if !import_error().is_empty() {
                            p { class: "welcome-error", "{import_error()}" }
                        }
                        div { class: "welcome-actions",
                            button {
                                class: "secondary",
                                onclick: move |_| {
                                    match crate::import_v1_settings() {
                                        Ok(()) => {
                                            import_error.set(String::new());
                                            returning_user.set(true);
                                            let next = settings.peek().clone().refresh(true);
                                            settings.set(next);
                                            step.set(2);
                                        }
                                        Err(err) => {
                                            if welcome_error_is_not_found(&err) {
                                                import_error.set("not found".to_string());
                                            } else {
                                                import_error.set(err.to_string());
                                            }
                                        }
                                    }
                                },
                                span { class: "solar-icon button-icon icon-import" }
                                "Import settings from old app"
                            }
                            button {
                                class: "save",
                                onclick: move |_| step.set(1),
                                "Set hotkey"
                            }
                        }
                    }
                } else if step() == 1 {
                    section { class: "welcome-screen welcome-hotkey-screen",
                        div { class: "welcome-kicker", "Step 2 of 3" }
                        h1 { "Bind mic toggle" }
                        p { class: "welcome-subtitle", "Press the shortcut you want. Recording is already active, because making you click Record here would be deranged." }
                        div { class: "welcome-keycaps recording",
                            for part in welcome_display_shortcut(
                                modifier_hold_shortcut(),
                                captured_shortcut(),
                            ).parts() {
                                span { class: "welcome-keycap", "{part}" }
                            }
                        }
                        p { class: "welcome-hint", "Hold only modifiers for one second to bind them without another key." }
                        div { class: "welcome-actions",
                            button {
                                class: "secondary",
                                onclick: move |_| step.set(0),
                                "Back"
                            }
                            button {
                                class: "save",
                                onclick: move |_| step.set(2),
                                "Looks good"
                            }
                        }
                    }
                } else if returning_user() {
                    section { class: "welcome-screen",
                        div { class: "welcome-kicker", "Imported settings" }
                        h1 { "Welcome Back!" }
                        div { class: "welcome-feature-list returning",
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-oven-mitts-bold" }
                                h3 { "Hold actions" }
                                p { "Hold to mute, unmute, or toggle without rebuilding your entire muscle memory. Revolutionary shit." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-monitor-bold" }
                                h3 { "Better overlay" }
                                p { "More styles, live positioning, and fewer reasons to squint at your own damn mic state." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-gamepad-bold" }
                                h3 { "Controller support" }
                                p { "Bind mute controls to gamepad buttons like the couch was always part of the plan." }
                            }
                        }
                        div { class: "welcome-actions single",
                            button { class: "save", onclick: finish, "Continue muting" }
                        }
                    }
                } else {
                    section { class: "welcome-screen welcome-final-screen",
                        div { class: "welcome-kicker", "All set" }
                        h1 { "Ready to mute" }
                        p { class: "welcome-subtitle", "Hotkeys wake up after this. Until now they were politely restrained, which is more than can be said for most software." }
                        div { class: "welcome-actions single",
                            button { class: "save", onclick: finish, "Start muting" }
                        }
                    }
                }
            }
        }
    }
}

fn welcome_error_is_not_found(err: &anyhow::Error) -> bool {
    err.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == ErrorKind::NotFound)
            || cause.to_string().to_ascii_lowercase().contains("not found")
    })
}

fn complete_capture_progress(
    mut capture_progress: Signal<f64>,
    mut capture_completed: Signal<bool>,
) {
    capture_completed.set(true);
    capture_progress.set(1.0);
}

fn welcome_display_shortcut(
    live_shortcut: Option<crate::Shortcut>,
    captured_shortcut: crate::Shortcut,
) -> crate::Shortcut {
    live_shortcut.unwrap_or(captured_shortcut)
}

fn welcome_current_modifier_shortcut() -> Option<crate::Shortcut> {
    let ctrl = crate::key_down(crate::VK_CONTROL);
    let alt = crate::key_down(crate::VK_MENU);
    let shift = crate::key_down(crate::VK_SHIFT);
    let win = crate::key_down(crate::VK_LWIN) || crate::key_down(crate::VK_RWIN);
    if ctrl || alt || shift || win {
        Some(crate::Shortcut {
            ctrl,
            alt,
            shift,
            win,
            vk: 0,
            mouse_buttons: Vec::new(),
        })
    } else {
        None
    }
}

fn welcome_shortcut_from_keyboard_data(
    data: &dioxus::events::KeyboardData,
) -> Option<crate::Shortcut> {
    let code = format!("{:?}", data.code());
    let vk = welcome_vk_from_code(&code)?;
    let modifiers = data.modifiers();
    let modifier_only = crate::is_modifier(vk);
    Some(crate::Shortcut {
        ctrl: modifiers.ctrl() || matches!(vk, crate::VK_CONTROL),
        alt: modifiers.alt() || matches!(vk, crate::VK_MENU),
        shift: modifiers.shift() || matches!(vk, crate::VK_SHIFT),
        win: modifiers.meta() || matches!(vk, crate::VK_LWIN | crate::VK_RWIN),
        vk: if modifier_only { 0 } else { vk },
        mouse_buttons: Vec::new(),
    })
}

fn welcome_vk_from_code(code: &str) -> Option<u32> {
    if let Some(letter) = code.strip_prefix("Key") {
        return letter
            .as_bytes()
            .first()
            .map(|byte| byte.to_ascii_uppercase() as u32);
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.as_bytes().first().map(|byte| *byte as u32);
    }
    if let Some(number) = code.strip_prefix('F') {
        let n = number.parse::<u32>().ok()?;
        if (1..=24).contains(&n) {
            return Some(crate::VK_F1 + n - 1);
        }
    }
    match code {
        "ShiftLeft" | "ShiftRight" | "Shift" => Some(crate::VK_SHIFT),
        "ControlLeft" | "ControlRight" | "Control" => Some(crate::VK_CONTROL),
        "AltLeft" | "AltRight" | "Alt" => Some(crate::VK_MENU),
        "MetaLeft" | "MetaRight" | "Meta" => Some(crate::VK_LWIN),
        "Space" => Some(0x20),
        "Tab" => Some(0x09),
        "Enter" => Some(0x0D),
        "Escape" => Some(0x1B),
        "Backspace" => Some(0x08),
        "ArrowLeft" => Some(0x25),
        "ArrowUp" => Some(0x26),
        "ArrowRight" => Some(0x27),
        "ArrowDown" => Some(0x28),
        _ => None,
    }
}
