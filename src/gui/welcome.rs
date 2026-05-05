use std::{
    io::ErrorKind,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use dioxus::prelude::*;

use super::SettingsSnapshot;

const WELCOME_CSS: Asset = asset!("/assets/styles/welcome.css", AssetOptions::css());
const WELCOME_STARS_JS: Asset = asset!("/assets/scripts/welcome-stars.js");
static NEXT_WELCOME_KEYCAPS_ID: AtomicUsize = AtomicUsize::new(1);

#[component]
pub(super) fn WelcomeSequence(
    mut settings: Signal<SettingsSnapshot>,
    mut main_intro: Signal<bool>,
) -> Element {
    let mut step = use_signal(|| 0_usize);
    let mut returning_user = use_signal(|| false);
    let mut import_error = use_signal(String::new);
    let mut closing = use_signal(|| false);
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
        if closing() {
            return;
        }

        closing.set(true);
        spawn(async move {
            tokio::time::sleep(Duration::from_millis(540)).await;
            if crate::complete_welcome().is_ok() {
                main_intro.set(true);
                let next = settings.peek().clone().refresh(false);
                settings.set(next);

                tokio::time::sleep(Duration::from_millis(800)).await;
                main_intro.set(false);
            }
        });
    };
    let welcome_progress = format!("{:.2}%", capture_progress() * 100.0);
    let welcome_keycaps_id = use_hook(|| {
        format!(
            "welcome-keycaps-{}",
            NEXT_WELCOME_KEYCAPS_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let shortcut_parts =
        welcome_display_shortcut(modifier_hold_shortcut(), captured_shortcut()).parts();
    let welcome_step = step();
    let welcome_keycaps_observer_id = welcome_keycaps_id.clone();

    use_effect(use_reactive!(|(
        welcome_step,
        welcome_keycaps_observer_id,
    )| {
        if welcome_step != 1 {
            return;
        }
        spawn(async move {
            let script = format!(
                r#"
const setupWelcomeKeycapAnimator = () => {{
  const root = document.querySelector('[data-welcome-keycaps-id="{welcome_keycaps_observer_id}"]');
  if (!root) {{
    return;
  }}

  window.__welcomeKeycapRects ??= new Map();
  window.__welcomeKeycapObservers ??= new Map();

  window.__welcomeKeycapObservers.get("{welcome_keycaps_observer_id}")?.disconnect();

  const readRects = () => {{
    const rects = new Map();
    root.querySelectorAll(".welcome-keycap").forEach((keycap) => {{
      const id = keycap.dataset.keycapId;
      if (!id) {{
        return;
      }}

      const rect = keycap.getBoundingClientRect();
      rects.set(id, {{ left: rect.left, top: rect.top }});
    }});
    return rects;
  }};

  const animateFrom = (previousRects) => {{
  const nextRects = new Map();

  root.querySelectorAll(".welcome-keycap").forEach((keycap) => {{
    const id = keycap.dataset.keycapId;
    if (!id) {{
      return;
    }}

    const rect = keycap.getBoundingClientRect();
    nextRects.set(id, {{ left: rect.left, top: rect.top }});

    const previous = previousRects.get(id);
    if (!previous) {{
      return;
    }}

    const dx = previous.left - rect.left;
    const dy = previous.top - rect.top;
    if (Math.abs(dx) < 0.5 && Math.abs(dy) < 0.5) {{
      return;
    }}

    keycap.animate(
      [
        {{ transform: `translate(${{dx}}px, ${{dy}}px)` }},
        {{ transform: "translate(0, 0)" }}
      ],
      {{
        duration: 220,
        easing: "cubic-bezier(0.22, 1, 0.36, 1)"
      }}
    );
  }});

    window.__welcomeKeycapRects.set("{welcome_keycaps_observer_id}", nextRects);
  }};

  window.__welcomeKeycapRects.set("{welcome_keycaps_observer_id}", readRects());

  const observer = new MutationObserver(() => {{
    const previousRects = window.__welcomeKeycapRects.get("{welcome_keycaps_observer_id}") ?? new Map();
    animateFrom(previousRects);
  }});

  observer.observe(root, {{
    childList: true,
    subtree: true,
    characterData: true
  }});

  window.__welcomeKeycapObservers.set("{welcome_keycaps_observer_id}", observer);
}};

setupWelcomeKeycapAnimator();
"#
            );
            let _ = dioxus::document::eval(&script).await;
        });
    }));

    rsx! {
        link { rel: "stylesheet", href: "{WELCOME_CSS}" }
        script { src: "{WELCOME_STARS_JS}" }
        main {
            class: {
                let capture_class = if capture_completed() { " capture-completed" } else { "" };
                let closing_class = if closing() { " closing" } else { "" };
                format!("welcome-shell{capture_class}{closing_class}")
            },
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
            canvas {
                class: "welcome-stars-canvas",
                "data-welcome-stars": "true",
                "data-hotkey-recorded": "{capture_completed()}",
                aria_hidden: "true",
            }
            div {
                class: "welcome-progress-bar",
                aria_hidden: "true",
                span {
                    class: "welcome-progress-fill",
                    style: "width: {welcome_progress};",
                }
            }
            div {
                class: "welcome-logo",
                span { "silence" }
                strong { "!" }
            }
            div { class: "welcome-stage",
                if step() == 0 {
                    section { class: "welcome-screen",
                        div { class: "welcome-kicker", " " }
                        div { class: "welcome-heading-copy",
                            h1 { "Hi! Welcome to silence!" }
                            p { "The best app to mute your mic. Period." }
                        }
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
                                            complete_capture_progress(capture_progress, capture_completed);
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
                                "Import settings from v.1.X.X app"
                            }
                            button {
                                class: "save",
                                onclick: move |_| {
                                    crate::set_settings_hotkey_recording(true);
                                    step.set(1);
                                    focus_welcome_shell();
                                },
                                "Set hotkey"
                                span { class: "solar-icon button-icon icon-arrow-right" }
                            }
                        }
                    }
                } else if step() == 1 {
                    section { class: "welcome-screen welcome-hotkey-screen",
                        div { class: "welcome-kicker", "Step 2 of 3" }
                        div { class: "welcome-heading-copy",
                            h1 { "Choose your hotkey" }
                        }
                        div {
                            class: "welcome-keycaps recording",
                            "data-welcome-keycaps-id": "{welcome_keycaps_id}",
                            for (index, part) in shortcut_parts.iter().enumerate() {
                                span {
                                    key: "{index}-{part}",
                                    class: "welcome-keycap",
                                    "data-keycap-id": "{index}-{part}",
                                    "{part}"
                                }
                            }
                        }
                        // p { class: "welcome-hint", "Hold only modifiers for one second to bind them without another key." }
                        div { class: "welcome-actions",
                            button {
                                class: "secondary",
                                onclick: move |_| {
                                    modifier_hold_started.set(None);
                                    modifier_hold_shortcut.set(None);
                                    capture_progress.set(0.0);
                                    capture_completed.set(false);
                                    step.set(0);
                                },
                                "Back"
                            }
                            button {
                                class: "save",
                                onclick: move |_| {
                                    let _ = crate::set_welcome_toggle_shortcut(captured_shortcut());
                                    complete_capture_progress(capture_progress, capture_completed);
                                    step.set(2);
                                },
                                "Looks good"
                                span { class: "solar-icon button-icon icon-arrow-right" }
                            }
                        }
                    }
                } else if returning_user() {
                    section { class: "welcome-screen",
                        div { class: "welcome-kicker", "Oh, it's you!" }
                        div { class: "welcome-heading-copy",
                        h1 { "Welcome Back!" }
                        p {"Have a look at new features"}
                        }
                        div { class: "welcome-feature-list returning",
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-microphone-3-bold" }
                                h3 { "Managing devices" }
                                p { "Pick exact mics or target every input." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-monitor-bold" }
                                h3 { "Better overlay" }
                                p { "More styles with live positioning." }
                            }
                            div { class: "welcome-feature-card",
                                span { class: "solar-icon icon-keyboard-bold" }
                                h3 { "New hotkeys system" }
                                p { "Bind keys, mouse, or controller buttons." }
                            }
                        }
                        div { class: "welcome-actions single",
                            button { class: "save", onclick: finish,
                                "Continue muting"
                                span { class: "solar-icon button-icon icon-arrow-right" }
                            }
                        }
                    }
                } else {
                    section { class: "welcome-screen welcome-final-screen",
                        div { class: "welcome-kicker", "All set" }
                        div { class: "welcome-heading-copy",
                            h1 { "Ready to mute" }
                            p { class: "welcome-subtitle", "Explore a bunch of options to customize your experience. Multiple hotkeys, custom sounds, managing your audio devices, gamepad support and more..." }
                        }
                        div { class: "welcome-actions single",
                            button { class: "save", onclick: finish,
                                "Start muting"
                                span { class: "solar-icon button-icon icon-arrow-right" }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn focus_welcome_shell() {
    spawn(async move {
        let script = r#"
requestAnimationFrame(() => {
  document.querySelector('.welcome-shell')?.focus();
});
"#;
        let _ = dioxus::document::eval(script).await;
    });
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
    let vk = crate::vk_from_keyboard_code(&code)?;
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
