fn exit_all_processes() {
    close_settings_window();
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = normalized_keyboard_vk(kb.vkCode, kb.scanCode, kb.flags.0);
    let is_down = event == WM_KEYDOWN || event == 0x0104;
    let is_up = event == WM_KEYUP || event == 0x0105;

    if {
        let state = STATE.lock().unwrap();
        hotkeys_blocked(&state)
    } {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if is_down {
        let mut actions = Vec::new();
        let mut suppress_key = false;
        let modifiers;
        {
            let mut state = STATE.lock().unwrap();
            update_modifier_state(&mut state.modifiers, vk, true);
            modifiers = state.modifiers;
        }
        record_mouse_shortcut_with_modifiers(modifiers);
        {
            let mut state = STATE.lock().unwrap();
            let matching_hotkeys: Vec<HotkeyBinding> = state
                .hotkeys
                .iter()
                .filter(|hotkey| hotkey.gamepad.is_none())
                .filter(|hotkey| {
                    if hotkey.shortcut.vk == 0 && !is_modifier(vk) {
                        return false;
                    }
                    hotkey.shortcut.is_pressed(
                        vk,
                        hotkey.ignore_modifiers,
                        &modifiers,
                        &state.mouse_buttons_down,
                    )
                })
                .cloned()
                .collect();
            let exact_keys: HashSet<u32> = matching_hotkeys
                .iter()
                .filter(|hotkey| !hotkey.ignore_modifiers)
                .map(|hotkey| hotkey.shortcut.vk)
                .collect();

            for hotkey in matching_hotkeys {
                if hotkey.shortcut.vk == 0 && !is_modifier(vk) {
                    continue;
                }
                if hotkey.ignore_modifiers && exact_keys.contains(&hotkey.shortcut.vk) {
                    continue;
                }
                if shortcut_is_alt_space(&hotkey.shortcut) {
                    suppress_key = true;
                }
                if !state.hotkeys_down.contains(&hotkey.id) {
                    state.hotkeys_down.insert(hotkey.id.clone());
                    actions.push(hotkey_action_request(&hotkey));
                }
            }
        }
        for action in actions {
            run_hotkey_action(action);
        }
        if suppress_key {
            return LRESULT(1);
        }
    }

    if is_up {
        let mut actions = Vec::new();
        let mut suppress_key = false;
        let mut state = STATE.lock().unwrap();
        update_modifier_state(&mut state.modifiers, vk, false);
        let modifiers = state.modifiers;
        let released: Vec<HotkeyBinding> = state
            .hotkeys
            .iter()
            .filter(|hotkey| hotkey.gamepad.is_none())
            .filter(|hotkey| {
                hotkey.shortcut.vk == vk
                    || (hotkey.shortcut.vk == 0
                        && is_modifier(vk)
                        && !hotkey.shortcut.is_pressed(
                            vk,
                            hotkey.ignore_modifiers,
                            &modifiers,
                            &state.mouse_buttons_down,
                        ))
            })
            .cloned()
            .collect();
        for hotkey in released {
            if shortcut_is_alt_space(&hotkey.shortcut) {
                suppress_key = true;
            }
            state.hotkeys_down.remove(&hotkey.id);
            if hotkey.action.is_hold() {
                actions.push(HotkeyRequest::ReleaseHold {
                    id: hotkey.id.clone(),
                });
            }
        }
        if state.shortcut.vk == vk {
            state.shortcut_down = false;
        }
        drop(state);

        for action in actions {
            run_hotkey_action(action);
        }
        if suppress_key {
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn normalized_keyboard_vk(vk: u32, scan_code: u32, flags: u32) -> u32 {
    if flags & LLKHF_EXTENDED != 0 {
        return vk;
    }

    match scan_code {
        0x52 => VK_NUMPAD0,
        0x4F => VK_NUMPAD0 + 1,
        0x50 => VK_NUMPAD0 + 2,
        0x51 => VK_NUMPAD0 + 3,
        0x4B => VK_NUMPAD0 + 4,
        0x4C => VK_NUMPAD0 + 5,
        0x4D => VK_NUMPAD0 + 6,
        0x47 => VK_NUMPAD0 + 7,
        0x48 => VK_NUMPAD0 + 8,
        0x49 => VK_NUMPAD0 + 9,
        0x53 => 0x6E,
        _ => vk,
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
    let Some(button) = mouse_button_from_event(event, mouse.mouseData) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    let down = mouse_button_event_is_down(event);

    if SETTINGS_HOTKEY_RECORDING.load(Ordering::Relaxed) {
        if down {
            let mut held = SETTINGS_MOUSE_HELD.lock().unwrap();
            if !held.contains(&button) {
                held.push(button);
            }
            drop(held);
            record_mouse_shortcut_with_modifiers(current_modifier_state());
        } else {
            SETTINGS_MOUSE_HELD
                .lock()
                .unwrap()
                .retain(|held| *held != button);
        }
    }

    if !MOUSE_HOTKEYS_ENABLED.load(Ordering::Relaxed) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let mut actions = Vec::new();
    {
        let mut state = STATE.lock().unwrap();
        if hotkeys_blocked(&state) {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        state.modifiers = current_modifier_state();
        if down {
            state.mouse_buttons_down.insert(button);
            actions.extend(mouse_press_actions(&mut state, button));
        } else {
            state.mouse_buttons_down.remove(&button);
            actions.extend(mouse_release_actions(&mut state, button));
        }
    }

    for action in actions {
        run_hotkey_action(action);
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn record_mouse_shortcut_with_modifiers(modifiers: ModifierState) {
    if !SETTINGS_HOTKEY_RECORDING.load(Ordering::Relaxed)
        || !(modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.win)
    {
        return;
    }

    let mut mouse_buttons = SETTINGS_MOUSE_HELD.lock().unwrap().clone();
    if mouse_buttons.is_empty() {
        return;
    }

    mouse_buttons.truncate(2);
    *SETTINGS_MOUSE_PRESSED_SHORTCUT.lock().unwrap() = Some(Shortcut {
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        shift: modifiers.shift,
        win: modifiers.win,
        vk: 0,
        mouse_buttons,
    });
}

fn mouse_press_actions(state: &mut AppState, button: u32) -> Vec<HotkeyRequest> {
    let modifiers = state.modifiers;
    let mut matches: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| hotkey.gamepad.is_none())
        .filter(|hotkey| {
            hotkey.shortcut.is_pressed(
                button,
                hotkey.ignore_modifiers,
                &modifiers,
                &state.mouse_buttons_down,
            )
        })
        .cloned()
        .collect();

    let has_combo_match = matches
        .iter()
        .any(|hotkey| hotkey.shortcut.mouse_buttons.len() > 1);
    matches.retain(|hotkey| !has_combo_match || hotkey.shortcut.mouse_buttons.len() > 1);

    matches
        .into_iter()
        .filter_map(|hotkey| {
            if state.hotkeys_down.insert(hotkey.id.clone()) {
                Some(hotkey_action_request(&hotkey))
            } else {
                None
            }
        })
        .collect()
}

fn mouse_release_actions(state: &mut AppState, button: u32) -> Vec<HotkeyRequest> {
    let released: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| {
            state.hotkeys_down.contains(&hotkey.id)
                && hotkey.shortcut.mouse_buttons.contains(&button)
        })
        .cloned()
        .collect();

    let mut actions = Vec::new();
    for hotkey in released {
        state.hotkeys_down.remove(&hotkey.id);
        if hotkey.action.is_hold() {
            actions.push(HotkeyRequest::ReleaseHold {
                id: hotkey.id.clone(),
            });
        }
    }
    actions
}

enum HotkeyRequest {
    ToggleMute {
        target: Option<String>,
    },
    SetMute {
        target: Option<String>,
        muted: bool,
    },
    StartHold {
        id: String,
        target: Option<String>,
        muted: bool,
    },
    StartHoldToggle {
        id: String,
        target: Option<String>,
    },
    ReleaseHold {
        id: String,
    },
    SetDefaultInput {
        target: Option<String>,
    },
    SetDefaultOutput {
        target: Option<String>,
    },
    ToggleDefaultInput {
        target_1: Option<String>,
        target_2: Option<String>,
    },
    ToggleDefaultOutput {
        target_1: Option<String>,
        target_2: Option<String>,
    },
    SetVolume {
        target: Option<String>,
    },
    ChangeVolume {
        target: Option<String>,
        direction: i32,
    },
    OpenSettings,
}

fn hotkey_action_request(hotkey: &HotkeyBinding) -> HotkeyRequest {
    match hotkey.action {
        HotkeyAction::ToggleMute => HotkeyRequest::ToggleMute {
            target: hotkey.target.clone(),
        },
        HotkeyAction::Mute => HotkeyRequest::SetMute {
            target: hotkey.target.clone(),
            muted: true,
        },
        HotkeyAction::Unmute => HotkeyRequest::SetMute {
            target: hotkey.target.clone(),
            muted: false,
        },
        HotkeyAction::HoldToToggle => HotkeyRequest::StartHoldToggle {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
        },
        HotkeyAction::HoldToMute => HotkeyRequest::StartHold {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
            muted: true,
        },
        HotkeyAction::HoldToUnmute => HotkeyRequest::StartHold {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
            muted: false,
        },
        HotkeyAction::SetDefaultInputDevice => HotkeyRequest::SetDefaultInput {
            target: hotkey.target.clone(),
        },
        HotkeyAction::SetDefaultOutputDevice => HotkeyRequest::SetDefaultOutput {
            target: hotkey.target.clone(),
        },
        HotkeyAction::ToggleDefaultInputDevice => HotkeyRequest::ToggleDefaultInput {
            target_1: hotkey.target.clone(),
            target_2: hotkey.target_2.clone(),
        },
        HotkeyAction::ToggleDefaultOutputDevice => HotkeyRequest::ToggleDefaultOutput {
            target_1: hotkey.target.clone(),
            target_2: hotkey.target_2.clone(),
        },
        HotkeyAction::SetVolume => HotkeyRequest::SetVolume {
            target: hotkey.target.clone(),
        },
        HotkeyAction::IncreaseVolume => HotkeyRequest::ChangeVolume {
            target: hotkey.target.clone(),
            direction: 1,
        },
        HotkeyAction::DecreaseVolume => HotkeyRequest::ChangeVolume {
            target: hotkey.target.clone(),
            direction: -1,
        },
        HotkeyAction::OpenSettings => HotkeyRequest::OpenSettings,
    }
}

pub(crate) fn run_overlay_action(binding: OverlayActionBinding) {
    let Some(action) = binding.action else {
        return;
    };
    let request = match action {
        HotkeyAction::HoldToToggle => HotkeyRequest::ToggleMute {
            target: binding.target.clone(),
        },
        HotkeyAction::HoldToMute => HotkeyRequest::SetMute {
            target: binding.target.clone(),
            muted: true,
        },
        HotkeyAction::HoldToUnmute => HotkeyRequest::SetMute {
            target: binding.target.clone(),
            muted: false,
        },
        _ => hotkey_action_request(&HotkeyBinding {
            id: "overlay-action".to_string(),
            action,
            shortcut: Shortcut::default(),
            gamepad: None,
            ignore_modifiers: false,
            target: binding.target,
            target_name: None,
            target_2: binding.target_2,
            target_2_name: None,
        }),
    };
    run_hotkey_action(request);
}

fn run_hotkey_action(action: HotkeyRequest) {
    match action {
        HotkeyRequest::ToggleMute { target } => toggle_mute_target(target.as_deref()),
        HotkeyRequest::SetMute { target, muted } => set_mute_target(target.as_deref(), muted),
        HotkeyRequest::StartHold { id, target, muted } => {
            start_hold_hotkey(&id, target, muted);
        }
        HotkeyRequest::StartHoldToggle { id, target } => start_hold_toggle_hotkey(&id, target),
        HotkeyRequest::ReleaseHold { id } => release_hold_hotkey(&id),
        HotkeyRequest::SetDefaultInput { target } => {
            if let Some(device_id) = target {
                if let Err(err) = set_default_capture_device(&device_id) {
                    eprintln!("failed to set default input device from hotkey: {err:?}");
                }
            }
        }
        HotkeyRequest::SetDefaultOutput { target } => {
            if let Some(device_id) = target {
                if let Err(err) = set_default_render_device(&device_id) {
                    eprintln!("failed to set default output device from hotkey: {err:?}");
                }
            }
        }
        HotkeyRequest::ToggleDefaultInput { target_1, target_2 } => {
            if let Err(err) = toggle_default_audio_device(eCapture, target_1, target_2) {
                eprintln!("failed to toggle default input device from hotkey: {err:?}");
            }
        }
        HotkeyRequest::ToggleDefaultOutput { target_1, target_2 } => {
            if let Err(err) = toggle_default_audio_device(eRender, target_1, target_2) {
                eprintln!("failed to toggle default output device from hotkey: {err:?}");
            }
        }
        HotkeyRequest::SetVolume { target } => {
            if let Err(err) = set_output_volume_from_hotkey(target.as_deref()) {
                eprintln!("failed to set output volume from hotkey: {err:?}");
            }
        }
        HotkeyRequest::ChangeVolume { target, direction } => {
            if let Err(err) = change_output_volume_from_hotkey(target.as_deref(), direction) {
                eprintln!("failed to change output volume from hotkey: {err:?}");
            }
        }
        HotkeyRequest::OpenSettings => open_settings_window(),
    }
}

fn toggle_mute() {
    toggle_mute_target(None);
}

fn toggle_mute_target(device_id: Option<&str>) {
    match set_mute_to_inverse(device_id) {
        Ok(target_muted) => {
            play_mute_sound(target_muted);
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, true);
        }
        Err(err) => eprintln!("failed to toggle microphone mute: {err:?}"),
    }
}

fn set_mute_target(device_id: Option<&str>, muted: bool) {
    match set_mute(device_id, muted) {
        Ok(target_muted) => {
            play_mute_sound(target_muted);
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, true);
        }
        Err(err) => eprintln!("failed to set microphone mute: {err:?}"),
    }
}

fn apply_startup_auto_mute() {
    let settings = STATE.lock().unwrap().auto_mute.clone();
    if !settings.mute_on_startup {
        return;
    }

    if let Err(err) = apply_auto_mute(settings.play_sounds, false) {
        eprintln!("failed to apply startup auto-mute: {err:?}");
    }
}

fn start_hold_hotkey(id: &str, target: Option<String>, muted: bool) {
    let previous_muted = match target_mute_state(target.as_deref()) {
        Ok(previous_muted) => previous_muted,
        Err(err) => {
            eprintln!("failed to read hold hotkey state: {err:?}");
            return;
        }
    };

    match set_mute(target.as_deref(), muted) {
        Ok(target_muted) => {
            {
                let mut state = STATE.lock().unwrap();
                state.active_hold_hotkeys.insert(
                    id.to_string(),
                    ActiveHoldHotkey {
                        target,
                        previous_muted,
                    },
                );
            }

            let changed = previous_muted != target_muted;
            if changed {
                play_hold_to_mute_sound(target_muted);
            }
            let show_overlay = STATE.lock().unwrap().hold_to_mute.show_overlay;
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, changed && show_overlay);
        }
        Err(err) => eprintln!("failed to apply hold hotkey mute state: {err:?}"),
    }
}

fn start_hold_toggle_hotkey(id: &str, target: Option<String>) {
    let previous_muted = match target_mute_state(target.as_deref()) {
        Ok(previous_muted) => previous_muted,
        Err(err) => {
            eprintln!("failed to read hold toggle hotkey state: {err:?}");
            return;
        }
    };
    start_hold_hotkey(id, target, !previous_muted);
}

fn release_hold_hotkey(id: &str) {
    let active_hold = {
        let mut state = STATE.lock().unwrap();
        state.active_hold_hotkeys.remove(id)
    };
    let Some(active_hold) = active_hold else {
        return;
    };

    let current_muted =
        target_mute_state(active_hold.target.as_deref()).unwrap_or(active_hold.previous_muted);
    match set_mute(active_hold.target.as_deref(), active_hold.previous_muted) {
        Ok(target_muted) => {
            let changed = current_muted != target_muted;
            if changed {
                play_hold_to_mute_sound(target_muted);
            }
            let show_overlay = STATE.lock().unwrap().hold_to_mute.show_overlay;
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, changed && show_overlay);
        }
        Err(err) => eprintln!("failed to restore hold hotkey mute state: {err:?}"),
    }
}

fn set_global_mute_state(muted: bool, trigger_overlay: bool) {
    let changed = {
        let mut state = STATE.lock().unwrap();
        let changed = state.muted != muted;
        state.muted = muted;
        changed
    };

    if !changed && !trigger_overlay {
        return;
    }

    refresh_tray_icon();
    let overlay = STATE.lock().unwrap().overlay.clone();
    if trigger_overlay && overlay.enabled && overlay.visibility == "AfterToggle" {
        let millis = (overlay.duration_secs.clamp(0.1, 10.0) * 1000.0) as u32;
        show_overlay_temporarily(millis);
    } else {
        apply_overlay_visibility();
    }
}

pub fn send_test_push_notification() {
    register_notification_integration();

    let mut toast = winrt_toast::Toast::new();
    apply_toast_logo(&mut toast);
    toast
        .text1("silence! push")
        .text2("System notification pipeline is alive.")
        .duration(winrt_toast::ToastDuration::Short)
        .launch("open-settings")
        .action(
            winrt_toast::Action::new("Toggle mute", "silence://toggle-mute", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        )
        .action(
            winrt_toast::Action::new("Settings", "silence://open-settings", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        );

    let manager = winrt_toast::ToastManager::new(APP_USER_MODEL_ID);
    let activated = Some(Box::new(move |arguments: winrt_toast::Result<String>| {
        match arguments
            .ok()
            .and_then(|arguments| notification_action_from_text(&arguments))
        {
            Some(action) => handle_notification_action(action),
            None => open_settings_window(),
        }
    })
        as Box<dyn FnMut(winrt_toast::Result<String>) + Send + 'static>);

    if let Err(err) = manager.show_with_callbacks(&toast, activated, None, None) {
        eprintln!("failed to show push notification: {err:?}");
    }
}

pub async fn check_for_update() -> Result<updater::UpdateCheck> {
    updater::check_for_update().await
}

pub async fn download_and_install_update(
    update: updater::UpdateInfo,
    on_progress: impl FnMut(f32),
) -> Result<()> {
    let installer = updater::download_update(&update, on_progress).await?;
    write_last_update_notification(&update)?;
    updater::install_update(installer)?;
    request_exit_all_processes();
    std::process::exit(0);
}
