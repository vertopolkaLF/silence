use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use dioxus::prelude::*;

use crate::gui::controls::{Checkbox, Select, SelectOption};

const XBOX_BUTTON_A_ICON: Asset = asset!("/assets/gamepad/xbox_button_a.png");
const XBOX_BUTTON_B_ICON: Asset = asset!("/assets/gamepad/xbox_button_b.png");
const XBOX_BUTTON_MENU_ICON: Asset = asset!("/assets/gamepad/xbox_button_menu.png");
const XBOX_BUTTON_SHARE_ICON: Asset = asset!("/assets/gamepad/xbox_button_share.png");
const XBOX_BUTTON_VIEW_ICON: Asset = asset!("/assets/gamepad/xbox_button_view.png");
const XBOX_BUTTON_X_ICON: Asset = asset!("/assets/gamepad/xbox_button_x.png");
const XBOX_BUTTON_Y_ICON: Asset = asset!("/assets/gamepad/xbox_button_y.png");
const XBOX_DPAD_DOWN_ICON: Asset = asset!("/assets/gamepad/xbox_dpad_down_outline.png");
const XBOX_DPAD_LEFT_ICON: Asset = asset!("/assets/gamepad/xbox_dpad_left_outline.png");
const XBOX_DPAD_RIGHT_ICON: Asset = asset!("/assets/gamepad/xbox_dpad_right_outline.png");
const XBOX_DPAD_UP_ICON: Asset = asset!("/assets/gamepad/xbox_dpad_up_outline.png");
const XBOX_LB_ICON: Asset = asset!("/assets/gamepad/xbox_lb.png");
const XBOX_LS_ICON: Asset = asset!("/assets/gamepad/xbox_ls.png");
const XBOX_LT_ICON: Asset = asset!("/assets/gamepad/xbox_lt.png");
const XBOX_RB_ICON: Asset = asset!("/assets/gamepad/xbox_rb.png");
const XBOX_RS_ICON: Asset = asset!("/assets/gamepad/xbox_rs.png");
const XBOX_RT_ICON: Asset = asset!("/assets/gamepad/xbox_rt.png");
const MODIFIER_HOLD_DURATION: Duration = Duration::from_millis(1000);
const DEFAULT_TARGET_LABEL: &str = "Default";
const ALL_MICROPHONES_LABEL: &str = "All microphones";
static NEXT_SHORTCUT_DISPLAY_ID: AtomicUsize = AtomicUsize::new(1);

#[derive(Clone, Copy, PartialEq, Eq)]
enum HotkeySource {
    Keyboard,
    Gamepad,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModifierKey {
    Ctrl,
    Alt,
    Shift,
    Win,
}

impl ModifierKey {
    fn label(self) -> &'static str {
        match self {
            Self::Ctrl => "Ctrl",
            Self::Alt => "Alt",
            Self::Shift => "Shift",
            Self::Win => "Win",
        }
    }

    fn is_down(self, shortcut: &crate::Shortcut) -> bool {
        match self {
            Self::Ctrl => shortcut.ctrl,
            Self::Alt => shortcut.alt,
            Self::Shift => shortcut.shift,
            Self::Win => shortcut.win,
        }
    }
}

pub fn render(
    settings: Signal<super::super::SettingsSnapshot>,
    mut modal_request: Signal<Option<super::super::HotkeyModalRequest>>,
) -> Element {
    let snapshot = settings();
    let hotkeys = snapshot.config.hotkeys.clone();
    let devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();

    rsx! {
        section {
            class: "hotkeys-panel",
            id: "hotkeys-overview",
            "data-settings-section": "true",
            div { class: "sounds-header section-head-row",
                h1 { "Hotkeys" }
                button {
                    class: "secondary add-hotkey-button",
                    onclick: move |_| {
                        modal_request.set(Some(super::super::HotkeyModalRequest::Add {
                            preset_action: None,
                        }));
                    },
                    span { class: "solar-icon button-icon icon-plus" }
                    "Add hotkey"
                }
            }

            div { class: "hotkey-table",
                div { class: "hotkey-table-head",
                    span { "Action" }
                    span { "Hotkey" }
                    span { "Options" }
                }

                if hotkeys.is_empty() {
                    div { class: "hotkey-empty",
                        span { class: "solar-icon icon-keyboard-bold" }
                        p { "No hotkeys configured." }
                    }
                }

                for hotkey in hotkeys {
                    HotkeyRow {
                        key: "{hotkey.id}",
                        hotkey: hotkey.clone(),
                        devices: devices.clone(),
                        output_devices: output_devices.clone(),
                        settings,
                        onedit: move |binding: crate::HotkeyBinding| {
                            modal_request.set(Some(super::super::HotkeyModalRequest::Edit {
                                binding,
                            }));
                        }
                    }
                }
            }
        }
    }
}

pub fn modal_host(
    settings: Signal<super::super::SettingsSnapshot>,
    mut modal_request: Signal<Option<super::super::HotkeyModalRequest>>,
) -> Element {
    let modal = use_signal(|| None::<ModalMode>);
    let mut recording = use_signal(|| false);
    let mut modifier_hold_started = use_signal(|| None::<Instant>);
    let mut hold_progress = use_signal(|| 0.0);
    let pending_exiting = use_signal(|| false);
    let panel_closing = use_signal(|| false);
    let mut live_modifier_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut hold_timer_shortcut = use_signal(|| None::<crate::Shortcut>);
    let modifier_press_order = use_signal(Vec::<ModifierKey>::new);
    let mut draft_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut recording_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut draft_gamepad = use_signal(|| None::<crate::GamepadShortcut>);
    let mut recording_gamepad = use_signal(|| None::<crate::GamepadShortcut>);
    let draft_source = use_signal(|| HotkeySource::Keyboard);
    let draft_action = use_signal(|| crate::HotkeyAction::ToggleMute);
    let draft_target = use_signal(String::new);
    let draft_target_2 = use_signal(String::new);
    let draft_ignore_modifiers = use_signal(|| false);

    use_effect(move || {
        crate::set_settings_hotkey_recording(recording());
        crate::set_settings_gamepad_recording(
            recording() && draft_source() == HotkeySource::Gamepad,
        );
    });

    use_future(move || async move {
        loop {
            if recording() {
                if draft_source() == HotkeySource::Gamepad {
                    let mut inputs = crate::settings_gamepad_held_inputs();
                    inputs.retain(|input| matches!(input, crate::GamepadInput::Button { .. }));
                    inputs.truncate(2);
                    if inputs.len() >= 2 {
                        let shortcut = crate::GamepadShortcut { inputs };
                        draft_gamepad.set(Some(shortcut.clone()));
                        recording_gamepad.set(Some(shortcut.clone()));
                        if let Some(id) = match modal() {
                            Some(ModalMode::Edit(id)) => Some(id),
                            _ => None,
                        } {
                            apply_draft_to_binding(
                                settings,
                                id,
                                draft_shortcut().unwrap_or_default(),
                                Some(shortcut.clone()),
                                draft_action(),
                                draft_target(),
                                draft_target_2(),
                                false,
                            );
                        }
                        recording.set(false);
                        modifier_hold_started.set(None);
                        hold_progress.set(0.0);
                    } else if inputs.len() == 1 {
                        let shortcut = crate::GamepadShortcut { inputs };
                        recording_gamepad.set(Some(shortcut.clone()));
                        if modifier_hold_started().is_none() {
                            modifier_hold_started.set(Some(Instant::now()));
                            hold_progress.set(0.0);
                        }
                        let progress = modifier_hold_started()
                            .map(|started| {
                                (started.elapsed().as_secs_f64()
                                    / MODIFIER_HOLD_DURATION.as_secs_f64())
                                .clamp(0.0, 1.0)
                            })
                            .unwrap_or(0.0);
                        hold_progress.set(progress);
                        if progress >= 1.0 {
                            draft_gamepad.set(Some(shortcut.clone()));
                            if let Some(id) = match modal() {
                                Some(ModalMode::Edit(id)) => Some(id),
                                _ => None,
                            } {
                                apply_draft_to_binding(
                                    settings,
                                    id,
                                    draft_shortcut().unwrap_or_default(),
                                    Some(shortcut),
                                    draft_action(),
                                    draft_target(),
                                    draft_target_2(),
                                    false,
                                );
                            }
                            recording.set(false);
                            modifier_hold_started.set(None);
                            hold_progress.set(0.0);
                            animate_pending_out(pending_exiting);
                        }
                    } else {
                        recording_gamepad.set(None);
                        modifier_hold_started.set(None);
                        hold_progress.set(0.0);
                    }
                } else if let Some(started) = modifier_hold_started() {
                    let current = current_mouse_shortcut().or_else(current_modifier_shortcut);
                    if let Some(shortcut) = current.clone() {
                        recording_shortcut.set(Some(shortcut.clone()));
                        live_modifier_shortcut.set(Some(shortcut));
                    }
                    if current != hold_timer_shortcut() {
                        hold_timer_shortcut.set(current.clone());
                        modifier_hold_started.set(current.as_ref().map(|_| Instant::now()));
                        hold_progress.set(0.0);
                        continue;
                    }
                    if let Some(shortcut) = current.clone().filter(|shortcut| {
                        !shortcut.mouse_buttons.is_empty() && shortcut_has_modifier(shortcut)
                    }) {
                        draft_shortcut.set(Some(shortcut.clone()));
                        recording_shortcut.set(Some(shortcut.clone()));
                        if let Some(id) = match modal() {
                            Some(ModalMode::Edit(id)) => Some(id),
                            _ => None,
                        } {
                            apply_draft_to_binding(
                                settings,
                                id,
                                shortcut.clone(),
                                None,
                                draft_action(),
                                draft_target(),
                                draft_target_2(),
                                draft_ignore_modifiers(),
                            );
                        }
                        recording.set(false);
                        modifier_hold_started.set(None);
                        hold_timer_shortcut.set(None);
                        hold_progress.set(0.0);
                        live_modifier_shortcut.set(None);
                        animate_pending_out(pending_exiting);
                        continue;
                    }
                    let progress = (started.elapsed().as_secs_f64()
                        / MODIFIER_HOLD_DURATION.as_secs_f64())
                    .clamp(0.0, 1.0);
                    hold_progress.set(progress);
                    if progress >= 1.0 {
                        if let Some(shortcut) = current {
                            draft_shortcut.set(Some(shortcut.clone()));
                            recording_shortcut.set(Some(shortcut.clone()));
                            if let Some(id) = match modal() {
                                Some(ModalMode::Edit(id)) => Some(id),
                                _ => None,
                            } {
                                apply_draft_to_binding(
                                    settings,
                                    id,
                                    shortcut.clone(),
                                    None,
                                    draft_action(),
                                    draft_target(),
                                    draft_target_2(),
                                    draft_ignore_modifiers(),
                                );
                            }
                            recording.set(false);
                            modifier_hold_started.set(None);
                            hold_timer_shortcut.set(None);
                            hold_progress.set(0.0);
                            live_modifier_shortcut.set(None);
                            animate_pending_out(pending_exiting);
                        } else {
                            recording_shortcut.set(None);
                            live_modifier_shortcut.set(None);
                            hold_timer_shortcut.set(None);
                            modifier_hold_started.set(None);
                            hold_progress.set(0.0);
                        }
                    }
                } else if let Some(shortcut) = crate::take_settings_mouse_pressed_shortcut() {
                    recording_shortcut.set(Some(shortcut.clone()));
                    live_modifier_shortcut.set(Some(shortcut.clone()));
                    draft_shortcut.set(Some(shortcut.clone()));
                    if let Some(id) = match modal() {
                        Some(ModalMode::Edit(id)) => Some(id),
                        _ => None,
                    } {
                        apply_draft_to_binding(
                            settings,
                            id,
                            shortcut.clone(),
                            None,
                            draft_action(),
                            draft_target(),
                            draft_target_2(),
                            draft_ignore_modifiers(),
                        );
                    }
                    recording.set(false);
                    live_modifier_shortcut.set(None);
                    hold_timer_shortcut.set(None);
                    hold_progress.set(0.0);
                } else if let Some(shortcut) = current_mouse_shortcut() {
                    recording_shortcut.set(Some(shortcut.clone()));
                    live_modifier_shortcut.set(Some(shortcut.clone()));
                    hold_timer_shortcut.set(Some(shortcut));
                    modifier_hold_started.set(Some(Instant::now()));
                    hold_progress.set(0.0);
                } else if hold_progress() != 0.0 {
                    recording_shortcut.set(None);
                    live_modifier_shortcut.set(None);
                    hold_timer_shortcut.set(None);
                    hold_progress.set(0.0);
                } else if live_modifier_shortcut().is_some() || recording_shortcut().is_some() {
                    recording_shortcut.set(None);
                    live_modifier_shortcut.set(None);
                    hold_timer_shortcut.set(None);
                }
            } else if hold_progress() != 0.0 {
                recording_shortcut.set(None);
                live_modifier_shortcut.set(None);
                hold_timer_shortcut.set(None);
                hold_progress.set(0.0);
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    });

    use_effect(move || {
        let Some(request) = modal_request() else {
            return;
        };

        match request {
            super::super::HotkeyModalRequest::Add { preset_action } => start_modal(
                None,
                None,
                preset_action,
                settings,
                modal,
                recording,
                modifier_hold_started,
                hold_progress,
                pending_exiting,
                panel_closing,
                live_modifier_shortcut,
                modifier_press_order,
                recording_shortcut,
                draft_shortcut,
                draft_gamepad,
                recording_gamepad,
                draft_source,
                draft_action,
                draft_target,
                draft_target_2,
                draft_ignore_modifiers,
            ),
            super::super::HotkeyModalRequest::Edit { binding } => start_modal(
                Some(binding.id.clone()),
                Some(binding),
                None,
                settings,
                modal,
                recording,
                modifier_hold_started,
                hold_progress,
                pending_exiting,
                panel_closing,
                live_modifier_shortcut,
                modifier_press_order,
                recording_shortcut,
                draft_shortcut,
                draft_gamepad,
                recording_gamepad,
                draft_source,
                draft_action,
                draft_target,
                draft_target_2,
                draft_ignore_modifiers,
            ),
        };
        modal_request.set(None);
    });

    let snapshot = settings();
    let devices = snapshot.devices.clone();
    let output_devices = snapshot.output_devices.clone();
    let modal_mode = modal();
    let can_create = match draft_source() {
        HotkeySource::Keyboard => draft_shortcut().is_some(),
        HotkeySource::Gamepad => draft_gamepad().is_some(),
    };

    rsx! {
        if let Some(mode) = modal_mode {
            HotkeyPanel {
                mode: mode.clone(),
                devices,
                output_devices,
                recording,
                modifier_hold_started,
                hold_progress,
                pending_exiting,
                panel_closing,
                live_modifier_shortcut,
                modifier_press_order,
                recording_shortcut,
                draft_shortcut,
                draft_gamepad,
                recording_gamepad,
                draft_source,
                draft_action,
                draft_target,
                draft_target_2,
                draft_ignore_modifiers,
                can_create,
                settings,
                onclose: move |_| {
                    close_modal(
                        settings,
                        modal,
                        recording,
                        modifier_hold_started,
                        hold_progress,
                        pending_exiting,
                        panel_closing,
                        live_modifier_shortcut,
                        modifier_press_order,
                        recording_shortcut,
                    );
                },
                oncreate: {
                    let save_mode = mode.clone();
                    move |_| {
                    if !matches!(save_mode, ModalMode::Add) {
                        return;
                    }
                    match draft_source() {
                        HotkeySource::Keyboard => {
                        if let Some(shortcut) = draft_shortcut() {
                            let action = draft_action();
                            let target = draft_target_for(action, draft_target());
                            let target_2 = draft_second_target_for(action, draft_target_2());
                            super::super::update_settings(settings, |config| {
                                config.hotkeys_paused = false;
                                config.hotkeys.push(crate::HotkeyBinding {
                                    shortcut,
                                    gamepad: None,
                                    action,
                                    target,
                                    target_2,
                                    ignore_modifiers: draft_ignore_modifiers(),
                                    ..crate::HotkeyBinding::default()
                                });
                                sync_legacy_shortcut(config);
                            });
                            close_modal(
                                settings,
                                modal,
                                recording,
                                modifier_hold_started,
                                hold_progress,
                                pending_exiting,
                                panel_closing,
                                live_modifier_shortcut,
                                modifier_press_order,
                                recording_shortcut,
                            );
                        }
                        }
                        HotkeySource::Gamepad => {
                        if let Some(gamepad) = draft_gamepad() {
                            let action = draft_action();
                            let target = draft_target_for(action, draft_target());
                            let target_2 = draft_second_target_for(action, draft_target_2());
                            super::super::update_settings(settings, |config| {
                                config.hotkeys_paused = false;
                                config.hotkeys.push(crate::HotkeyBinding {
                                    gamepad: Some(gamepad),
                                    action,
                                    target,
                                    target_2,
                                    ignore_modifiers: false,
                                    ..crate::HotkeyBinding::default()
                                });
                                sync_legacy_shortcut(config);
                            });
                            close_modal(
                                settings,
                                modal,
                                recording,
                                modifier_hold_started,
                                hold_progress,
                                pending_exiting,
                                panel_closing,
                                live_modifier_shortcut,
                                modifier_press_order,
                                recording_shortcut,
                            );
                        }
                        }
                    }
                    }
                }
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum ModalMode {
    Add,
    Edit(String),
}

fn start_modal(
    edit_id: Option<String>,
    binding: Option<crate::HotkeyBinding>,
    preset_action: Option<crate::HotkeyAction>,
    settings: Signal<super::super::SettingsSnapshot>,
    mut modal: Signal<Option<ModalMode>>,
    mut recording: Signal<bool>,
    mut modifier_hold_started: Signal<Option<Instant>>,
    mut hold_progress: Signal<f64>,
    mut pending_exiting: Signal<bool>,
    mut panel_closing: Signal<bool>,
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    mut modifier_press_order: Signal<Vec<ModifierKey>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_gamepad: Signal<Option<crate::GamepadShortcut>>,
    mut recording_gamepad: Signal<Option<crate::GamepadShortcut>>,
    mut draft_source: Signal<HotkeySource>,
    mut draft_action: Signal<crate::HotkeyAction>,
    mut draft_target: Signal<String>,
    mut draft_target_2: Signal<String>,
    mut draft_ignore_modifiers: Signal<bool>,
) {
    if let Some(binding) = binding {
        draft_shortcut.set(Some(binding.shortcut));
        draft_gamepad.set(binding.gamepad.clone());
        draft_source.set(if binding.gamepad.is_some() {
            HotkeySource::Gamepad
        } else {
            HotkeySource::Keyboard
        });
        draft_action.set(binding.action);
        draft_target.set(binding.target.unwrap_or_default());
        draft_target_2.set(binding.target_2.unwrap_or_default());
        draft_ignore_modifiers.set(binding.ignore_modifiers);
        modal.set(Some(ModalMode::Edit(binding.id)));
    } else {
        draft_shortcut.set(None);
        draft_gamepad.set(None);
        draft_source.set(HotkeySource::Keyboard);
        draft_action.set(preset_action.unwrap_or(crate::HotkeyAction::ToggleMute));
        draft_target.set(String::new());
        draft_target_2.set(String::new());
        draft_ignore_modifiers.set(false);
        modal.set(Some(ModalMode::Add));
    }
    if let Some(id) = edit_id {
        modal.set(Some(ModalMode::Edit(id)));
    }
    recording.set(false);
    modifier_hold_started.set(None);
    hold_progress.set(0.0);
    pending_exiting.set(false);
    panel_closing.set(false);
    live_modifier_shortcut.set(None);
    modifier_press_order.set(Vec::new());
    recording_shortcut.set(None);
    recording_gamepad.set(None);
    super::super::update_settings(settings, |config| {
        config.hotkeys_paused = true;
    });
}

fn close_modal(
    settings: Signal<super::super::SettingsSnapshot>,
    mut modal: Signal<Option<ModalMode>>,
    mut recording: Signal<bool>,
    mut modifier_hold_started: Signal<Option<Instant>>,
    mut hold_progress: Signal<f64>,
    mut pending_exiting: Signal<bool>,
    mut panel_closing: Signal<bool>,
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    mut modifier_press_order: Signal<Vec<ModifierKey>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
) {
    super::super::update_settings(settings, |config| {
        config.hotkeys_paused = false;
    });
    if panel_closing() {
        return;
    }
    panel_closing.set(true);
    recording.set(false);
    modifier_hold_started.set(None);
    hold_progress.set(0.0);
    pending_exiting.set(false);
    live_modifier_shortcut.set(None);
    modifier_press_order.set(Vec::new());
    recording_shortcut.set(None);
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(190)).await;
        modal.set(None);
        panel_closing.set(false);
    });
}

fn animate_pending_out(mut pending_exiting: Signal<bool>) {
    pending_exiting.set(true);
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        pending_exiting.set(false);
    });
}

#[component]
fn HotkeyPanel(
    mode: ModalMode,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    mut recording: Signal<bool>,
    mut modifier_hold_started: Signal<Option<Instant>>,
    mut hold_progress: Signal<f64>,
    mut pending_exiting: Signal<bool>,
    panel_closing: Signal<bool>,
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    modifier_press_order: Signal<Vec<ModifierKey>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_gamepad: Signal<Option<crate::GamepadShortcut>>,
    mut recording_gamepad: Signal<Option<crate::GamepadShortcut>>,
    mut draft_source: Signal<HotkeySource>,
    mut draft_action: Signal<crate::HotkeyAction>,
    mut draft_target: Signal<String>,
    mut draft_target_2: Signal<String>,
    mut draft_ignore_modifiers: Signal<bool>,
    can_create: bool,
    settings: Signal<super::super::SettingsSnapshot>,
    onclose: EventHandler<()>,
    oncreate: EventHandler<()>,
) -> Element {
    let title = match &mode {
        ModalMode::Add => "Add hotkey",
        ModalMode::Edit(_) => "Edit hotkey",
    };
    let subtitle = match &mode {
        ModalMode::Add => "Choose the action, then record the shortcut.",
        ModalMode::Edit(_) => "Changes apply as soon as you make them.",
    };
    let editing_id = match &mode {
        ModalMode::Edit(id) => Some(id.clone()),
        ModalMode::Add => None,
    };
    let keydown_editing_id = editing_id.clone();
    let ignore_editing_id = editing_id.clone();
    let action_editing_id = editing_id.clone();
    let target_editing_id = editing_id.clone();
    let target_2_editing_id = editing_id.clone();
    let alt_space_editing_id = editing_id.clone();
    let source = draft_source();
    let action = draft_action();
    let target_is_missing = action.requires_explicit_target()
        && (draft_target().is_empty() || (action.needs_second_target() && draft_target_2().is_empty()));
    let action_devices = devices.clone();
    let action_output_devices = output_devices.clone();
    let backdrop_class = if panel_closing() {
        "hotkey-panel-backdrop exiting"
    } else {
        "hotkey-panel-backdrop"
    };
    let panel_class = if panel_closing() {
        "hotkey-editor-panel exiting"
    } else {
        "hotkey-editor-panel"
    };
    let source_class = match source {
        HotkeySource::Keyboard => "keyboard-active",
        HotkeySource::Gamepad => "gamepad-active",
    };
    let record_height_source = source;
    let record_height_key = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}",
        recording(),
        draft_shortcut()
            .map(|shortcut| shortcut.display())
            .unwrap_or_default(),
        draft_gamepad()
            .map(|shortcut| shortcut.parts().join("|"))
            .unwrap_or_default(),
        recording_shortcut()
            .map(|shortcut| shortcut.display())
            .unwrap_or_default(),
        recording_gamepad()
            .map(|shortcut| shortcut.parts().join("|"))
            .unwrap_or_default(),
        live_modifier_shortcut()
            .map(|shortcut| shortcut.display())
            .unwrap_or_default(),
        modifier_hold_started().is_some(),
        pending_exiting()
    );

    use_effect(use_reactive!(|record_height_source, record_height_key| {
        let _ = record_height_key.as_str();
        spawn(async move {
            let active_pane = match record_height_source {
                HotkeySource::Keyboard => "keyboard",
                HotkeySource::Gamepad => "gamepad",
            };
            let script = format!(
                r#"
const viewport = document.querySelector('[data-hotkey-record-viewport]');
const pane = document.querySelector(`[data-hotkey-record-pane="{active_pane}"]`);
if (viewport && pane) {{
  const applyHeight = () => {{
    viewport.style.setProperty('--record-pane-height', `${{pane.scrollHeight}}px`);
  }};
  requestAnimationFrame(() => requestAnimationFrame(applyHeight));
  setTimeout(applyHeight, 120);
}}
"#
            );
            let _ = dioxus::document::eval(&script).await;
        });
    }));

    use_future(move || {
        let alt_space_editing_id = alt_space_editing_id.clone();
        async move {
            loop {
                if recording()
                    && draft_source() == HotkeySource::Keyboard
                    && crate::take_settings_alt_space_recorded()
                {
                    let shortcut = crate::Shortcut {
                        ctrl: false,
                        alt: true,
                        shift: false,
                        win: false,
                        vk: 0x20,
                        mouse_buttons: Vec::new(),
                    };
                    draft_shortcut.set(Some(shortcut.clone()));
                    recording_shortcut.set(Some(shortcut.clone()));
                    if let Some(id) = alt_space_editing_id.clone() {
                        apply_draft_to_binding(
                            settings,
                            id,
                            shortcut.clone(),
                            None,
                            draft_action(),
                            draft_target(),
                            draft_target_2(),
                            draft_ignore_modifiers(),
                        );
                    }
                    if modifier_hold_started().is_some() {
                        animate_pending_out(pending_exiting);
                    }
                    recording.set(false);
                    modifier_hold_started.set(None);
                    hold_progress.set(0.0);
                    live_modifier_shortcut.set(None);
                }
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        }
    });

    rsx! {
        div {
            class: "{backdrop_class}",
            onclick: move |_| onclose.call(()),
            div {
                class: "{panel_class}",
                tabindex: "0",
                onclick: move |evt| evt.stop_propagation(),
                onkeydown: move |evt| {
                    if !recording() || draft_source() != HotkeySource::Keyboard {
                        return;
                    }
                    evt.prevent_default();
                    if let Some(shortcut) = shortcut_from_keyboard_data(&evt.data()) {
                        if shortcut.vk == 0 {
                            sync_modifier_press_order(modifier_press_order, Some(&shortcut));
                            if let Some(mouse_shortcut) = current_mouse_shortcut()
                                .filter(shortcut_has_modifier)
                            {
                                draft_shortcut.set(Some(mouse_shortcut.clone()));
                                recording_shortcut.set(Some(mouse_shortcut.clone()));
                                if let Some(id) = keydown_editing_id.clone() {
                                    apply_draft_to_binding(
                                        settings,
                                        id,
                                        mouse_shortcut.clone(),
                                        None,
                                        draft_action(),
                                        draft_target(),
                                        draft_target_2(),
                                        draft_ignore_modifiers(),
                                    );
                                }
                                if modifier_hold_started().is_some() {
                                    animate_pending_out(pending_exiting);
                                }
                                recording.set(false);
                                modifier_hold_started.set(None);
                                hold_progress.set(0.0);
                                live_modifier_shortcut.set(None);
                                return;
                            }
                            let current = current_modifier_shortcut().unwrap_or(shortcut);
                            if live_modifier_shortcut() != Some(current.clone()) {
                                modifier_hold_started.set(Some(Instant::now()));
                                hold_progress.set(0.0);
                            }
                            live_modifier_shortcut.set(Some(current));
                        } else {
                            draft_shortcut.set(Some(shortcut.clone()));
                            recording_shortcut.set(Some(shortcut.clone()));
                            if let Some(id) = keydown_editing_id.clone() {
                                apply_draft_to_binding(
                                    settings,
                                    id,
                                    shortcut.clone(),
                                    None,
                                    draft_action(),
                                    draft_target(),
                                    draft_target_2(),
                                    draft_ignore_modifiers(),
                                );
                            }
                            if modifier_hold_started().is_some() {
                                animate_pending_out(pending_exiting);
                            }
                            recording.set(false);
                            modifier_hold_started.set(None);
                            hold_progress.set(0.0);
                            live_modifier_shortcut.set(None);
                        }
                    }
                },
                onkeyup: move |evt| {
                    if !recording() || draft_source() != HotkeySource::Keyboard {
                        return;
                    }
                    evt.prevent_default();
                    let current = current_modifier_shortcut();
                    sync_modifier_press_order(modifier_press_order, current.as_ref());
                    live_modifier_shortcut.set(current.clone());
                    if current.is_none() {
                        if modifier_hold_started().is_some() {
                            animate_pending_out(pending_exiting);
                        }
                        recording_shortcut.set(None);
                        modifier_hold_started.set(None);
                        hold_progress.set(0.0);
                    }
                },
                div { class: "hotkey-panel-head",
                    div { class: "hotkey-panel-title",
                        h2 { "{title}" }
                        p { "{subtitle}" }
                    }
                    button {
                        class: "icon-button",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        span { class: "solar-icon icon-close" }
                    }
                }

                div { class: "hotkey-panel-body",
                    div { class: "hotkey-panel-body-inner",
                    div { class: "hotkey-source-toggle",
                        button {
                            class: if source == HotkeySource::Keyboard { "source-option active" } else { "source-option" },
                            onclick: move |_| {
                                draft_source.set(HotkeySource::Keyboard);
                                recording.set(false);
                                recording_gamepad.set(None);
                                modifier_hold_started.set(None);
                                hold_progress.set(0.0);
                            },
                            span { class: "solar-icon source-option-icon icon-keyboard-bold" }
                            "Keyboard"
                        }
                        button {
                            class: if source == HotkeySource::Gamepad { "source-option active" } else { "source-option" },
                            onclick: move |_| {
                                draft_source.set(HotkeySource::Gamepad);
                                recording.set(false);
                                modifier_hold_started.set(None);
                                hold_progress.set(0.0);
                                live_modifier_shortcut.set(None);
                            },
                            span { class: "solar-icon source-option-icon icon-gamepad-bold" }
                            "Gamepad"
                        }
                    }
                    div { class: "field-group modal-field",
                        label { if source == HotkeySource::Gamepad { "Gamepad input" } else { "Shortcut" } }
                            div {
                                class: "shortcut-record-viewport {source_class}",
                                "data-hotkey-record-viewport": "true",
                                div { class: "shortcut-record-track",
                                    div {
                                        class: "shortcut-record-pane keyboard-pane",
                                        "data-hotkey-record-pane": "keyboard",
                                        div { class: "shortcut-record-stack",
                                            div { class: "shortcut-record-row",
                                                KeyDisplay {
                                                    display_id: Some("hotkey-editor-keyboard-shortcut".to_string()),
                                                    shortcut: if recording() && source == HotkeySource::Keyboard {
                                                        live_modifier_shortcut().or_else(|| recording_shortcut())
                                                    } else {
                                                        draft_shortcut()
                                                    },
                                                    parts_override: if recording() && source == HotkeySource::Keyboard {
                                                        live_modifier_shortcut()
                                                            .or_else(|| recording_shortcut())
                                                            .map(|shortcut| recording_shortcut_parts(&shortcut, &modifier_press_order()))
                                                    } else {
                                                        None
                                                    },
                                                    gamepad: None,
                                                    recording: recording() && source == HotkeySource::Keyboard,
                                                    boxed: true,
                                                    animate: true,
                                                    modifier_hold_active: source == HotkeySource::Keyboard && modifier_hold_started().is_some(),
                                                    pending_exiting: source == HotkeySource::Keyboard && pending_exiting(),
                                                    hold_progress: if source == HotkeySource::Keyboard { hold_progress() } else { 0.0 }
                                                }
                                                button {
                                                    class: "secondary",
                                                    onclick: move |_| {
                                                        draft_source.set(HotkeySource::Keyboard);
                                                        let next = !(recording() && draft_source() == HotkeySource::Keyboard);
                                                        recording.set(next);
                                                        recording_shortcut.set(None);
                                                        recording_gamepad.set(None);
                                                        if modifier_hold_started().is_some() {
                                                            animate_pending_out(pending_exiting);
                                                        }
                                                        modifier_hold_started.set(None);
                                                        hold_progress.set(0.0);
                                                        live_modifier_shortcut.set(None);
                                                    },
                                                    span { class: "solar-icon button-icon icon-record" }
                                                    if recording() && source == HotkeySource::Keyboard {
                                                        "Cancel"
                                                    } else {
                                                        "Record"
                                                    }
                                                }
                                            }
                                            p { class: "shortcut-record-hint", "Hold to bind only modifier keys" }
                                        }
                                    }
                                    div {
                                        class: "shortcut-record-pane gamepad-pane",
                                        "data-hotkey-record-pane": "gamepad",
                                        div { class: "shortcut-record-stack",
                                            div { class: "shortcut-record-row",
                                                KeyDisplay {
                                                    display_id: Some("hotkey-editor-gamepad-shortcut".to_string()),
                                                    shortcut: None,
                                                    gamepad: if source == HotkeySource::Gamepad && recording() {
                                                        recording_gamepad().or_else(|| draft_gamepad())
                                                    } else {
                                                        draft_gamepad()
                                                    },
                                                    recording: recording() && source == HotkeySource::Gamepad,
                                                    boxed: true,
                                                    animate: true,
                                                    modifier_hold_active: source == HotkeySource::Gamepad && modifier_hold_started().is_some(),
                                                    pending_exiting: source == HotkeySource::Gamepad && pending_exiting(),
                                                    hold_progress: if source == HotkeySource::Gamepad { hold_progress() } else { 0.0 }
                                                }
                                                button {
                                                    class: "secondary",
                                                    onclick: move |_| {
                                                        draft_source.set(HotkeySource::Gamepad);
                                                        let next = !(recording() && draft_source() == HotkeySource::Gamepad);
                                                        recording.set(next);
                                                        recording_shortcut.set(None);
                                                        recording_gamepad.set(None);
                                                        if modifier_hold_started().is_some() {
                                                            animate_pending_out(pending_exiting);
                                                        }
                                                        modifier_hold_started.set(None);
                                                        hold_progress.set(0.0);
                                                        live_modifier_shortcut.set(None);
                                                    },
                                                    span { class: "solar-icon button-icon icon-record" }
                                                    if recording() && source == HotkeySource::Gamepad {
                                                        "Cancel"
                                                    } else {
                                                        "Record"
                                                    }
                                                }
                                            }
                                            p { class: "shortcut-record-hint", "Hold one control to bind it, or press a second while the first is still held" }
                                        }
                                    }
                                }
                            }

                    }

                    if source == HotkeySource::Keyboard {
                        Checkbox {
                            class: "modal-check".to_string(),
                            checked: draft_ignore_modifiers(),
                            label: "Ignore modifiers".to_string(),
                            onchange: move |checked: bool| {
                                draft_ignore_modifiers.set(checked);
                                if let (Some(id), Some(shortcut)) = (ignore_editing_id.clone(), draft_shortcut()) {
                                    apply_draft_to_binding(settings, id, shortcut, None, draft_action(), draft_target(), draft_target_2(), checked);
                                }
                            }
                        }
                    }

                    div { class: "hotkey-action-target-row",
                        div { class: "field-group modal-field",
                            label { "Action" }
                            Select {
                                value: action.id().to_string(),
                                options: action_options(),
                                onchange: move |value: String| {
                                    let action = crate::HotkeyAction::from_id(&value);
                                    draft_action.set(action);
                                    let mut target = draft_target();
                                    if !action.needs_target() || !target_is_valid_for_action(action, &target, &action_devices, &action_output_devices) {
                                        target = default_target_for_action(action, &action_devices, &action_output_devices);
                                        draft_target.set(target.clone());
                                    }
                                    let mut target_2 = draft_target_2();
                                    if !action.needs_second_target() || !target_is_valid_for_action(action, &target_2, &action_devices, &action_output_devices) {
                                        target_2 = default_second_target_for_action(action, &target, &action_devices, &action_output_devices);
                                        draft_target_2.set(target_2.clone());
                                    }
                                    if let Some(id) = action_editing_id.clone() {
                                        apply_draft_to_binding(
                                            settings,
                                            id,
                                            draft_shortcut().unwrap_or_default(),
                                            draft_gamepad(),
                                            action,
                                            target,
                                            target_2,
                                            if draft_source() == HotkeySource::Keyboard { draft_ignore_modifiers() } else { false },
                                        );
                                    }
                                }
                            }
                        }

                        if action.needs_target() {
                            TargetSelect {
                                action,
                                label: "Target 1",
                                value: draft_target(),
                                devices: devices.clone(),
                                output_devices: output_devices.clone(),
                                onchange: move |value: String| {
                                    draft_target.set(value.clone());
                                    if let Some(id) = target_editing_id.clone() {
                                        apply_draft_to_binding(
                                            settings,
                                            id,
                                            draft_shortcut().unwrap_or_default(),
                                            draft_gamepad(),
                                            draft_action(),
                                            value,
                                            draft_target_2(),
                                            if draft_source() == HotkeySource::Keyboard { draft_ignore_modifiers() } else { false },
                                        );
                                    }
                                }
                            }
                            if action.needs_second_target() {
                                TargetSelect {
                                    action,
                                    label: "Target 2",
                                    value: draft_target_2(),
                                    devices: devices.clone(),
                                    output_devices: output_devices.clone(),
                                    onchange: move |value: String| {
                                        draft_target_2.set(value.clone());
                                        if let Some(id) = target_2_editing_id.clone() {
                                            apply_draft_to_binding(
                                                settings,
                                                id,
                                                draft_shortcut().unwrap_or_default(),
                                                draft_gamepad(),
                                                draft_action(),
                                                draft_target(),
                                                value,
                                                if draft_source() == HotkeySource::Keyboard { draft_ignore_modifiers() } else { false },
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div { class: "hotkey-panel-actions",
                        if matches!(mode, ModalMode::Add) {
                            button {
                                class: "save",
                                disabled: !can_create || target_is_missing,
                                onclick: move |_| oncreate.call(()),
                                span { class: "solar-icon button-icon icon-plus" }
                                "Add hotkey"
                            }
                        } else {
                            button {
                                class: "secondary",
                                onclick: move |_| onclose.call(()),
                                "Cancel"
                            }
                            button {
                                class: "save",
                                onclick: move |_| onclose.call(()),
                                "Done"
                            }
                        }
                    }
                    }
                }
            }
        }
    }
}

fn apply_draft_to_binding(
    settings: Signal<super::super::SettingsSnapshot>,
    id: String,
    shortcut: crate::Shortcut,
    gamepad: Option<crate::GamepadShortcut>,
    action: crate::HotkeyAction,
    target: String,
    target_2: String,
    ignore_modifiers: bool,
) {
    super::super::update_settings(settings, |config| {
        if let Some(binding) = config.hotkeys.iter_mut().find(|binding| binding.id == id) {
            binding.shortcut = shortcut;
            binding.gamepad = gamepad;
            binding.action = action;
            binding.target = draft_target_for(action, target);
            binding.target_2 = draft_second_target_for(action, target_2);
            binding.ignore_modifiers = ignore_modifiers;
        }
        sync_legacy_shortcut(config);
    });
}

fn draft_second_target_for(action: crate::HotkeyAction, target: String) -> Option<String> {
    if action.needs_second_target() && !target.is_empty() {
        Some(target)
    } else {
        None
    }
}

fn draft_target_for(action: crate::HotkeyAction, target: String) -> Option<String> {
    if action.needs_target() && !target.is_empty() {
        Some(target)
    } else {
        None
    }
}

#[component]
fn KeyDisplay(
    #[props(default)] display_id: Option<String>,
    shortcut: Option<crate::Shortcut>,
    #[props(default)] parts_override: Option<Vec<String>>,
    #[props(default)] gamepad: Option<crate::GamepadShortcut>,
    recording: bool,
    boxed: bool,
    animate: bool,
    modifier_hold_active: bool,
    pending_exiting: bool,
    hold_progress: f64,
) -> Element {
    let parts = parts_override.unwrap_or_else(|| {
        gamepad
            .as_ref()
            .map(|shortcut| shortcut.parts())
            .or_else(|| shortcut.map(|shortcut| shortcut.parts()))
            .unwrap_or_default()
    });
    let gamepad_inputs = gamepad
        .as_ref()
        .map(|shortcut| shortcut.inputs.clone())
        .unwrap_or_default();
    let progress = format!("{:.0}%", hold_progress * 100.0);
    let generated_display_id = use_hook(|| {
        format!(
            "shortcut-display-{}",
            NEXT_SHORTCUT_DISPLAY_ID.fetch_add(1, Ordering::Relaxed)
        )
    });
    let display_id = display_id.unwrap_or(generated_display_id);
    let measure_key = parts.join("|");
    use_effect(use_reactive!(
        |display_id, measure_key, modifier_hold_active, pending_exiting| {
            let has_pending = modifier_hold_active || pending_exiting;
            if !has_pending {
                return;
            }
            let _ = measure_key.as_str();
            spawn(async move {
                let script = format!(
                    r#"
const updatePending = () => {{
  const root = document.querySelector('[data-shortcut-display-id="{display_id}"]');
  const keycapList = root?.querySelector('.keycap-list');
  const pending = root?.querySelector('.shortcut-pending');
  if (!root || !keycapList || !pending) {{
    console.log('[hotkey pending]', 'missing nodes', {{
      displayId: '{display_id}',
      hasRoot: !!root,
      hasKeycapList: !!keycapList,
      hasPending: !!pending
    }});
    return;
  }}

  const styles = getComputedStyle(root);
  const gap = Number.parseFloat(styles.getPropertyValue('--shortcut-gap')) || 0;
  const nextOffset = keycapList.offsetLeft + keycapList.getBoundingClientRect().width + gap;
  window.__hotkeyPendingOffsets ??= new Map();
  const previousOffset = window.__hotkeyPendingOffsets.get('{display_id}') ?? nextOffset;
  window.__hotkeyPendingOffsets.set('{display_id}', nextOffset);

  root.style.setProperty('--pending-offset', `${{nextOffset}}px`);
  console.log('[hotkey pending]', 'measured', {{
    displayId: '{display_id}',
    keycapWidth: keycapList.getBoundingClientRect().width,
    gap,
    previousOffset,
    nextOffset,
    applied: root.style.getPropertyValue('--pending-offset')
  }});

  // The CSS left transition handles the movement. Avoid a transform FLIP here:
  // combining both makes the pending text visually jump back toward the input edge.
}};

requestAnimationFrame(() => requestAnimationFrame(updatePending));
setTimeout(updatePending, 80);
"#
                );
                let _ = dioxus::document::eval(&script).await;
            });
        }
    ));

    rsx! {
        div {
            class: display_class(boxed, recording),
            style: "--hold-progress: {progress};",
            "data-shortcut-display-id": "{display_id}",
            span { class: "keycap-list",
                if gamepad_inputs.is_empty() {
                    for part in parts {
                        span { class: keycap_class(animate), "{part}" }
                    }
                } else {
                    for input in gamepad_inputs {
                        GamepadKeycap {
                            input,
                            animate
                        }
                    }
                }
            }
            if modifier_hold_active || pending_exiting {
                span {
                    class: if pending_exiting { "shortcut-pending exiting" } else { "shortcut-pending" },
                    "+ ..."
                }
            }
        }
    }
}

#[component]
fn HotkeyRow(
    hotkey: crate::HotkeyBinding,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    settings: Signal<super::super::SettingsSnapshot>,
    onedit: EventHandler<crate::HotkeyBinding>,
) -> Element {
    let id = hotkey.id.clone();
    let action = hotkey.action;
    let target_label = target_label(
        action,
        hotkey.target.as_deref(),
        hotkey.target_2.as_deref(),
        &devices,
        &output_devices,
    );
    rsx! {
        div { class: "hotkey-entry",
            div { class: "hotkey-main-row",
                div { class: "hotkey-action-cell",
                    h3 { "{action.label()}" }
                    if action.needs_target() {
                        span { class: "hotkey-target-label", "{target_label}" }
                    }
                }
                div { class: "hotkey-shortcut-cell",
                    KeyDisplay {
                    shortcut: Some(hotkey.shortcut.clone()),
                        gamepad: hotkey.gamepad.clone(),
                        recording: false,
                        boxed: false,
                        animate: false,
                        modifier_hold_active: false,
                        pending_exiting: false,
                        hold_progress: 0.0
                    }
                    if hotkey.gamepad.is_none() && hotkey.ignore_modifiers {
                        span { class: "hotkey-modifier-mode", "Ignores modifiers" }
                    }
                }
                div { class: "hotkey-row-actions",
                    button {
                        class: "icon-button",
                        title: "Edit hotkey",
                        onclick: move |_| onedit.call(hotkey.clone()),
                        span { class: "solar-icon icon-settings" }
                    }
                    button {
                        class: "icon-button danger-button",
                        title: "Remove hotkey",
                        onclick: move |_| {
                            let id = id.clone();
                            super::super::update_settings(settings, |config| {
                                config.hotkeys.retain(|binding| binding.id != id);
                                sync_legacy_shortcut(config);
                            });
                        },
                        span { class: "solar-icon icon-close" }
                    }
                }
            }
        }
    }
}

fn display_class(boxed: bool, recording: bool) -> &'static str {
    match (boxed, recording) {
        (true, true) => "shortcut-display fake-input recording",
        (true, false) => "shortcut-display fake-input",
        (false, true) => "shortcut-display recording",
        (false, false) => "shortcut-display",
    }
}

fn keycap_class(animate: bool) -> &'static str {
    if animate {
        "keycap anim"
    } else {
        "keycap noanim"
    }
}

#[component]
fn GamepadKeycap(input: crate::GamepadInput, animate: bool) -> Element {
    let label = input.label();
    if let Some(icon) = gamepad_icon(input) {
        rsx! {
            span {
                class: gamepad_keycap_class(animate),
                title: "{label}",
                img {
                    class: "gamepad-keycap-icon",
                    src: "{icon}",
                    alt: "{label}"
                }
            }
        }
    } else {
        rsx! {
            span { class: keycap_class(animate), "{label}" }
        }
    }
}

fn gamepad_icon(input: crate::GamepadInput) -> Option<Asset> {
    match input.icon_id()? {
        "xbox_button_a" => Some(XBOX_BUTTON_A_ICON),
        "xbox_button_b" => Some(XBOX_BUTTON_B_ICON),
        "xbox_button_menu" => Some(XBOX_BUTTON_MENU_ICON),
        "xbox_button_share" => Some(XBOX_BUTTON_SHARE_ICON),
        "xbox_button_view" => Some(XBOX_BUTTON_VIEW_ICON),
        "xbox_button_x" => Some(XBOX_BUTTON_X_ICON),
        "xbox_button_y" => Some(XBOX_BUTTON_Y_ICON),
        "xbox_dpad_down_outline" => Some(XBOX_DPAD_DOWN_ICON),
        "xbox_dpad_left_outline" => Some(XBOX_DPAD_LEFT_ICON),
        "xbox_dpad_right_outline" => Some(XBOX_DPAD_RIGHT_ICON),
        "xbox_dpad_up_outline" => Some(XBOX_DPAD_UP_ICON),
        "xbox_lb" => Some(XBOX_LB_ICON),
        "xbox_ls" => Some(XBOX_LS_ICON),
        "xbox_lt" => Some(XBOX_LT_ICON),
        "xbox_rb" => Some(XBOX_RB_ICON),
        "xbox_rs" => Some(XBOX_RS_ICON),
        "xbox_rt" => Some(XBOX_RT_ICON),
        _ => None,
    }
}

fn gamepad_keycap_class(animate: bool) -> &'static str {
    if animate {
        "gamepad-keycap anim"
    } else {
        "gamepad-keycap noanim"
    }
}

#[component]
fn TargetSelect(
    action: crate::HotkeyAction,
    #[props(default = "Target".to_string())] label: String,
    value: String,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
    onchange: EventHandler<String>,
) -> Element {
    let options = target_options(action, devices, output_devices);

    rsx! {
        div { class: "field-group modal-field target-field",
            label { "{label}" }
            Select {
                value: value.clone(),
                options,
                onchange: move |value: String| onchange.call(value)
            }
        }
    }
}

fn target_options(
    action: crate::HotkeyAction,
    devices: Vec<crate::MicDevice>,
    output_devices: Vec<crate::AudioDevice>,
) -> Vec<SelectOption> {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .into_iter()
            .map(|device| SelectOption::new(device.id, device.name).icon("icon-mic"))
            .collect(),
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .into_iter()
            .map(|device| SelectOption::new(device.id, device.name).icon("icon-volume"))
            .collect(),
        _ => {
            let default_detail = default_target_detail(&devices);
            std::iter::once(
                SelectOption::new("", DEFAULT_TARGET_LABEL)
                    .detail(default_detail)
                    .icon("icon-mic"),
            )
            .chain(std::iter::once(
                SelectOption::new(crate::HOTKEY_TARGET_ALL_MICROPHONES, ALL_MICROPHONES_LABEL)
                    .detail("Apply the action to every active microphone")
                    .icon("icon-mic"),
            ))
            .chain(
                devices
                    .into_iter()
                    .map(|device| SelectOption::new(device.id, device.name).icon("icon-mic")),
            )
            .collect()
        }
    }
}

fn action_options() -> Vec<SelectOption> {
    crate::HotkeyAction::ALL
        .iter()
        .map(|action| {
            let option = SelectOption::new(action.id(), action.label());
            match action {
                crate::HotkeyAction::ToggleMute => option.group("Mute").icon("icon-mic"),
                crate::HotkeyAction::Mute => option.group("Mute").icon("icon-mic"),
                crate::HotkeyAction::Unmute => option.group("Mute").icon("icon-mic"),
                crate::HotkeyAction::HoldToToggle => {
                    option.group("Hold to mute").icon("icon-oven-mitts")
                }
                crate::HotkeyAction::HoldToMute => {
                    option.group("Hold to mute").icon("icon-oven-mitts")
                }
                crate::HotkeyAction::HoldToUnmute => {
                    option.group("Hold to mute").icon("icon-oven-mitts")
                }
                crate::HotkeyAction::SetDefaultInputDevice => {
                    option.group("System").icon("icon-mic")
                }
                crate::HotkeyAction::SetDefaultOutputDevice => {
                    option.group("System").icon("icon-volume")
                }
                crate::HotkeyAction::ToggleDefaultInputDevice => {
                    option.group("System").icon("icon-mic")
                }
                crate::HotkeyAction::ToggleDefaultOutputDevice => {
                    option.group("System").icon("icon-volume")
                }
                crate::HotkeyAction::OpenSettings => option.group("System").icon("icon-settings"),
            }
        })
        .collect()
}

fn target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    target_2: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> String {
    if action.needs_second_target() {
        let first = device_target_label(action, target, devices, output_devices);
        let second = device_target_label(action, target_2, devices, output_devices);
        return format!("{first} / {second}");
    }

    device_target_label(action, target, devices, output_devices)
}

fn device_target_label(
    action: crate::HotkeyAction,
    target: Option<&str>,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> String {
    if matches!(
        action,
        crate::HotkeyAction::SetDefaultInputDevice | crate::HotkeyAction::ToggleDefaultInputDevice
    ) {
        return target
            .and_then(|target| devices.iter().find(|device| device.id == target))
            .map(|device| device.name.clone())
            .unwrap_or_else(|| "Choose an input device".to_string());
    }

    if matches!(
        action,
        crate::HotkeyAction::SetDefaultOutputDevice | crate::HotkeyAction::ToggleDefaultOutputDevice
    ) {
        return target
            .and_then(|target| output_devices.iter().find(|device| device.id == target))
            .map(|device| device.name.clone())
            .unwrap_or_else(|| "Choose an output device".to_string());
    }

    if matches!(target, Some(crate::HOTKEY_TARGET_ALL_MICROPHONES)) {
        return ALL_MICROPHONES_LABEL.to_string();
    }

    target
        .filter(|target| !target.is_empty())
        .and_then(|target| devices.iter().find(|device| device.id == target))
        .map(|device| device.name.clone())
        .unwrap_or_else(|| DEFAULT_TARGET_LABEL.to_string())
}

fn target_is_valid_for_action(
    action: crate::HotkeyAction,
    target: &str,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> bool {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => {
            devices.iter().any(|device| device.id == target)
        }
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => {
            output_devices.iter().any(|device| device.id == target)
        }
        _ => true,
    }
}

fn default_target_for_action(
    action: crate::HotkeyAction,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> String {
    match action {
        crate::HotkeyAction::SetDefaultInputDevice
        | crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .first()
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        crate::HotkeyAction::SetDefaultOutputDevice
        | crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .first()
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn default_second_target_for_action(
    action: crate::HotkeyAction,
    target_1: &str,
    devices: &[crate::MicDevice],
    output_devices: &[crate::AudioDevice],
) -> String {
    match action {
        crate::HotkeyAction::ToggleDefaultInputDevice => devices
            .iter()
            .find(|device| device.id != target_1)
            .or_else(|| devices.first())
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        crate::HotkeyAction::ToggleDefaultOutputDevice => output_devices
            .iter()
            .find(|device| device.id != target_1)
            .or_else(|| output_devices.first())
            .map(|device| device.id.clone())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn default_target_detail(devices: &[crate::MicDevice]) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Use the current Windows default microphone".to_string())
}

fn sync_legacy_shortcut(config: &mut crate::Config) {
    if let Some(shortcut) = config
        .hotkeys
        .iter()
        .find(|binding| {
            binding.gamepad.is_none() && binding.action == crate::HotkeyAction::ToggleMute
        })
        .map(|binding| binding.shortcut.clone())
    {
        config.shortcut = shortcut;
    }
}

fn shortcut_from_keyboard_data(data: &dioxus::events::KeyboardData) -> Option<crate::Shortcut> {
    let code = format!("{:?}", data.code());
    let vk = vk_from_code(&code)?;
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

fn current_modifier_shortcut() -> Option<crate::Shortcut> {
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

fn current_mouse_shortcut() -> Option<crate::Shortcut> {
    let mut mouse_buttons = crate::settings_mouse_held_buttons();
    mouse_buttons.truncate(2);
    if mouse_buttons.is_empty() {
        return None;
    }

    Some(crate::Shortcut {
        ctrl: crate::key_down(crate::VK_CONTROL),
        alt: crate::key_down(crate::VK_MENU),
        shift: crate::key_down(crate::VK_SHIFT),
        win: crate::key_down(crate::VK_LWIN) || crate::key_down(crate::VK_RWIN),
        vk: 0,
        mouse_buttons,
    })
}

fn shortcut_has_modifier(shortcut: &crate::Shortcut) -> bool {
    shortcut.ctrl || shortcut.alt || shortcut.shift || shortcut.win
}

fn sync_modifier_press_order(
    mut modifier_press_order: Signal<Vec<ModifierKey>>,
    shortcut: Option<&crate::Shortcut>,
) {
    let Some(shortcut) = shortcut else {
        modifier_press_order.set(Vec::new());
        return;
    };

    let mut order = modifier_press_order();
    order.retain(|modifier| modifier.is_down(shortcut));
    for modifier in [
        ModifierKey::Ctrl,
        ModifierKey::Alt,
        ModifierKey::Shift,
        ModifierKey::Win,
    ] {
        if modifier.is_down(shortcut) && !order.contains(&modifier) {
            order.push(modifier);
        }
    }
    modifier_press_order.set(order);
}

fn recording_shortcut_parts(
    shortcut: &crate::Shortcut,
    modifier_press_order: &[ModifierKey],
) -> Vec<String> {
    let mut parts = Vec::new();
    let mut used_modifiers = Vec::new();
    for modifier in modifier_press_order {
        if modifier.is_down(shortcut) {
            parts.push(modifier.label().to_string());
            used_modifiers.push(*modifier);
        }
    }
    for modifier in [
        ModifierKey::Ctrl,
        ModifierKey::Alt,
        ModifierKey::Shift,
        ModifierKey::Win,
    ] {
        if modifier.is_down(shortcut) && !used_modifiers.contains(&modifier) {
            parts.push(modifier.label().to_string());
        }
    }
    for button in &shortcut.mouse_buttons {
        parts.push(crate::mouse_button_name(*button).to_string());
    }
    if shortcut.vk != 0 {
        parts.push(crate::vk_name(shortcut.vk));
    }
    parts
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
        "ShiftLeft" | "ShiftRight" | "Shift" => Some(crate::VK_SHIFT),
        "ControlLeft" | "ControlRight" | "Control" => Some(crate::VK_CONTROL),
        "AltLeft" | "AltRight" | "Alt" => Some(crate::VK_MENU),
        "MetaLeft" | "MetaRight" | "Meta" => Some(crate::VK_LWIN),
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
