fn write_last_update_notification(update: &updater::UpdateInfo) -> Result<()> {
    let marker = LastUpdateNotification {
        version: update.version.clone(),
        release_url: update.release_url.clone(),
        shown: false,
    };
    let path = last_update_notification_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&marker)?)?;
    Ok(())
}

fn maybe_show_updated_notification() {
    let Ok(Some(mut marker)) = read_last_update_notification() else {
        return;
    };
    if marker.shown {
        return;
    }

    show_updated_notification(&marker.version);
    marker.shown = true;
    if let Ok(path) = last_update_notification_path() {
        let _ = fs::write(
            path,
            serde_json::to_string_pretty(&marker).unwrap_or_else(|_| "{}".to_string()),
        );
    }
}

fn show_updated_notification(version: &str) {
    register_notification_integration();

    let mut toast = winrt_toast::Toast::new();
    apply_toast_logo(&mut toast);
    toast
        .text1(format!("Updated to {version} successfully"))
        .duration(winrt_toast::ToastDuration::Long)
        .launch("whats-new")
        .action(
            winrt_toast::Action::new("What's new", "silence://whats-new", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        );

    let manager = winrt_toast::ToastManager::new(APP_USER_MODEL_ID);
    let activated = Some(Box::new(move |arguments: winrt_toast::Result<String>| {
        if let Some(action) = arguments
            .ok()
            .and_then(|arguments| notification_action_from_text(&arguments))
        {
            handle_notification_action(action);
        }
    })
        as Box<dyn FnMut(winrt_toast::Result<String>) + Send + 'static>);

    if let Err(err) = manager.show_with_callbacks(&toast, activated, None, None) {
        eprintln!("failed to show updated notification: {err:?}");
    }
}

fn open_last_update_release() {
    let target = read_last_update_notification()
        .ok()
        .flatten()
        .map(|marker| marker.release_url)
        .filter(|url| !url.trim().is_empty())
        .unwrap_or_else(|| "https://github.com/vertopolkaLF/silence/releases/latest".to_string());
    if let Err(err) = open_external(&target) {
        eprintln!("failed to open update release: {err:?}");
    }
}

fn read_last_update_notification() -> Result<Option<LastUpdateNotification>> {
    let path = last_update_notification_path()?;
    if !path.exists() {
        return Ok(None);
    }
    let content = fs::read_to_string(path)?;
    Ok(Some(serde_json::from_str(&content)?))
}

fn last_update_notification_path() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("last_update.json"))
}

fn start_update_check() {
    thread::spawn(|| {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(err) => {
                eprintln!("failed to create update check runtime: {err:?}");
                return;
            }
        };
        let result = runtime.block_on(updater::check_for_update());
        match result {
            Ok(updater::UpdateCheck::Available(update)) => {
                set_available_update(Some(update.clone()));
                if !auto_update_notifications_disabled() && updater::should_prompt_update(&update)
                {
                    show_update_notification(&update);
                }
            }
            Ok(updater::UpdateCheck::UpToDate) => {
                set_available_update(None);
            }
            Err(err) => eprintln!("update check failed: {err:?}"),
        }
    });
}

fn set_available_update(update: Option<updater::UpdateInfo>) {
    STATE.lock().unwrap().available_update = update;
}

fn auto_update_notifications_disabled() -> bool {
    load_config()
        .map(|config| config.advanced.disable_auto_updates)
        .unwrap_or(false)
}

pub(crate) fn development_tools_enabled() -> bool {
    cfg!(debug_assertions)
}

fn launched_from_installer() -> bool {
    std::env::args().any(|arg| arg == ARG_POST_INSTALL)
}

fn show_update_notification(update: &updater::UpdateInfo) {
    register_notification_integration();
    let mut toast = winrt_toast::Toast::new();
    apply_toast_logo(&mut toast);
    toast
        .text1("silence! update available")
        .text2(&format!(
            "{} -> {}",
            updater::current_version_text(),
            update.version
        ))
        .duration(winrt_toast::ToastDuration::Long)
        .launch("view-update")
        .action(
            winrt_toast::Action::new("Update now", "silence://update-now", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        )
        .action(
            winrt_toast::Action::new("View Release", "silence://view-update", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        );

    let manager = winrt_toast::ToastManager::new(APP_USER_MODEL_ID);
    let activated = Some(Box::new(move |arguments: winrt_toast::Result<String>| {
        if let Some(action) = arguments
            .ok()
            .and_then(|arguments| notification_action_from_text(&arguments))
        {
            handle_notification_action(action);
        }
    })
        as Box<dyn FnMut(winrt_toast::Result<String>) + Send + 'static>);

    if let Err(err) = manager.show_with_callbacks(&toast, activated, None, None) {
        eprintln!("failed to show update notification: {err:?}");
    }
}

fn apply_overlay_visibility() {
    let (hwnd, muted, mic_in_use, overlay) = {
        let state = STATE.lock().unwrap();
        (
            state.hwnd,
            state.muted,
            state.mic_in_use,
            state.overlay.clone(),
        )
    };

    if native_overlay::is_positioning() {
        native_overlay::update(muted, &overlay);
        native_overlay::show();
        sync_overlay_drag_timer(hwnd, &overlay);
        return;
    }

    if !overlay.enabled {
        native_overlay::hide();
        sync_overlay_drag_timer(hwnd, &overlay);
        return;
    }

    let should_show = match overlay.visibility.as_str() {
        "Always" => true,
        "WhenMuted" => muted,
        "WhenUnmuted" => !muted,
        "WhenMicInUse" => mic_in_use,
        "AfterToggle" => false,
        _ => muted,
    };

    if should_show {
        native_overlay::update(muted, &overlay);
        native_overlay::show();
    } else {
        native_overlay::hide();
    }
    sync_overlay_drag_timer(hwnd, &overlay);
}

fn show_overlay_temporarily(duration_ms: u32) {
    let (hwnd, muted, overlay) = {
        let state = STATE.lock().unwrap();
        (state.hwnd, state.muted, state.overlay.clone())
    };
    native_overlay::update(muted, &overlay);
    native_overlay::show();
    sync_overlay_drag_timer(hwnd, &overlay);
    unsafe {
        let _ = KillTimer(hwnd, ID_OVERLAY_HIDE_TIMER);
        let _ = SetTimer(hwnd, ID_OVERLAY_HIDE_TIMER, duration_ms, None);
    }
}

fn sync_overlay_drag_timer(hwnd: HWND, overlay: &OverlayConfig) {
    unsafe {
        if native_overlay::is_positioning() || (overlay.enabled && overlay.behaviour == "Button") {
            let _ = SetTimer(hwnd, ID_OVERLAY_DRAG_TIMER, 16, None);
        } else {
            let _ = KillTimer(hwnd, ID_OVERLAY_DRAG_TIMER);
        }
    }
}

pub fn set_overlay_positioning(active: bool) -> Option<OverlayConfig> {
    let class = wide("SilenceV2Hidden");
    let hwnd = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(null())) };
    let Ok(hwnd) = hwnd else {
        return None;
    };
    if hwnd.0.is_null() {
        return None;
    }
    let _ = unsafe {
        SendMessageW(
            hwnd,
            WM_OVERLAY_POSITIONING,
            WPARAM(usize::from(active)),
            LPARAM(0),
        )
    };

    if active {
        None
    } else {
        load_config().ok().map(|config| config.overlay)
    }
}

pub(crate) fn save_overlay_position(position_x: f64, position_y: f64) {
    let mut config = load_config().unwrap_or_default();
    config.overlay.position_x = position_x;
    config.overlay.position_y = position_y;
    let _ = save_config(&config);

    let mut state = STATE.lock().unwrap();
    state.overlay.position_x = position_x;
    state.overlay.position_y = position_y;
    state.config_modified = config_modified_time();
}
