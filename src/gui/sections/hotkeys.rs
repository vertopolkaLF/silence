use std::time::{Duration, Instant};

use dioxus::prelude::*;

use crate::gui::controls::{Checkbox, Select, SelectOption};

const MODIFIER_HOLD_DURATION: Duration = Duration::from_millis(1000);

pub fn render(settings: Signal<super::super::SettingsSnapshot>) -> Element {
    let mut modal = use_signal(|| None::<ModalMode>);
    let mut recording = use_signal(|| false);
    let mut modifier_hold_started = use_signal(|| None::<Instant>);
    let mut hold_progress = use_signal(|| 0.0);
    let mut pending_exiting = use_signal(|| false);
    let mut live_modifier_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut draft_shortcut = use_signal(|| None::<crate::Shortcut>);
    let mut recording_shortcut = use_signal(|| None::<crate::Shortcut>);
    let draft_action = use_signal(|| crate::HotkeyAction::ToggleMute);
    let draft_target = use_signal(String::new);
    let draft_ignore_modifiers = use_signal(|| false);

    use_future(move || async move {
        loop {
            if recording() {
                if let Some(started) = modifier_hold_started() {
                    let progress = (started.elapsed().as_secs_f64()
                        / MODIFIER_HOLD_DURATION.as_secs_f64())
                    .clamp(0.0, 1.0);
                    hold_progress.set(progress);
                    if progress >= 1.0 {
                        if let Some(shortcut) = current_modifier_shortcut() {
                            draft_shortcut.set(Some(shortcut));
                            recording_shortcut.set(Some(shortcut));
                            recording.set(false);
                            modifier_hold_started.set(None);
                            hold_progress.set(0.0);
                            live_modifier_shortcut.set(None);
                            animate_pending_out(pending_exiting);
                        }
                    }
                } else if hold_progress() != 0.0 {
                    hold_progress.set(0.0);
                }
            } else if hold_progress() != 0.0 {
                hold_progress.set(0.0);
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
    });

    let snapshot = settings();
    let hotkeys = snapshot.config.hotkeys.clone();
    let devices = snapshot.devices.clone();
    let modal_mode = modal();
    let can_save = draft_shortcut().is_some();

    rsx! {
        section {
            class: "hotkeys-panel",
            id: "hotkeys-overview",
            "data-settings-section": "true",
            div { class: "sounds-header",
                h1 { "Hotkeys" }
            }

            div { class: "hotkey-table",
                div { class: "hotkey-table-head",
                    span { "Action" }
                    span { "Hotkey" }
                    span { "Options" }
                }

                if hotkeys.is_empty() {
                    div { class: "hotkey-empty",
                        span { class: "solar-icon icon-keyboard" }
                        p { "No hotkeys configured." }
                    }
                }

                for hotkey in hotkeys {
                    HotkeyRow {
                        key: "{hotkey.id}",
                        hotkey: hotkey.clone(),
                        devices: devices.clone(),
                        settings,
                        onedit: move |binding: crate::HotkeyBinding| {
                            start_modal(
                                Some(binding.id.clone()),
                                Some(binding),
                                settings,
                                modal,
                                recording,
                                modifier_hold_started,
                                hold_progress,
                                pending_exiting,
                                live_modifier_shortcut,
                                recording_shortcut,
                                draft_shortcut,
                                draft_action,
                                draft_target,
                                draft_ignore_modifiers,
                            );
                        }
                    }
                }
            }

            button {
                class: "secondary add-hotkey-button",
                onclick: move |_| {
                    start_modal(
                        None,
                        None,
                        settings,
                        modal,
                        recording,
                        modifier_hold_started,
                        hold_progress,
                        pending_exiting,
                        live_modifier_shortcut,
                        recording_shortcut,
                        draft_shortcut,
                        draft_action,
                        draft_target,
                        draft_ignore_modifiers,
                    );
                },
                span { class: "solar-icon button-icon icon-record" }
                "Add hotkey"
            }
        }

        if let Some(mode) = modal_mode {
            HotkeyModal {
                mode: mode.clone(),
                devices,
                recording,
                modifier_hold_started,
                hold_progress,
                pending_exiting,
                live_modifier_shortcut,
                recording_shortcut,
                draft_shortcut,
                draft_action,
                draft_target,
                draft_ignore_modifiers,
                can_save,
                settings,
                onclose: move |_| {
                    close_modal(
                        settings,
                        modal,
                        recording,
                        modifier_hold_started,
                        hold_progress,
                        pending_exiting,
                        live_modifier_shortcut,
                        recording_shortcut,
                    );
                },
                onsave: {
                    let save_mode = mode.clone();
                    move |_| {
                    if let Some(shortcut) = draft_shortcut() {
                        let action = draft_action();
                        let target = if action.needs_target() && !draft_target().is_empty() {
                            Some(draft_target())
                        } else {
                            None
                        };
                        let mode = save_mode.clone();
                        super::super::update_settings(settings, |config| {
                            config.hotkeys_paused = false;
                            match mode {
                                ModalMode::Add => config.hotkeys.push(crate::HotkeyBinding {
                                    shortcut,
                                    action,
                                    target,
                                    ignore_modifiers: draft_ignore_modifiers(),
                                    ..crate::HotkeyBinding::default()
                                }),
                                ModalMode::Edit(id) => {
                                    if let Some(binding) = config.hotkeys.iter_mut().find(|binding| binding.id == id) {
                                        binding.shortcut = shortcut;
                                        binding.action = action;
                                        binding.target = target;
                                        binding.ignore_modifiers = draft_ignore_modifiers();
                                    }
                                }
                            }
                            sync_legacy_shortcut(config);
                        });
                        modal.set(None);
                        recording.set(false);
                        modifier_hold_started.set(None);
                        hold_progress.set(0.0);
                        pending_exiting.set(false);
                        live_modifier_shortcut.set(None);
                        recording_shortcut.set(None);
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
    settings: Signal<super::super::SettingsSnapshot>,
    mut modal: Signal<Option<ModalMode>>,
    mut recording: Signal<bool>,
    mut modifier_hold_started: Signal<Option<Instant>>,
    mut hold_progress: Signal<f64>,
    mut pending_exiting: Signal<bool>,
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_action: Signal<crate::HotkeyAction>,
    mut draft_target: Signal<String>,
    mut draft_ignore_modifiers: Signal<bool>,
) {
    if let Some(binding) = binding {
        draft_shortcut.set(Some(binding.shortcut));
        draft_action.set(binding.action);
        draft_target.set(binding.target.unwrap_or_default());
        draft_ignore_modifiers.set(binding.ignore_modifiers);
        modal.set(Some(ModalMode::Edit(binding.id)));
    } else {
        draft_shortcut.set(None);
        draft_action.set(crate::HotkeyAction::ToggleMute);
        draft_target.set(String::new());
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
    live_modifier_shortcut.set(None);
    recording_shortcut.set(None);
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
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
) {
    super::super::update_settings(settings, |config| {
        config.hotkeys_paused = false;
    });
    modal.set(None);
    recording.set(false);
    modifier_hold_started.set(None);
    hold_progress.set(0.0);
    pending_exiting.set(false);
    live_modifier_shortcut.set(None);
    recording_shortcut.set(None);
}

fn animate_pending_out(mut pending_exiting: Signal<bool>) {
    pending_exiting.set(true);
    spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        pending_exiting.set(false);
    });
}

#[component]
fn HotkeyModal(
    mode: ModalMode,
    devices: Vec<crate::MicDevice>,
    mut recording: Signal<bool>,
    mut modifier_hold_started: Signal<Option<Instant>>,
    mut hold_progress: Signal<f64>,
    mut pending_exiting: Signal<bool>,
    mut live_modifier_shortcut: Signal<Option<crate::Shortcut>>,
    mut recording_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_shortcut: Signal<Option<crate::Shortcut>>,
    mut draft_action: Signal<crate::HotkeyAction>,
    mut draft_target: Signal<String>,
    mut draft_ignore_modifiers: Signal<bool>,
    can_save: bool,
    settings: Signal<super::super::SettingsSnapshot>,
    onclose: EventHandler<()>,
    onsave: EventHandler<()>,
) -> Element {
    let title = match mode {
        ModalMode::Add => "Add hotkey",
        ModalMode::Edit(_) => "Edit hotkey",
    };
    let save_label = match mode {
        ModalMode::Add => "Add",
        ModalMode::Edit(_) => "Save",
    };
    let action = draft_action();

    rsx! {
        div {
            class: "modal-backdrop",
            onclick: move |_| onclose.call(()),
            div {
                class: "mini-modal",
                tabindex: "0",
                onclick: move |evt| evt.stop_propagation(),
                onkeydown: move |evt| {
                    if !recording() {
                        return;
                    }
                    evt.prevent_default();
                    if let Some(shortcut) = shortcut_from_keyboard_data(&evt.data()) {
                        if shortcut.vk == 0 {
                            let current = current_modifier_shortcut().unwrap_or(shortcut);
                            if live_modifier_shortcut() != Some(current) {
                                modifier_hold_started.set(Some(Instant::now()));
                                hold_progress.set(0.0);
                            }
                            live_modifier_shortcut.set(Some(current));
                        } else {
                            draft_shortcut.set(Some(shortcut));
                            recording_shortcut.set(Some(shortcut));
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
                    if !recording() {
                        return;
                    }
                    evt.prevent_default();
                    let current = current_modifier_shortcut();
                    live_modifier_shortcut.set(current);
                    if current.is_none() {
                        if modifier_hold_started().is_some() {
                            animate_pending_out(pending_exiting);
                        }
                        modifier_hold_started.set(None);
                        hold_progress.set(0.0);
                    }
                },
                div { class: "mini-modal-head",
                    h2 { "{title}" }
                    button {
                        class: "icon-button",
                        title: "Close",
                        onclick: move |_| onclose.call(()),
                        span { class: "solar-icon icon-close" }
                    }
                }

                div { class: "hotkey-modal-grid",
                    div { class: "field-group modal-field",
                        label { "Shortcut" }
                            div { class: "shortcut-record-stack",
                                div { class: "shortcut-record-row",
                                    KeyDisplay {
                                        shortcut: if recording() {
                                            live_modifier_shortcut().or_else(|| recording_shortcut())
                                        } else {
                                            draft_shortcut()
                                        },
                                        recording: recording(),
                                        boxed: true,
                                        modifier_hold_active: modifier_hold_started().is_some(),
                                        pending_exiting: pending_exiting(),
                                        hold_progress: hold_progress()
                                    }
                                    button {
                                        class: "secondary",
                                        onclick: move |_| {
                                            let next = !recording();
                                            recording.set(next);
                                            recording_shortcut.set(None);
                                            if modifier_hold_started().is_some() {
                                                animate_pending_out(pending_exiting);
                                            }
                                            modifier_hold_started.set(None);
                                            hold_progress.set(0.0);
                                            live_modifier_shortcut.set(None);
                                        },
                                        span { class: "solar-icon button-icon icon-record" }
                                        if recording() {
                                            "Cancel"
                                        } else {
                                            "Record"
                                        }
                                    }
                                }
                                if recording() {
                                    p { class: "shortcut-record-hint", "Hold to bind only modifier keys" }
                                }
                            }

                    }

                    Checkbox {
                        class: "modal-check".to_string(),
                        checked: draft_ignore_modifiers(),
                        label: "Ignore modifiers".to_string(),
                        onchange: move |checked: bool| draft_ignore_modifiers.set(checked)
                    }

                    div { class: "field-group modal-field",
                        label { "Action" }
                        Select {
                            value: action.id().to_string(),
                            options: action_options(),
                            onchange: move |value: String| {
                                let action = crate::HotkeyAction::from_id(&value);
                                draft_action.set(action);
                                if !action.needs_target() {
                                    draft_target.set(String::new());
                                }
                            }
                        }
                    }

                    if action.needs_target() {
                        TargetSelect {
                            value: draft_target(),
                            devices,
                            onchange: move |value: String| draft_target.set(value)
                        }
                    }
                }

                div { class: "mini-modal-actions",
                    button {
                        class: "secondary",
                        onclick: move |_| onclose.call(()),
                        "Cancel"
                    }
                    button {
                        class: "save",
                        disabled: !can_save,
                        onclick: move |_| onsave.call(()),
                        "{save_label}"
                    }
                }
            }
        }
    }
}

#[component]
fn KeyDisplay(
    shortcut: Option<crate::Shortcut>,
    recording: bool,
    boxed: bool,
    modifier_hold_active: bool,
    pending_exiting: bool,
    hold_progress: f64,
) -> Element {
    let parts = shortcut
        .map(|shortcut| shortcut.parts())
        .unwrap_or_default();
    let progress = format!("{:.0}%", hold_progress * 100.0);
    let pending_offset = format!("{}px", pending_offset_px(&parts));

    rsx! {
        div {
            class: display_class(boxed, recording),
            style: "--hold-progress: {progress}; --pending-offset: {pending_offset};",
            for part in parts {
                span { class: "keycap", "{part}" }
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
    settings: Signal<super::super::SettingsSnapshot>,
    onedit: EventHandler<crate::HotkeyBinding>,
) -> Element {
    let id = hotkey.id.clone();
    let action = hotkey.action;
    let target_label = if action.needs_target() {
        target_label(hotkey.target.as_deref(), &devices)
    } else {
        "No target needed".to_string()
    };
    let modifier_label = if hotkey.ignore_modifiers {
        "Ignores modifiers"
    } else {
        "Exact modifiers"
    };

    rsx! {
        div { class: "hotkey-entry",
            div { class: "hotkey-main-row",
                div { class: "hotkey-action-cell",
                    h3 { "{action.label()}" }
                }
                KeyDisplay {
                    shortcut: Some(hotkey.shortcut),
                    recording: false,
                    boxed: false,
                    modifier_hold_active: false,
                    pending_exiting: false,
                    hold_progress: 0.0
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

            div { class: "hotkey-secondary-row",
                span { "{action.label()}" }
                span { "{target_label}" }
                span { class: "hotkey-modifier-mode", "{modifier_label}" }
            }
        }
    }
}

fn pending_offset_px(parts: &[String]) -> usize {
    if parts.is_empty() {
        return 0;
    }

    let key_widths: usize = parts
        .iter()
        .map(|part| {
            let text_width = part.chars().count() * 7 + 18;
            text_width.clamp(28, 92)
        })
        .sum();
    key_widths + (parts.len() - 1) * 6 + 8
}

fn display_class(boxed: bool, recording: bool) -> &'static str {
    match (boxed, recording) {
        (true, true) => "shortcut-display fake-input recording",
        (true, false) => "shortcut-display fake-input",
        (false, true) => "shortcut-display recording",
        (false, false) => "shortcut-display",
    }
}

#[component]
fn TargetSelect(
    value: String,
    devices: Vec<crate::MicDevice>,
    onchange: EventHandler<String>,
) -> Element {
    let options = std::iter::once(
        SelectOption::new("", "Selected microphone")
            .detail("Follow whatever microphone is currently active")
            .icon("icon-mic"),
    )
    .chain(devices.into_iter().map(|device| {
        let option = SelectOption::new(device.id, device.name).icon("icon-mic");
        if device.is_default {
            option.detail("Windows default")
        } else {
            option
        }
    }))
    .collect::<Vec<_>>();

    rsx! {
        div { class: "field-group modal-field target-field",
            label { "Target" }
            Select {
                value: value.clone(),
                options,
                onchange: move |value: String| onchange.call(value)
            }
        }
    }
}

fn action_options() -> Vec<SelectOption> {
    crate::HotkeyAction::ALL
        .iter()
        .map(|action| {
            let option = SelectOption::new(action.id(), action.label());
            match action {
                crate::HotkeyAction::ToggleMute => option
                    .detail("Flip the mute state for the chosen microphone")
                    .icon("icon-mic"),
                crate::HotkeyAction::Mute => option
                    .detail("Force the microphone into the muted state")
                    .icon("icon-mic"),
                crate::HotkeyAction::Unmute => option
                    .detail("Force the microphone into the live state")
                    .icon("icon-mic"),
                crate::HotkeyAction::OpenSettings => option
                    .detail("Bring the settings window to the front")
                    .icon("icon-settings"),
            }
        })
        .collect()
}

fn target_label(target: Option<&str>, devices: &[crate::MicDevice]) -> String {
    target
        .filter(|target| !target.is_empty())
        .and_then(|target| devices.iter().find(|device| device.id == target))
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Selected microphone".to_string())
}

fn sync_legacy_shortcut(config: &mut crate::Config) {
    if let Some(shortcut) = config
        .hotkeys
        .iter()
        .find(|binding| binding.action == crate::HotkeyAction::ToggleMute)
        .map(|binding| binding.shortcut)
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
        })
    } else {
        None
    }
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
