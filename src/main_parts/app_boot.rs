pub(crate) fn set_settings_hotkey_recording(recording: bool) {
    let was_recording = SETTINGS_HOTKEY_RECORDING.swap(recording, Ordering::Relaxed);
    if recording != was_recording {
        SETTINGS_MOUSE_HELD.lock().unwrap().clear();
        SETTINGS_MOUSE_PRESSED_SHORTCUT.lock().unwrap().take();
    }
}

pub(crate) fn take_settings_alt_space_recorded() -> bool {
    SETTINGS_ALT_SPACE_RECORDED.swap(false, Ordering::Relaxed)
}

pub(crate) fn set_settings_gamepad_recording(recording: bool) {
    let was_recording = SETTINGS_GAMEPAD_RECORDING.swap(recording, Ordering::Relaxed);
    if recording != was_recording {
        SETTINGS_GAMEPAD_HELD.lock().unwrap().clear();
    }
    if recording {
        ensure_gamepad_monitors(false);
    }
}

pub(crate) fn settings_gamepad_held_inputs() -> Vec<GamepadInput> {
    let mut inputs = SETTINGS_GAMEPAD_HELD
        .lock()
        .unwrap()
        .iter()
        .copied()
        .collect::<Vec<_>>();
    inputs.sort_by_key(|input| input.label());
    inputs
}

pub(crate) fn settings_mouse_held_buttons() -> Vec<u32> {
    SETTINGS_MOUSE_HELD.lock().unwrap().clone()
}

pub(crate) fn take_settings_mouse_pressed_shortcut() -> Option<Shortcut> {
    SETTINGS_MOUSE_PRESSED_SHORTCUT.lock().unwrap().take()
}

fn has_alt_space_hotkey() -> bool {
    STATE
        .lock()
        .unwrap()
        .hotkeys
        .iter()
        .filter(|hotkey| hotkey.gamepad.is_none())
        .any(|hotkey| shortcut_is_alt_space(&hotkey.shortcut))
}

fn shortcut_is_alt_space(shortcut: &Shortcut) -> bool {
    shortcut.alt && !shortcut.ctrl && !shortcut.shift && !shortcut.win && shortcut.vk == 0x20
}

pub(crate) fn install_settings_window_guard(hwnd: isize) {
    if hwnd == 0 || SETTINGS_ORIGINAL_WNDPROC.load(Ordering::Relaxed) != 0 {
        return;
    }

    let mica_enabled = load_config()
        .map(|config| effective_settings_mica_enabled(&config))
        .unwrap_or_default();
    SETTINGS_MICA_ENABLED.store(mica_enabled, Ordering::Relaxed);
    apply_settings_backdrop(HWND(hwnd as *mut c_void), mica_enabled);

    let previous = unsafe {
        SetWindowLongPtrW(
            HWND(hwnd as *mut c_void),
            GWL_WNDPROC,
            settings_window_proc as *const () as WindowLongPtrValue,
        )
    };
    if previous != 0 {
        SETTINGS_ORIGINAL_WNDPROC.store(previous as isize, Ordering::Relaxed);
    }
}

pub(crate) fn set_settings_mica_enabled(enabled: bool) {
    let enabled = enabled && settings_mica_available();
    SETTINGS_MICA_ENABLED.store(enabled, Ordering::Relaxed);
    let title = wide(SETTINGS_WINDOW_TITLE);
    if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR(null()), PCWSTR(title.as_ptr())) } {
        apply_settings_backdrop(hwnd, enabled);
    }
}

fn apply_settings_backdrop(hwnd: HWND, enabled: bool) {
    let enabled = enabled && settings_mica_available();
    unsafe {
        let use_dark_mode = 1_i32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &use_dark_mode as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );

        let backdrop = if enabled {
            DWMSBT_MAINWINDOW
        } else {
            DWMSBT_NONE
        };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const c_void,
            size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        );

        let mica_enabled = if enabled { 1_i32 } else { 0_i32 };
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_MICA_EFFECT,
            &mica_enabled as *const _ as *const c_void,
            size_of::<i32>() as u32,
        );
    }
}

unsafe extern "system" fn settings_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_SYSCOMMAND && (wparam.0 & 0xfff0) == SC_KEYMENU as usize {
        if SETTINGS_HOTKEY_RECORDING.load(Ordering::Relaxed) {
            SETTINGS_ALT_SPACE_RECORDED.store(true, Ordering::Relaxed);
            return LRESULT(0);
        }
        if has_alt_space_hotkey() {
            return LRESULT(0);
        }
    }

    let refresh_backdrop = matches!(
        msg,
        WM_DWMCOMPOSITIONCHANGED
            | WM_DISPLAYCHANGE
            | WM_DPICHANGED
            | WM_SETTINGCHANGE
            | WM_THEMECHANGED
            | WM_WINDOWPOSCHANGED
    );

    let previous = SETTINGS_ORIGINAL_WNDPROC.load(Ordering::Relaxed);
    let result = if previous == 0 {
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    } else {
        let previous: WNDPROC = unsafe { std::mem::transmute(previous) };
        unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) }
    };

    if refresh_backdrop {
        apply_settings_backdrop(hwnd, SETTINGS_MICA_ENABLED.load(Ordering::Relaxed));
    }

    result
}

struct AudioEngine {
    cached_sounds: HashMap<String, SamplesBuffer>,
    active_sinks: Vec<ActiveSink>,
    preview_sink: Option<ActiveSink>,
}

struct ActiveSink {
    sink: MixerDeviceSink,
    finishes_at: Instant,
}

impl AudioEngine {
    fn new() -> Result<Self> {
        Ok(Self {
            cached_sounds: HashMap::new(),
            active_sinks: Vec::new(),
            preview_sink: None,
        })
    }

    fn decoded_sound(&mut self, cache_key: &str, path: &Path) -> Result<SamplesBuffer> {
        if let Some(sound) = self.cached_sounds.get(cache_key) {
            return Ok(sound.clone());
        }

        let bytes =
            fs::read(path).with_context(|| format!("read sound asset {}", path.display()))?;
        self.decoded_sound_bytes(cache_key, bytes)
    }

    fn decoded_sound_bytes(&mut self, cache_key: &str, bytes: Vec<u8>) -> Result<SamplesBuffer> {
        if let Some(sound) = self.cached_sounds.get(cache_key) {
            return Ok(sound.clone());
        }

        let decoder = Decoder::try_from(Cursor::new(bytes)).context("decode sound asset")?;
        let sound = decoder.record();
        self.cached_sounds
            .insert(cache_key.to_string(), sound.clone());
        Ok(sound)
    }

    fn play_sound(&mut self, sound: SamplesBuffer, volume: f32) -> Result<Duration> {
        self.prune_finished_sinks();

        let mut sink =
            DeviceSinkBuilder::open_default_sink().context("open default audio stream")?;
        sink.log_on_drop(false);

        let clip_duration = sound.total_duration().unwrap_or(Duration::from_secs(1));
        sink.mixer().add(sound.amplify(volume));
        self.active_sinks.push(ActiveSink {
            sink,
            finishes_at: Instant::now() + clip_duration + Duration::from_millis(250),
        });
        Ok(clip_duration)
    }

    fn play_preview_sound(&mut self, sound: SamplesBuffer, volume: f32) -> Result<Duration> {
        self.stop_preview_sound();

        let mut sink =
            DeviceSinkBuilder::open_default_sink().context("open default audio stream")?;
        sink.log_on_drop(false);

        let clip_duration = sound.total_duration().unwrap_or(Duration::from_secs(1));
        sink.mixer().add(sound.amplify(volume));
        self.preview_sink = Some(ActiveSink {
            sink,
            finishes_at: Instant::now() + clip_duration + Duration::from_millis(250),
        });
        Ok(clip_duration)
    }

    fn stop_preview_sound(&mut self) {
        self.preview_sink = None;
    }

    fn prune_finished_sinks(&mut self) {
        let now = Instant::now();
        self.active_sinks.retain(|sink| sink.finishes_at > now);
        if self
            .preview_sink
            .as_ref()
            .is_some_and(|sink| sink.finishes_at <= now)
        {
            self.preview_sink = None;
        }
    }
}

const SOUND_THEMES: &[SoundTheme] = &[
    SoundTheme {
        id: "8bit",
        label: "8-Bit",
    },
    SoundTheme {
        id: "blob",
        label: "Blob",
    },
    SoundTheme {
        id: "digital",
        label: "Digital",
    },
    SoundTheme {
        id: "discord",
        label: "Discord",
    },
    SoundTheme {
        id: "pop",
        label: "Pop",
    },
    SoundTheme {
        id: "punchy",
        label: "Punchy",
    },
    SoundTheme {
        id: "scifi",
        label: "Sci-Fi",
    },
    SoundTheme {
        id: "vibrant",
        label: "Vibrant",
    },
];

fn main() -> Result<()> {
    set_dpi_awareness();

    if let Some(action) = notification_action_from_args(std::env::args().skip(1)) {
        if dispatch_notification_action(action) {
            return Ok(());
        }
        PENDING_NOTIFICATION_ACTION.lock().unwrap().replace(action);
    }

    if std::env::args().any(|arg| arg == "--settings") {
        let settings_mutex = unsafe { CreateMutexW(None, true, w!("SilenceV2SettingsWindow"))? };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            focus_settings_window();
            return Ok(());
        }

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
        }
        let settings_window_size = LogicalSize::new(760.0, 590.0);
        let settings_window_position = centered_window_position(settings_window_size);
        let cfg = DesktopConfig::new()
            .with_window(
                WindowBuilder::new()
                    .with_title(SETTINGS_WINDOW_TITLE)
                    .with_decorations(false)
                    .with_resizable(true)
                    .with_transparent(true)
                    .with_no_redirection_bitmap(true)
                    .with_visible(false)
                    .with_inner_size(settings_window_size)
                    .with_min_inner_size(settings_window_size)
                    .with_position(settings_window_position),
            )
            .with_icon(
                dioxus::desktop::icon_from_memory(include_bytes!("../../assets/app.png"))
                    .expect("load app icon"),
            )
            .with_custom_head(gui::settings_startup_head())
            .with_background_color((0, 0, 0, 0));
        MOUSE_HOTKEYS_ENABLED.store(false, Ordering::Relaxed);
        install_mouse_hook(unsafe { GetModuleHandleW(None)? }.into())?;
        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(|| {
            if gamepad_monitoring_needed(false) {
                ensure_gamepad_monitors(false);
            }
            gui::settings_app()
        });
        let _settings_mutex = settings_mutex;
        return Ok(());
    }

    let main_mutex = unsafe { CreateMutexW(None, true, MAIN_INSTANCE_MUTEX)? };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if let Some(action) = PENDING_NOTIFICATION_ACTION.lock().unwrap().take() {
            dispatch_notification_action(action);
        } else {
            dispatch_notification_action(NotificationAction::OpenSettings);
        }
        return Ok(());
    }

    let result = run_background_app();
    let _main_mutex = main_mutex;
    result
}

fn notification_action_from_args(
    args: impl IntoIterator<Item = String>,
) -> Option<NotificationAction> {
    args.into_iter()
        .find_map(|arg| notification_action_from_text(&arg))
}

fn notification_action_from_text(value: &str) -> Option<NotificationAction> {
    let lower = value.trim().trim_end_matches('/').to_ascii_lowercase();
    if lower == "--toggle-mute"
        || lower == "toggle-mute"
        || lower.ends_with("://toggle-mute")
        || lower.contains("toggle-mute")
    {
        Some(NotificationAction::ToggleMute)
    } else if lower == "--mute" || lower == "mute" || lower.ends_with("://mute") {
        Some(NotificationAction::Mute)
    } else if lower == "--unmute"
        || lower == "unmute"
        || lower.ends_with("://unmute")
        || lower.contains("unmute")
    {
        Some(NotificationAction::Unmute)
    } else if lower == "settings"
        || lower == "open-settings"
        || lower.ends_with("://settings")
        || lower.ends_with("://open-settings")
        || lower.contains("open-settings")
    {
        Some(NotificationAction::OpenSettings)
    } else if lower == "update-now"
        || lower == "--update-now"
        || lower.ends_with("://update-now")
        || lower.contains("update-now")
    {
        Some(NotificationAction::UpdateNow)
    } else if lower == "view-update"
        || lower == "--view-update"
        || lower.ends_with("://view-update")
        || lower.contains("view-update")
    {
        Some(NotificationAction::ViewUpdate)
    } else if lower == "whats-new"
        || lower == "what-s-new"
        || lower == "--whats-new"
        || lower.ends_with("://whats-new")
        || lower.ends_with("://what-s-new")
        || lower.contains("whats-new")
    {
        Some(NotificationAction::WhatsNew)
    } else if lower == "exit-all"
        || lower == "--exit-all"
        || lower == "quit"
        || lower == "--quit"
        || lower.ends_with("://exit-all")
        || lower.ends_with("://quit")
        || lower.contains("exit-all")
    {
        Some(NotificationAction::ExitAll)
    } else {
        None
    }
}

fn hidden_window() -> Option<HWND> {
    let class = wide("SilenceV2Hidden");
    let hwnd = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(null())) }.ok()?;
    (!hwnd.0.is_null()).then_some(hwnd)
}

fn dispatch_notification_action(action: NotificationAction) -> bool {
    let Some(hwnd) = hidden_window() else {
        return false;
    };
    let message = match action {
        NotificationAction::ToggleMute => WM_TOGGLE_MUTE,
        NotificationAction::Mute => WM_MUTE,
        NotificationAction::Unmute => WM_UNMUTE,
        NotificationAction::OpenSettings => WM_OPEN_SETTINGS,
        NotificationAction::UpdateNow => WM_UPDATE_NOW,
        NotificationAction::ViewUpdate => WM_OPEN_SETTINGS,
        NotificationAction::WhatsNew => WM_WHATS_NEW,
        NotificationAction::ExitAll => WM_EXIT_ALL,
    };
    unsafe {
        let _ = SendMessageW(hwnd, message, WPARAM(0), LPARAM(0));
    }
    true
}

fn apply_pending_notification_action() {
    let action = PENDING_NOTIFICATION_ACTION.lock().unwrap().take();
    if let Some(action) = action {
        handle_notification_action(action);
    }
}

fn handle_notification_action(action: NotificationAction) {
    match action {
        NotificationAction::ToggleMute => toggle_mute(),
        NotificationAction::Mute => set_mute_target(None, true),
        NotificationAction::Unmute => set_mute_target(None, false),
        NotificationAction::OpenSettings => launch_settings_window(Some("--about")),
        NotificationAction::UpdateNow => launch_settings_window(Some("--about-update")),
        NotificationAction::ViewUpdate => launch_settings_window(Some("--about")),
        NotificationAction::WhatsNew => open_last_update_release(),
        NotificationAction::ExitAll => exit_all_processes(),
    }
}

pub(crate) fn run_update_now_action() {
    handle_notification_action(NotificationAction::UpdateNow);
}

fn post_audio_window_message(hwnd: HWND, message: u32) {
    if hwnd.0.is_null() {
        return;
    }

    unsafe {
        let _ = PostMessageW(hwnd, message, WPARAM(0), LPARAM(0));
    }
}

fn ensure_audio_notification_registration() {
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }

    AUDIO_NOTIFICATION_REGISTRATION.with(|registration| {
        let mut registration = registration.borrow_mut();
        if let Some(registration) = registration.as_mut() {
            if let Err(err) = registration.rebind_default_capture_volume() {
                eprintln!("failed to rebind default capture mute notifications: {err:?}");
            }
            return;
        }

        match AudioNotificationRegistration::new(hwnd) {
            Ok(value) => *registration = Some(value),
            Err(err) => eprintln!("failed to initialize audio notifications: {err:?}"),
        }
    });
}

fn shutdown_audio_notification_registration() {
    AUDIO_NOTIFICATION_REGISTRATION.with(|registration| {
        let registration = registration.borrow_mut().take();
        if let Some(registration) = registration {
            registration.shutdown();
        }
    });
}

fn register_notification_integration() {
    if let Err(err) = register_protocol_handler() {
        eprintln!("failed to register notification protocol handler: {err:?}");
    }
    if let Err(err) = winrt_toast::register(
        APP_USER_MODEL_ID,
        "silence!",
        notification_icon_path().as_deref(),
    ) {
        eprintln!("failed to register toast app id: {err:?}");
    }
}

fn notification_icon_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let direct_candidates = [
        exe_dir.join("app.ico"),
        exe_dir.join("assets").join("app.ico"),
        PathBuf::from("assets").join("app.ico"),
    ]
    .into_iter()
    .find_map(|path| {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir().ok()?.join(path)
        };
        path.exists().then_some(path)
    });
    direct_candidates.or_else(|| find_asset_icon(&exe_dir.join("assets")))
}

fn find_asset_icon(asset_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(asset_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            name.starts_with("app-") && name.ends_with(".ico")
        })
}

fn toast_logo_path() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let direct_candidates = [
        exe_dir.join("app.png"),
        exe_dir.join("assets").join("app.png"),
        PathBuf::from("assets").join("app.png"),
    ]
    .into_iter()
    .find_map(|path| {
        let path = if path.is_absolute() {
            path
        } else {
            std::env::current_dir().ok()?.join(path)
        };
        path.exists().then_some(path)
    });
    direct_candidates
        .or_else(|| find_asset_image(&exe_dir.join("assets"), "app-", ".png"))
        .or_else(|| notification_icon_path())
}

fn find_asset_image(asset_dir: &Path, prefix: &str, suffix: &str) -> Option<PathBuf> {
    fs::read_dir(asset_dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            name.starts_with(prefix) && name.ends_with(suffix)
        })
}

fn apply_toast_logo(toast: &mut winrt_toast::Toast) {
    let Some(path) = toast_logo_path() else {
        return;
    };
    let Ok(image) = winrt_toast::Image::new_local(path).map(|image| {
        image
            .with_placement(winrt_toast::content::image::ImagePlacement::AppLogoOverride)
            .with_alt("silence!")
    }) else {
        return;
    };
    toast.image(1, image);
}

fn register_protocol_handler() -> Result<()> {
    let exe = std::env::current_exe().context("locate current executable for protocol handler")?;
    let command = format!("\"{}\" \"%1\"", exe.display());
    let root_key = format!(r"Software\Classes\{APP_PROTOCOL}");
    let command_key = format!(r"{root_key}\shell\open\command");
    set_hkcu_string(&root_key, "", &format!("URL:{APP_PROTOCOL}"))?;
    set_hkcu_string(&root_key, "URL Protocol", "")?;
    set_hkcu_string(&command_key, "", &command)?;
    Ok(())
}

fn set_hkcu_string(subkey: &str, value_name: &str, value: &str) -> Result<()> {
    let subkey = wide(subkey);
    let value_name_wide = wide(value_name);
    let value = wide(value);
    let name = if value_name.is_empty() {
        PCWSTR(null())
    } else {
        PCWSTR(value_name_wide.as_ptr())
    };
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            name,
            REG_SZ.0,
            Some(value.as_ptr() as *const c_void),
            (value.len() * size_of::<u16>()) as u32,
        )
    };
    anyhow::ensure!(
        status == ERROR_SUCCESS,
        "registry write failed with status {status:?}"
    );
    Ok(())
}

fn centered_window_position(size: LogicalSize<f64>) -> PhysicalPosition<i32> {
    let dpi_scale = unsafe { GetDpiForSystem() }.max(96) as f64 / 96.0;
    let screen_width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_height = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let window_width = (size.width * dpi_scale).round() as i32;
    let window_height = (size.height * dpi_scale).round() as i32;

    PhysicalPosition::new(
        ((screen_width - window_width) / 2).max(0),
        ((screen_height - window_height) / 2).max(0),
    )
}

fn set_dpi_awareness() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}
