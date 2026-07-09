fn refresh_runtime_state() {
    reload_config_if_changed();
    evaluate_auto_mute_inactivity();
    refresh_mute_state();
    refresh_mic_in_use_state_throttled();
}

fn refresh_mic_in_use_state_throttled() {
    let mut last_refresh = LAST_MIC_IN_USE_REFRESH.lock().unwrap();
    if last_refresh.is_some_and(|last| last.elapsed() < Duration::from_millis(MIC_IN_USE_REFRESH_MS))
    {
        return;
    }
    *last_refresh = Some(Instant::now());
    drop(last_refresh);

    refresh_mic_in_use_state();
}

fn refresh_mic_in_use_state() {
    let mic_in_use = match current_mic_in_use() {
        Ok(mic_in_use) => mic_in_use,
        Err(err) => {
            eprintln!("failed to refresh microphone use state: {err:?}");
            return;
        }
    };
    let changed = {
        let mut state = STATE.lock().unwrap();
        let changed = state.mic_in_use != mic_in_use;
        state.mic_in_use = mic_in_use;
        changed
    };
    if changed {
        refresh_tray_icon();
        apply_overlay_visibility();
    }
}

fn refresh_mute_state() {
    let muted = match enforce_default_capture_mute_volume() {
        Ok(muted) => muted,
        Err(err) => {
            eprintln!("failed to refresh microphone mute state: {err:?}");
            ensure_audio_notification_registration();
            return;
        }
    };
    if !muted && STATE.lock().unwrap().auto_muted_by_inactivity {
        clear_inactivity_auto_mute_flag();
    }
    let changed = STATE.lock().unwrap().muted != muted;
    if changed {
        set_global_mute_state(muted, true);
    }
}

fn evaluate_auto_mute_inactivity() {
    let settings = STATE.lock().unwrap().auto_mute.clone();
    if !auto_mute_monitoring_enabled(&settings) {
        reset_auto_mute_monitoring_state();
        return;
    }

    if STATE.lock().unwrap().auto_muted_by_inactivity && !current_mute_state().unwrap_or(true) {
        clear_inactivity_auto_mute_flag();
    }

    if STATE.lock().unwrap().auto_muted_by_inactivity
        && settings.unmute_on_activity
        && try_auto_unmute_from_mouse_movement()
    {
        return;
    }

    let last_input_tick = get_last_input_tick();
    if last_input_tick == 0 {
        return;
    }

    let threshold = Duration::from_secs(u64::from(settings.after_inactivity_minutes) * 60);
    if get_idle_time(last_input_tick) < threshold {
        return;
    }

    {
        let state = STATE.lock().unwrap();
        if state.last_auto_mute_input_tick == last_input_tick {
            return;
        }
    }

    STATE.lock().unwrap().last_auto_mute_input_tick = last_input_tick;
    if let Err(err) = apply_auto_mute(settings.play_sounds, true) {
        eprintln!("failed to apply inactivity auto-mute: {err:?}");
    }
}

fn apply_auto_mute(play_sound: bool, from_inactivity: bool) -> Result<()> {
    if current_mute_state()? {
        return Ok(());
    }

    let target_muted = set_mute(None, true)?;
    {
        let mut state = STATE.lock().unwrap();
        if from_inactivity {
            state.auto_muted_by_inactivity = true;
            state.auto_mute_cursor_position = get_cursor_position_or_default();
        } else {
            state.auto_muted_by_inactivity = false;
            state.auto_mute_cursor_position = POINT::default();
        }
    }
    set_global_mute_state(target_muted, true);

    if play_sound {
        play_auto_mute_sound();
    }

    Ok(())
}

fn try_auto_unmute_from_mouse_movement() -> bool {
    let current_position = get_cursor_position_or_default();
    let initial_position = STATE.lock().unwrap().auto_mute_cursor_position;
    if current_position.x == initial_position.x && current_position.y == initial_position.y {
        return false;
    }

    match set_mute(None, false) {
        Ok(target_muted) if !target_muted => {
            clear_inactivity_auto_mute_flag();
            set_global_mute_state(false, true);
            true
        }
        Ok(_) => false,
        Err(err) => {
            eprintln!("failed to auto-unmute after activity: {err:?}");
            false
        }
    }
}

fn clear_inactivity_auto_mute_flag() {
    let mut state = STATE.lock().unwrap();
    state.auto_muted_by_inactivity = false;
    state.auto_mute_cursor_position = POINT::default();
}

fn reset_auto_mute_monitoring_state() {
    let mut state = STATE.lock().unwrap();
    if !state.auto_muted_by_inactivity && state.last_auto_mute_input_tick == 0 {
        return;
    }
    state.auto_muted_by_inactivity = false;
    state.last_auto_mute_input_tick = 0;
    state.auto_mute_cursor_position = POINT::default();
}

fn auto_mute_monitoring_enabled(settings: &AutoMuteSettings) -> bool {
    settings.after_inactivity_enabled && settings.after_inactivity_minutes > 0
}

fn get_cursor_position_or_default() -> POINT {
    let mut point = POINT::default();
    if unsafe { GetCursorPos(&mut point) }.is_ok() {
        point
    } else {
        POINT::default()
    }
}

fn get_idle_time(last_input_tick: u32) -> Duration {
    let current_tick = unsafe { GetTickCount() };
    Duration::from_millis(u64::from(current_tick.wrapping_sub(last_input_tick)))
}

fn get_last_input_tick() -> u32 {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        info.dwTime
    } else {
        0
    }
}

fn reload_config_if_changed() {
    let modified = config_modified_time();
    let state = STATE.lock().unwrap();
    if modified == state.config_modified {
        return;
    }
    if let Ok(config) = load_config() {
        drop(state);
        apply_live_config(&config, modified);
    }
}

fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(path)?;
    parse_config_content(&content)
}

fn parse_config_content(content: &str) -> Result<Config> {
    let has_hotkeys = serde_json::from_str::<serde_json::Value>(&content)
        .ok()
        .and_then(|value| {
            value
                .as_object()
                .map(|object| object.contains_key("hotkeys"))
        })
        .unwrap_or(false);
    let mut config: Config = serde_json::from_str(&content)?;
    if !has_hotkeys {
        config.hotkeys = vec![HotkeyBinding {
            shortcut: config.shortcut.clone(),
            ..HotkeyBinding::default()
        }];
    }
    normalize_hotkeys(&mut config.hotkeys);
    migrate_custom_sound_settings(&mut config.sound_settings);
    normalize_overlay_config(&mut config.overlay);
    normalize_tray_icon_config(&mut config.tray_icon);
    config.advanced.audio_device_name_display =
        normalize_audio_device_name_display(&config.advanced.audio_device_name_display).to_string();
    config.auto_mute.after_inactivity_minutes =
        config.auto_mute.after_inactivity_minutes.clamp(1, 1440);
    Ok(config)
}

pub(crate) fn normalize_tray_icon_config(tray_icon: &mut TrayIconConfig) {
    let mut seen = HashSet::new();
    tray_icon.mic_in_use_ignored_apps = tray_icon
        .mic_in_use_ignored_apps
        .iter()
        .filter_map(|app| normalized_process_image_name(app))
        .filter(|app| seen.insert(app.to_ascii_lowercase()))
        .collect();
}

fn normalize_overlay_config(overlay: &mut OverlayConfig) {
    if let Some(action) = overlay.single_click_action.take() {
        overlay.single_click.action = Some(action);
    }
    if let Some(action) = overlay.double_click_action.take() {
        overlay.double_click.action = Some(action);
    }
    if let Some(action) = overlay.middle_click_action.take() {
        overlay.middle_click.action = Some(action);
    }
    if let Some(action) = overlay.right_click_action.take() {
        overlay.right_click.action = Some(action);
    }
    if let Some(action) = overlay.wheel_up_action.take() {
        overlay.wheel_up.action = Some(action);
    }
    if let Some(action) = overlay.wheel_down_action.take() {
        overlay.wheel_down.action = Some(action);
    }

    normalize_overlay_action_binding(&mut overlay.single_click);
    normalize_overlay_action_binding(&mut overlay.double_click);
    normalize_overlay_action_binding(&mut overlay.middle_click);
    normalize_overlay_action_binding(&mut overlay.right_click);
    normalize_overlay_action_binding(&mut overlay.wheel_up);
    normalize_overlay_action_binding(&mut overlay.wheel_down);

    if overlay.variant == "MicIcon" && overlay.show_text {
        overlay.variant = "IconText".to_string();
    }
    if !matches!(
        overlay.visibility.as_str(),
        "Always" | "WhenMuted" | "WhenUnmuted" | "WhenMicInUse" | "AfterToggle"
    ) {
        overlay.visibility = default_overlay_visibility();
    }
    if !matches!(
        overlay.variant.as_str(),
        "MicIcon" | "IconText" | "Text" | "Dot"
    ) {
        overlay.variant = default_overlay_variant();
    }
    overlay.show_text = false;
    overlay.text_font_weight = overlay.text_font_weight.clamp(100, 900);
}

fn normalize_overlay_action_binding(binding: &mut OverlayActionBinding) {
    let Some(action) = binding.action else {
        binding.target = None;
        binding.target_2 = None;
        return;
    };

    if !action.needs_target() {
        binding.target = None;
    }
    if !action.needs_second_target() {
        binding.target_2 = None;
    }
}

fn migrate_custom_sound_settings(settings: &mut SoundSettings) {
    if let Some(custom_sound) = settings.custom_mute_sound.take() {
        let id = add_migrated_custom_sound(settings, custom_sound, "mute");
        settings.mute_theme = custom_sound_value(&id);
    }
    if let Some(custom_sound) = settings.custom_unmute_sound.take() {
        let id = add_migrated_custom_sound(settings, custom_sound, "unmute");
        settings.unmute_theme = custom_sound_value(&id);
    }
}

fn add_migrated_custom_sound(
    settings: &mut SoundSettings,
    mut custom_sound: CustomSound,
    fallback_stem: &str,
) -> String {
    if custom_sound.id.is_empty() {
        custom_sound.id = fallback_stem.to_string();
    }
    let mut id = custom_sound.id.clone();
    if settings
        .custom_sounds
        .iter()
        .any(|existing| existing.id == id)
    {
        id = default_custom_sound_id();
        custom_sound.id = id.clone();
    }
    settings.custom_sounds.push(custom_sound);
    id
}

fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

pub fn export_settings() -> Result<()> {
    let Some(target) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .set_file_name("silence-settings.json")
        .save_file()
    else {
        return Ok(());
    };

    let config = load_config()?;
    fs::write(target, serde_json::to_string_pretty(&config)?)?;
    Ok(())
}

pub fn import_settings() -> Result<()> {
    let Some(source) = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file()
    else {
        return Ok(());
    };

    let content = fs::read_to_string(source)?;
    let mut config = parse_config_content(&content)?;
    config.welcome_completed = true;
    config.hotkeys_paused = false;
    save_config(&config)?;
    apply_live_config(&config, config_modified_time());
    Ok(())
}

pub fn import_v1_settings() -> Result<()> {
    let content = fs::read_to_string(v1_settings_path()?)?;
    let settings: Value = serde_json::from_str(&content)?;
    let mut config = Config::default();

    config.startup.launch_on_startup =
        value_bool(&settings, "AutoStartEnabled").unwrap_or(config.startup.launch_on_startup);
    config.sound_settings.enabled =
        value_bool(&settings, "SoundsEnabled").unwrap_or(config.sound_settings.enabled);
    config.sound_settings.volume =
        value_unit_percent(&settings, "SoundVolume").unwrap_or(config.sound_settings.volume);
    config.sound_settings.mute_theme = v1_sound_selection(
        &settings,
        "MuteSoundPreloaded",
        "MuteSoundCustomPath",
        "v1-mute",
    );
    config.sound_settings.unmute_theme = v1_sound_selection(
        &settings,
        "UnmuteSoundPreloaded",
        "UnmuteSoundCustomPath",
        "v1-unmute",
    );
    config.sound_settings.custom_sounds = v1_custom_sounds(&settings);

    config.hold_to_mute.play_sounds =
        value_bool(&settings, "HoldPlaySounds").unwrap_or(config.hold_to_mute.play_sounds);
    config.hold_to_mute.show_overlay =
        value_bool(&settings, "HoldShowOverlay").unwrap_or(config.hold_to_mute.show_overlay);
    config.hold_to_mute.volume_override = value_unit_percent(&settings, "HoldSoundVolume");
    config.hold_to_mute.mute_theme_override = v1_optional_sound_selection(
        &settings,
        "HoldMuteSoundPreloaded",
        "HoldMuteSoundCustomPath",
    );
    config.hold_to_mute.unmute_theme_override = v1_optional_sound_selection(
        &settings,
        "HoldUnmuteSoundPreloaded",
        "HoldUnmuteSoundCustomPath",
    );

    config.auto_mute.mute_on_startup =
        value_bool(&settings, "AutoMuteOnStartup").unwrap_or(config.auto_mute.mute_on_startup);
    config.auto_mute.after_inactivity_enabled =
        value_bool(&settings, "AutoMuteAfterInactivityEnabled")
            .unwrap_or(config.auto_mute.after_inactivity_enabled);
    config.auto_mute.after_inactivity_minutes =
        value_u16(&settings, "AutoMuteAfterInactivityMinutes")
            .unwrap_or(config.auto_mute.after_inactivity_minutes)
            .clamp(1, 1440);
    config.auto_mute.unmute_on_activity = value_bool(&settings, "AutoUnmuteOnActivity")
        .unwrap_or(config.auto_mute.unmute_on_activity);
    config.auto_mute.play_sounds =
        value_bool(&settings, "AutoMutePlaySounds").unwrap_or(config.auto_mute.play_sounds);

    config.overlay.enabled =
        value_bool(&settings, "OverlayEnabled").unwrap_or(config.overlay.enabled);
    config.overlay.visibility =
        value_string(&settings, "OverlayVisibilityMode").unwrap_or(config.overlay.visibility);
    config.overlay.position_x =
        value_f64(&settings, "OverlayPositionX").unwrap_or(config.overlay.position_x);
    config.overlay.position_y =
        value_f64(&settings, "OverlayPositionY").unwrap_or(config.overlay.position_y);
    config.overlay.duration_secs =
        value_f64(&settings, "OverlayShowDuration").unwrap_or(config.overlay.duration_secs);
    config.overlay.scale = value_u32(&settings, "OverlayScale").unwrap_or(config.overlay.scale);
    config.overlay.show_text =
        value_bool(&settings, "OverlayShowText").unwrap_or(config.overlay.show_text);
    config.overlay.variant =
        value_string(&settings, "OverlayVariant").unwrap_or(config.overlay.variant);
    config.overlay.icon_style =
        value_string(&settings, "OverlayIconStyle").unwrap_or(config.overlay.icon_style);
    config.overlay.background_style = value_string(&settings, "OverlayBackgroundStyle")
        .unwrap_or(config.overlay.background_style);
    config.overlay.background_opacity = value_u8(&settings, "OverlayOpacity")
        .unwrap_or(config.overlay.background_opacity)
        .min(100);
    config.overlay.content_opacity = value_u8(&settings, "OverlayContentOpacity")
        .unwrap_or(config.overlay.content_opacity)
        .clamp(20, 100);
    config.overlay.border_radius = value_u8(&settings, "OverlayBorderRadius")
        .unwrap_or(config.overlay.border_radius)
        .min(24);
    config.overlay.show_border =
        value_bool(&settings, "OverlayShowBorder").unwrap_or(config.overlay.show_border);
    normalize_overlay_config(&mut config.overlay);

    config.tray_icon.variant = match value_string(&settings, "TrayIconStyle").as_deref() {
        Some("FilledCircle") => "ColorDot".to_string(),
        Some("Dot") => "ColorDot".to_string(),
        Some("Standard") => "StatusMic".to_string(),
        _ => config.tray_icon.variant,
    };

    config.hotkeys = v1_hotkeys(&settings);
    config.shortcut = config
        .hotkeys
        .iter()
        .find(|binding| binding.action == HotkeyAction::ToggleMute && binding.gamepad.is_none())
        .map(|binding| binding.shortcut.clone())
        .unwrap_or_default();
    normalize_hotkeys(&mut config.hotkeys);

    save_config(&config)?;
    sync_startup_registration(config.startup.launch_on_startup)?;
    apply_live_config(&config, config_modified_time());
    Ok(())
}

pub fn complete_welcome() -> Result<()> {
    let mut config = load_config().unwrap_or_default();
    config.welcome_completed = true;
    config.hotkeys_paused = false;
    save_config(&config)?;
    apply_live_config(&config, config_modified_time());
    Ok(())
}

pub fn set_welcome_toggle_shortcut(shortcut: Shortcut) -> Result<()> {
    let mut config = load_config().unwrap_or_default();
    let shortcut = shortcut.normalized();
    if let Some(binding) = config
        .hotkeys
        .iter_mut()
        .find(|binding| binding.action == HotkeyAction::ToggleMute && binding.gamepad.is_none())
    {
        binding.shortcut = shortcut.clone();
        binding.gamepad = None;
        binding.target = None;
        binding.target_name = None;
        binding.target_2 = None;
        binding.target_2_name = None;
    } else {
        config.hotkeys.insert(
            0,
            HotkeyBinding {
                action: HotkeyAction::ToggleMute,
                shortcut: shortcut.clone(),
                gamepad: None,
                target: None,
                target_name: None,
                target_2: None,
                target_2_name: None,
                ..HotkeyBinding::default()
            },
        );
    }
    config.shortcut = shortcut;
    normalize_hotkeys(&mut config.hotkeys);
    save_config(&config)?;
    apply_live_config(&config, config_modified_time());
    Ok(())
}

pub fn reset_settings() -> Result<()> {
    let config = Config::default();
    save_config(&config)?;
    apply_live_config(&config, config_modified_time());
    Ok(())
}

fn v1_settings_path() -> Result<PathBuf> {
    let local_appdata = std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
    Ok(PathBuf::from(local_appdata)
        .join("silence")
        .join("settings.json"))
}

fn value_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key)?.as_bool()
}

fn value_string(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

fn value_f64(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64()
}

fn value_u8(value: &Value, key: &str) -> Option<u8> {
    let number = value.get(key)?.as_u64()?;
    u8::try_from(number).ok()
}

fn value_u16(value: &Value, key: &str) -> Option<u16> {
    let number = value.get(key)?.as_u64()?;
    u16::try_from(number).ok()
}

fn value_u32(value: &Value, key: &str) -> Option<u32> {
    let number = value.get(key)?.as_u64()?;
    u32::try_from(number).ok()
}

fn value_unit_percent(value: &Value, key: &str) -> Option<u8> {
    let percent = (value.get(key)?.as_f64()? * 100.0).round();
    Some(percent.clamp(0.0, 100.0) as u8)
}

fn v1_custom_sounds(settings: &Value) -> Vec<CustomSound> {
    [
        ("MuteSoundCustomPath", "v1-mute"),
        ("UnmuteSoundCustomPath", "v1-unmute"),
        ("HoldMuteSoundCustomPath", "v1-hold-mute"),
        ("HoldUnmuteSoundCustomPath", "v1-hold-unmute"),
    ]
    .into_iter()
    .filter_map(|(key, id)| {
        let path = value_string(settings, key)?;
        let path = PathBuf::from(path);
        let original_file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("custom sound")
            .to_string();
        Some(CustomSound {
            id: id.to_string(),
            path,
            original_file_name,
        })
    })
    .collect()
}

fn v1_optional_sound_selection(
    settings: &Value,
    preloaded_key: &str,
    custom_path_key: &str,
) -> Option<String> {
    if value_string(settings, custom_path_key).is_some() {
        return Some(custom_sound_value(match custom_path_key {
            "HoldMuteSoundCustomPath" => "v1-hold-mute",
            "HoldUnmuteSoundCustomPath" => "v1-hold-unmute",
            _ => "v1-custom",
        }));
    }
    value_string(settings, preloaded_key).map(|theme| v1_sound_theme(&theme).to_string())
}

fn v1_sound_selection(
    settings: &Value,
    preloaded_key: &str,
    custom_path_key: &str,
    custom_id: &str,
) -> String {
    if value_string(settings, custom_path_key).is_some() {
        return custom_sound_value(custom_id);
    }
    value_string(settings, preloaded_key)
        .map(|theme| v1_sound_theme(&theme).to_string())
        .unwrap_or_else(default_sound_theme)
}

fn v1_sound_theme(theme: &str) -> &'static str {
    match theme {
        "blob" => "blob",
        "digital" => "digital",
        "discord" => "discord",
        "pop" => "pop",
        "punchy" => "punchy",
        "scifi" => "scifi",
        "vibrant" => "vibrant",
        "8bit" => "8bit",
        "sifi" => "scifi",
        _ => default_sound_theme_static(),
    }
}

fn default_sound_theme_static() -> &'static str {
    "8bit"
}

fn v1_hotkeys(settings: &Value) -> Vec<HotkeyBinding> {
    let mut hotkeys = Vec::new();
    if let Some(bindings) = settings.get("HotkeyBindings").and_then(Value::as_array) {
        hotkeys.extend(bindings.iter().filter_map(v1_hotkey_binding));
    }
    if let Some(bindings) = settings.get("HoldHotkeyBindings").and_then(Value::as_array) {
        let action = match value_string(settings, "HoldAction").as_deref() {
            Some("HoldToMute") => HotkeyAction::HoldToMute,
            Some("HoldToUnmute") => HotkeyAction::HoldToUnmute,
            _ => HotkeyAction::HoldToToggle,
        };
        hotkeys.extend(bindings.iter().filter_map(|binding| {
            let mut hotkey = v1_base_hotkey_binding(binding, action)?;
            hotkey.ignore_modifiers = value_bool(settings, "IgnoreHoldModifiers").unwrap_or(false);
            Some(hotkey)
        }));
    }
    if hotkeys.is_empty() {
        let mut hotkey = HotkeyBinding::default();
        hotkey.shortcut = Shortcut {
            ctrl: v1_modifier_enabled(settings, "HotkeyModifiers", 2),
            alt: v1_modifier_enabled(settings, "HotkeyModifiers", 4),
            shift: v1_modifier_enabled(settings, "HotkeyModifiers", 1),
            win: v1_modifier_enabled(settings, "HotkeyModifiers", 8),
            vk: value_u32(settings, "HotkeyCode").unwrap_or_default(),
            mouse_buttons: Vec::new(),
        };
        hotkey.ignore_modifiers = value_bool(settings, "IgnoreModifiers").unwrap_or(false);
        hotkeys.push(hotkey);
    }
    if hotkeys.is_empty() {
        hotkeys.push(HotkeyBinding::default());
    }
    hotkeys
}

fn v1_hotkey_binding(binding: &Value) -> Option<HotkeyBinding> {
    let action = match value_string(binding, "Action").as_deref() {
        Some("Mute") => HotkeyAction::Mute,
        Some("Unmute") => HotkeyAction::Unmute,
        _ => HotkeyAction::ToggleMute,
    };
    let mut hotkey = v1_base_hotkey_binding(binding, action)?;
    hotkey.ignore_modifiers = value_bool(binding, "IgnoreModifiers").unwrap_or(false);
    Some(hotkey)
}

fn v1_base_hotkey_binding(binding: &Value, action: HotkeyAction) -> Option<HotkeyBinding> {
    let mut hotkey = HotkeyBinding {
        id: value_string(binding, "Id")
            .filter(|id| !id.is_empty())
            .unwrap_or_else(default_hotkey_id),
        action,
        shortcut: Shortcut {
            ctrl: v1_modifier_enabled(binding, "Modifiers", 2),
            alt: v1_modifier_enabled(binding, "Modifiers", 4),
            shift: v1_modifier_enabled(binding, "Modifiers", 1),
            win: v1_modifier_enabled(binding, "Modifiers", 8),
            vk: value_u32(binding, "KeyCode").unwrap_or_default(),
            mouse_buttons: value_u32_array(binding, "ChordKeyCodes")
                .into_iter()
                .filter(|code| is_supported_mouse_button(*code))
                .collect(),
        },
        gamepad: None,
        ignore_modifiers: false,
        target: None,
        target_name: None,
        target_2: None,
        target_2_name: None,
    };
    if v1_input_device_is_gamepad(binding) {
        hotkey.shortcut = Shortcut::default();
        hotkey.gamepad = Some(GamepadShortcut {
            inputs: v1_gamepad_inputs(binding),
        });
    }
    Some(hotkey)
}

fn v1_input_device_is_gamepad(value: &Value) -> bool {
    match value.get("DeviceKind") {
        Some(Value::Number(number)) => number.as_u64() == Some(1),
        Some(Value::String(kind)) => kind == "Gamepad",
        _ => false,
    }
}

fn v1_modifier_enabled(value: &Value, key: &str, flag: u64) -> bool {
    match value.get(key) {
        Some(Value::Number(number)) => number.as_u64().is_some_and(|bits| bits & flag != 0),
        Some(Value::String(text)) => text.split(", ").any(|part| {
            matches!(
                (part, flag),
                ("Shift", 1) | ("Ctrl", 2) | ("Alt", 4) | ("Win", 8)
            )
        }),
        _ => false,
    }
}

fn value_u32_array(value: &Value, key: &str) -> Vec<u32> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_u64().and_then(|number| u32::try_from(number).ok()))
                .collect()
        })
        .unwrap_or_default()
}

fn v1_gamepad_inputs(binding: &Value) -> Vec<GamepadInput> {
    let mask = binding
        .get("GamepadButtonsMask")
        .and_then(Value::as_u64)
        .unwrap_or_default();
    let mut inputs = (0..16)
        .filter(|index| mask & (1 << index) != 0)
        .filter_map(|index| v1_gamepad_button_id(index + 1))
        .collect::<Vec<_>>();
    if inputs.is_empty() {
        if let Some(button) = binding
            .get("GamepadButton")
            .and_then(Value::as_u64)
            .and_then(v1_gamepad_button_id)
        {
            inputs.push(button);
        }
    }
    inputs
        .into_iter()
        .map(|button| GamepadInput::Button { button })
        .collect()
}

fn v1_gamepad_button_id(id: u64) -> Option<GamepadButton> {
    Some(match id {
        1 => GamepadButton::South,
        2 => GamepadButton::East,
        3 => GamepadButton::West,
        4 => GamepadButton::North,
        5 => GamepadButton::LeftTrigger,
        6 => GamepadButton::RightTrigger,
        7 => GamepadButton::LeftTrigger2,
        8 => GamepadButton::RightTrigger2,
        9 => GamepadButton::Select,
        10 => GamepadButton::Start,
        11 => GamepadButton::DPadUp,
        12 => GamepadButton::DPadDown,
        13 => GamepadButton::DPadLeft,
        14 => GamepadButton::DPadRight,
        15 => GamepadButton::LeftThumb,
        16 => GamepadButton::RightThumb,
        _ => return None,
    })
}

pub fn open_external(target: &str) -> Result<()> {
    Command::new("explorer")
        .arg(target)
        .spawn()
        .with_context(|| format!("open external target {target}"))?;
    Ok(())
}

pub(crate) fn apply_live_config(config: &Config, modified: Option<SystemTime>) {
    let mut state = STATE.lock().unwrap();
    let volume_zero_mute_changed =
        state.advanced.set_mic_volume_to_zero_on_mute != config.advanced.set_mic_volume_to_zero_on_mute;
    state.shortcut = config.shortcut.clone();
    state.hotkeys = config.hotkeys.clone();
    state.hotkeys_paused = config.hotkeys_paused;
    state.sound_settings = config.sound_settings.clone();
    state.hold_to_mute = config.hold_to_mute.clone();
    state.auto_mute = config.auto_mute.clone();
    state.overlay = config.overlay.clone();
    state.tray_icon = config.tray_icon.clone();
    state.advanced = config.advanced.clone();
    state.welcome_completed = config.welcome_completed;
    state.modifiers = ModifierState::default();
    state.config_modified = modified;
    state.shortcut_down = false;
    state.hotkeys_down.clear();
    state.gamepad_inputs_down.clear();
    state.gamepad_hotkeys_down.clear();
    if !auto_mute_monitoring_enabled(&state.auto_mute) {
        state.last_auto_mute_input_tick = 0;
        state.auto_muted_by_inactivity = false;
        state.auto_mute_cursor_position = POINT::default();
    }
    drop(state);
    if gamepad_monitoring_needed(true) {
        ensure_gamepad_monitors(true);
    }
    refresh_tray_icon();
    apply_overlay_visibility();
    prime_sound_assets(&config.sound_settings);
    if volume_zero_mute_changed
        && let Err(err) = sync_capture_volume_zero_mute_setting(
            config.advanced.set_mic_volume_to_zero_on_mute,
        )
    {
        eprintln!("failed to sync capture volume mute setting: {err:?}");
    }
}

fn hotkeys_blocked(state: &AppState) -> bool {
    state.hotkeys_paused || !state.welcome_completed
}

fn normalize_hotkeys(hotkeys: &mut [HotkeyBinding]) {
    let mut seen = HashSet::new();
    for hotkey in hotkeys {
        if hotkey.id.is_empty() || !seen.insert(hotkey.id.clone()) {
            hotkey.id = default_hotkey_id();
            while !seen.insert(hotkey.id.clone()) {
                hotkey.id = default_hotkey_id();
            }
        }
        hotkey.shortcut = hotkey.shortcut.clone().normalized();
        hotkey.gamepad = hotkey.gamepad.take().and_then(GamepadShortcut::normalized);
    }
}

fn config_path() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("config.json"))
}

fn app_config_dir() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(appdata).join("SilenceV2"))
}

fn config_modified_time() -> Option<SystemTime> {
    config_path().ok()?.metadata().ok()?.modified().ok()
}

pub(crate) fn sync_startup_registration(enabled: bool) -> Result<()> {
    let subkey = wide(STARTUP_RUN_SUBKEY);
    let value_name = wide(STARTUP_RUN_VALUE);

    if enabled {
        let command = format!(
            "\"{}\"",
            std::env::current_exe()
                .context("locate current executable for startup registration")?
                .display()
        );
        let command_wide = wide(&command);
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(subkey.as_ptr()),
                PCWSTR(value_name.as_ptr()),
                REG_SZ.0,
                Some(command_wide.as_ptr() as *const c_void),
                (command_wide.len() * size_of::<u16>()) as u32,
            )
        };
        anyhow::ensure!(
            status == ERROR_SUCCESS,
            "startup registration failed with status {status:?}"
        );
        return Ok(());
    }

    let status = unsafe {
        RegDeleteKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
        )
    };
    anyhow::ensure!(
        status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND,
        "startup registration removal failed with status {status:?}"
    );
    Ok(())
}
