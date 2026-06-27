fn start_xinput_monitor(enable_hotkeys: bool) {
    if XINPUT_MONITOR_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    thread::spawn(move || {
        let mut previous_buttons = [0_u16; 4];
        let mut previous_left_trigger = [false; 4];
        let mut previous_right_trigger = [false; 4];
        let mut previous_connected = [false; 4];
        loop {
            if !gamepad_monitoring_needed(enable_hotkeys) {
                reset_gamepad_input_state(enable_hotkeys);
                thread::sleep(Duration::from_millis(GAMEPAD_INACTIVE_POLL_MS));
                continue;
            }

            let mut any_connected = false;
            for user_index in 0..4 {
                let mut state = XINPUT_STATE::default();
                let result = unsafe { XInputGetState(user_index, &mut state) };
                let connected = result == ERROR_SUCCESS.0;
                any_connected |= connected;
                if connected != previous_connected[user_index as usize] {
                    previous_connected[user_index as usize] = connected;
                    eprintln!(
                        "xinput controller {} {}",
                        user_index,
                        if connected {
                            "connected"
                        } else {
                            "disconnected"
                        }
                    );
                }
                if !connected {
                    previous_buttons[user_index as usize] = 0;
                    previous_left_trigger[user_index as usize] = false;
                    previous_right_trigger[user_index as usize] = false;
                    continue;
                }

                let buttons = state.Gamepad.wButtons.0;
                let changed = buttons ^ previous_buttons[user_index as usize];
                if changed != 0 {
                    for (mask, input) in xinput_button_inputs() {
                        if changed & mask != 0 {
                            let down = buttons & mask != 0;
                            handle_gamepad_input_change(input, down, enable_hotkeys);
                        }
                    }
                    previous_buttons[user_index as usize] = buttons;
                }

                let left_trigger = update_xinput_trigger_state(
                    previous_left_trigger[user_index as usize],
                    state.Gamepad.bLeftTrigger,
                );
                if left_trigger != previous_left_trigger[user_index as usize] {
                    previous_left_trigger[user_index as usize] = left_trigger;
                    handle_gamepad_input_change(
                        GamepadInput::Button {
                            button: GamepadButton::LeftTrigger2,
                        },
                        left_trigger,
                        enable_hotkeys,
                    );
                }

                let right_trigger = update_xinput_trigger_state(
                    previous_right_trigger[user_index as usize],
                    state.Gamepad.bRightTrigger,
                );
                if right_trigger != previous_right_trigger[user_index as usize] {
                    previous_right_trigger[user_index as usize] = right_trigger;
                    handle_gamepad_input_change(
                        GamepadInput::Button {
                            button: GamepadButton::RightTrigger2,
                        },
                        right_trigger,
                        enable_hotkeys,
                    );
                }
            }
            let poll_ms = if any_connected {
                XINPUT_CONNECTED_POLL_MS
            } else {
                XINPUT_IDLE_POLL_MS
            };
            thread::sleep(Duration::from_millis(poll_ms));
        }
    });
}

fn update_xinput_trigger_state(was_down: bool, value: u8) -> bool {
    if was_down {
        value > XINPUT_TRIGGER_RELEASE_THRESHOLD
    } else {
        value >= XINPUT_TRIGGER_PRESS_THRESHOLD
    }
}

fn xinput_button_inputs() -> [(u16, GamepadInput); 14] {
    [
        (
            XINPUT_GAMEPAD_A.0,
            GamepadInput::Button {
                button: GamepadButton::South,
            },
        ),
        (
            XINPUT_GAMEPAD_B.0,
            GamepadInput::Button {
                button: GamepadButton::East,
            },
        ),
        (
            XINPUT_GAMEPAD_X.0,
            GamepadInput::Button {
                button: GamepadButton::West,
            },
        ),
        (
            XINPUT_GAMEPAD_Y.0,
            GamepadInput::Button {
                button: GamepadButton::North,
            },
        ),
        (
            XINPUT_GAMEPAD_LEFT_SHOULDER.0,
            GamepadInput::Button {
                button: GamepadButton::LeftTrigger,
            },
        ),
        (
            XINPUT_GAMEPAD_RIGHT_SHOULDER.0,
            GamepadInput::Button {
                button: GamepadButton::RightTrigger,
            },
        ),
        (
            XINPUT_GAMEPAD_LEFT_THUMB.0,
            GamepadInput::Button {
                button: GamepadButton::LeftThumb,
            },
        ),
        (
            XINPUT_GAMEPAD_RIGHT_THUMB.0,
            GamepadInput::Button {
                button: GamepadButton::RightThumb,
            },
        ),
        (
            XINPUT_GAMEPAD_BACK.0,
            GamepadInput::Button {
                button: GamepadButton::Select,
            },
        ),
        (
            XINPUT_GAMEPAD_START.0,
            GamepadInput::Button {
                button: GamepadButton::Start,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_UP.0,
            GamepadInput::Button {
                button: GamepadButton::DPadUp,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_DOWN.0,
            GamepadInput::Button {
                button: GamepadButton::DPadDown,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_LEFT.0,
            GamepadInput::Button {
                button: GamepadButton::DPadLeft,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_RIGHT.0,
            GamepadInput::Button {
                button: GamepadButton::DPadRight,
            },
        ),
    ]
}

fn run_background_app() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }
    let instance = unsafe { GetModuleHandleW(None)? };
    register_class(instance.into())?;
    let hwnd = create_message_window(instance.into())?;
    let muted = current_mute_state().unwrap_or(false);
    let mic_in_use = current_mic_in_use().unwrap_or(false);
    let overlay_config = STATE.lock().unwrap().overlay.clone();
    {
        let mut state = STATE.lock().unwrap();
        state.hwnd = hwnd;
        state.muted = muted;
        state.mic_in_use = mic_in_use;
    }
    register_notification_integration();
    updater::cleanup_downloads_after_startup();
    maybe_show_updated_notification();
    if !launched_from_installer() && !development_tools_enabled() {
        start_update_check();
    }
    let sound_settings = STATE.lock().unwrap().sound_settings.clone();
    native_overlay::init(instance.into(), muted, &overlay_config)?;
    apply_overlay_visibility();
    prime_sound_assets(&sound_settings);

    install_keyboard_hook(instance.into())?;
    install_mouse_hook(instance.into())?;
    if gamepad_monitoring_needed(true) {
        ensure_gamepad_monitors(true);
    }
    ensure_audio_notification_registration();
    add_tray_icon(hwnd)?;
    if !STATE.lock().unwrap().welcome_completed {
        open_settings_window();
    }
    apply_startup_auto_mute();
    apply_pending_notification_action();
    unsafe {
        let _ = SetTimer(hwnd, ID_STATE_TIMER, 250, None);
    }

    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    cleanup();
    Ok(())
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    unsafe {
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance,
            lpszClassName: w!("SilenceV2Hidden"),
            lpfnWndProc: Some(main_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&class);
    }
    Ok(())
}

fn create_message_window(instance: HINSTANCE) -> Result<HWND> {
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SilenceV2Hidden"),
            w!("SilenceV2Hidden"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            instance,
            None,
        )
    }?;
    if hwnd.0.is_null() {
        anyhow::bail!("failed to create hidden window");
    }
    Ok(hwnd)
}

fn install_keyboard_hook(instance: HINSTANCE) -> Result<()> {
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0) }
        .context("install low-level keyboard hook")?;
    STATE.lock().unwrap().hook = hook;
    Ok(())
}

fn install_mouse_hook(instance: HINSTANCE) -> Result<()> {
    let hook = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), instance, 0) }
        .context("install low-level mouse hook")?;
    STATE.lock().unwrap().mouse_hook = hook;
    Ok(())
}

fn start_gamepad_monitor(enable_hotkeys: bool) {
    if GILRS_MONITOR_STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    thread::spawn(move || {
        let Ok(mut gilrs) = Gilrs::new() else {
            eprintln!("failed to initialize gamepad input");
            return;
        };
        let mut connected_gamepads = gilrs.gamepads().count();

        loop {
            if !gamepad_monitoring_needed(enable_hotkeys) {
                reset_gamepad_input_state(enable_hotkeys);
                thread::sleep(Duration::from_millis(GAMEPAD_INACTIVE_POLL_MS));
                continue;
            }

            let timeout = if connected_gamepads == 0 {
                Duration::from_millis(GAMEPAD_INACTIVE_POLL_MS)
            } else {
                Duration::from_millis(GILRS_ACTIVE_POLL_MS)
            };

            while let Some(event) = gilrs.next_event_blocking(Some(timeout)) {
                match event.event {
                    EventType::Connected => {
                        connected_gamepads = gilrs.gamepads().count();
                        eprintln!("gamepad connected; connected gamepads: {connected_gamepads}");
                    }
                    EventType::Disconnected => {
                        connected_gamepads = gilrs.gamepads().count();
                        reset_gamepad_input_state(enable_hotkeys);
                        eprintln!("gamepad disconnected; connected gamepads: {connected_gamepads}");
                    }
                    _ => {}
                }
                handle_gamepad_event(event.event, enable_hotkeys);

                if connected_gamepads == 0 {
                    break;
                }
            }
        }
    });
}

fn ensure_gamepad_monitors(enable_hotkeys: bool) {
    start_gamepad_monitor(enable_hotkeys);
    start_xinput_monitor(enable_hotkeys);
}

fn gamepad_monitoring_needed(enable_hotkeys: bool) -> bool {
    if SETTINGS_GAMEPAD_RECORDING.load(Ordering::Relaxed) {
        return true;
    }

    enable_hotkeys
        && STATE
            .lock()
            .unwrap()
            .hotkeys
            .iter()
            .any(|hotkey| hotkey.gamepad.is_some())
}

fn reset_gamepad_input_state(enable_hotkeys: bool) {
    SETTINGS_GAMEPAD_HELD.lock().unwrap().clear();
    if enable_hotkeys {
        let mut state = STATE.lock().unwrap();
        state.gamepad_inputs_down.clear();
        state.gamepad_hotkeys_down.clear();
    }
}

fn handle_gamepad_event(event: EventType, enable_hotkeys: bool) {
    match event {
        EventType::ButtonPressed(button, _) => {
            if let Some(input) =
                GamepadButton::from_gilrs(button).map(|button| GamepadInput::Button { button })
            {
                handle_gamepad_input_change(input, true, enable_hotkeys);
            }
        }
        EventType::ButtonReleased(button, _) => {
            if let Some(input) =
                GamepadButton::from_gilrs(button).map(|button| GamepadInput::Button { button })
            {
                handle_gamepad_input_change(input, false, enable_hotkeys);
            }
        }
        _ => {}
    }
}

fn handle_gamepad_input_change(input: GamepadInput, down: bool, enable_hotkeys: bool) {
    if SETTINGS_GAMEPAD_RECORDING.load(Ordering::Relaxed) {
        let mut held = SETTINGS_GAMEPAD_HELD.lock().unwrap();
        if down {
            held.insert(input);
        } else {
            held.remove(&input);
        }
    }

    if !enable_hotkeys {
        return;
    }

    let mut actions = Vec::new();
    {
        let mut state = STATE.lock().unwrap();
        if hotkeys_blocked(&state) {
            return;
        }

        if down {
            state.gamepad_inputs_down.insert(input);
            actions.extend(gamepad_press_actions(&mut state, input));
        } else {
            state.gamepad_inputs_down.remove(&input);
            actions.extend(gamepad_release_actions(&mut state, input));
        }
    }

    for action in actions {
        run_hotkey_action(action);
    }
}

fn gamepad_press_actions(state: &mut AppState, input: GamepadInput) -> Vec<HotkeyRequest> {
    let mut matches: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| {
            hotkey.gamepad.as_ref().is_some_and(|shortcut| {
                gamepad_shortcut_matches(shortcut, &state.gamepad_inputs_down)
            })
        })
        .cloned()
        .collect();

    let has_combo_match = matches.iter().any(|hotkey| {
        hotkey
            .gamepad
            .as_ref()
            .is_some_and(|shortcut| shortcut.inputs.len() > 1 && shortcut.inputs.contains(&input))
    });

    matches.retain(|hotkey| {
        hotkey.gamepad.as_ref().is_some_and(|shortcut| {
            !has_combo_match || shortcut.inputs.len() > 1 || !shortcut.inputs.contains(&input)
        })
    });

    matches
        .into_iter()
        .filter_map(|hotkey| {
            if state.gamepad_hotkeys_down.insert(hotkey.id.clone()) {
                Some(hotkey_action_request(&hotkey))
            } else {
                None
            }
        })
        .collect()
}

fn gamepad_release_actions(state: &mut AppState, input: GamepadInput) -> Vec<HotkeyRequest> {
    let released: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| {
            state.gamepad_hotkeys_down.contains(&hotkey.id)
                && hotkey
                    .gamepad
                    .as_ref()
                    .is_some_and(|shortcut| shortcut.inputs.contains(&input))
        })
        .cloned()
        .collect();

    let mut actions = Vec::new();
    for hotkey in released {
        state.gamepad_hotkeys_down.remove(&hotkey.id);
        if hotkey.action.is_hold() {
            actions.push(HotkeyRequest::ReleaseHold {
                id: hotkey.id.clone(),
            });
        }
    }
    actions
}

fn gamepad_shortcut_matches(shortcut: &GamepadShortcut, pressed: &HashSet<GamepadInput>) -> bool {
    !shortcut.inputs.is_empty()
        && shortcut.inputs.len() <= MAX_GAMEPAD_COMBO_INPUTS
        && shortcut.inputs.iter().all(|input| pressed.contains(input))
}
