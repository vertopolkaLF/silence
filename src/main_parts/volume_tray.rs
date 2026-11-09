// Second notification icon: output volume. The mic icon stays on `WM_TRAY` / `ID_TRAY`.
// This one uses `WM_VOLUME_TRAY` / `ID_VOLUME_TRAY` and the same pre-Vista callback
// shape (mouse message in `lparam`). `NOTIFYICON_VERSION_4` is not set.

static VOLUME_TRAY_ADDED: AtomicBool = AtomicBool::new(false);
static SUPPRESS_NEXT_VOLUME_TRAY_LBUTTON_UP: AtomicBool = AtomicBool::new(false);
static VOLUME_TRAY_ICONS: Lazy<Mutex<VolumeTrayIcons>> =
    Lazy::new(|| Mutex::new(VolumeTrayIcons::new()));

struct VolumeTrayIcons {
    cache: volume_tray_icons::VolumeIconCache,
    shown_key: Option<VolumeTrayIconKey>,
    shown_tip: String,
}

// `HICON` is a raw handle. The cache is only touched on the tray UI thread and
// from `Drop` at process exit.
unsafe impl Send for VolumeTrayIcons {}

impl VolumeTrayIcons {
    fn new() -> Self {
        Self {
            cache: volume_tray_icons::VolumeIconCache::new(),
            shown_key: None,
            shown_tip: String::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct VolumeTrayIconKey {
    percent: u8,
    muted: bool,
    style: volume_tray_icons::VolumeIconStyle,
    size_px: u32,
}

struct VolumeTraySnapshot {
    key: VolumeTrayIconKey,
    tip: String,
}

fn volume_tray_wanted() -> bool {
    let state = STATE.lock().unwrap();
    state.modules.volume && state.volume.tray_icon.enabled
}

/// Output devices move to the volume icon's menu while that icon is actually shown.
/// Module on + tray icon off keeps them on the microphone menu.
fn volume_menu_owns_output() -> bool {
    volume_tray_wanted()
}

fn add_volume_tray_icon(hwnd: HWND) {
    if !volume_tray_wanted() {
        unsafe {
            let _ = KillTimer(hwnd, ID_VOLUME_TRAY_ADD_RETRY_TIMER);
        }
        return;
    }
    if VOLUME_TRAY_ADDED.load(Ordering::Relaxed) {
        return;
    }

    let snapshot = volume_tray_snapshot();
    let Some(icon) = take_volume_icon(&snapshot.key) else {
        unsafe {
            let _ = SetTimer(hwnd, ID_VOLUME_TRAY_ADD_RETRY_TIMER, TRAY_ADD_RETRY_MS, None);
        }
        return;
    };

    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_VOLUME_TRAY,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_VOLUME_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    write_packed_wide_buf(std::ptr::addr_of_mut!(nid.szTip), &snapshot.tip);
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            VOLUME_TRAY_ADDED.store(true, Ordering::Relaxed);
            let _ = KillTimer(hwnd, ID_VOLUME_TRAY_ADD_RETRY_TIMER);
            {
                let mut icons = VOLUME_TRAY_ICONS.lock().unwrap();
                icons.shown_key = Some(snapshot.key);
                icons.shown_tip = snapshot.tip;
            }
            if wheel_targets_refresh_wanted() {
                refresh_wheel_targets();
            }
        } else {
            let _ = SetTimer(hwnd, ID_VOLUME_TRAY_ADD_RETRY_TIMER, TRAY_ADD_RETRY_MS, None);
        }
    }
}

fn remove_volume_tray_icon() {
    let hwnd = STATE.lock().unwrap().hwnd;
    if !hwnd.0.is_null() {
        let nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ID_VOLUME_TRAY,
            ..Default::default()
        };
        unsafe {
            let _ = KillTimer(hwnd, ID_VOLUME_TRAY_ADD_RETRY_TIMER);
            let _ = KillTimer(hwnd, ID_VOLUME_TRAY_CLICK_TIMER);
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
    VOLUME_TRAY_ADDED.store(false, Ordering::Relaxed);
    SUPPRESS_NEXT_VOLUME_TRAY_LBUTTON_UP.store(false, Ordering::Relaxed);
    let mut icons = VOLUME_TRAY_ICONS.lock().unwrap();
    icons.shown_key = None;
    icons.shown_tip.clear();
    icons.cache.invalidate();
}

fn sync_volume_tray_icon() {
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    if volume_tray_wanted() {
        add_volume_tray_icon(hwnd);
        refresh_volume_tray();
    } else {
        remove_volume_tray_icon();
    }
}

fn refresh_volume_tray() {
    if !VOLUME_TRAY_ADDED.load(Ordering::Relaxed) {
        return;
    }
    let snapshot = volume_tray_snapshot();
    refresh_volume_tray_icon(&snapshot);
    refresh_volume_tray_tip(&snapshot);
}

fn refresh_volume_tray_icon(snapshot: &VolumeTraySnapshot) {
    if !VOLUME_TRAY_ADDED.load(Ordering::Relaxed) {
        return;
    }
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    if VOLUME_TRAY_ICONS.lock().unwrap().shown_key.as_ref() == Some(&snapshot.key) {
        return;
    }
    let Some(icon) = take_volume_icon(&snapshot.key) else {
        return;
    };
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_VOLUME_TRAY,
        uFlags: NIF_ICON,
        hIcon: icon,
        ..Default::default()
    };
    unsafe {
        if Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() {
            VOLUME_TRAY_ICONS.lock().unwrap().shown_key = Some(snapshot.key);
        }
    }
}

fn refresh_volume_tray_tip(snapshot: &VolumeTraySnapshot) {
    if !VOLUME_TRAY_ADDED.load(Ordering::Relaxed) {
        return;
    }
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    if VOLUME_TRAY_ICONS.lock().unwrap().shown_tip == snapshot.tip {
        return;
    }
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_VOLUME_TRAY,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    write_packed_wide_buf(std::ptr::addr_of_mut!(nid.szTip), &snapshot.tip);
    unsafe {
        if Shell_NotifyIconW(NIM_MODIFY, &nid).as_bool() {
            VOLUME_TRAY_ICONS.lock().unwrap().shown_tip = snapshot.tip.clone();
        }
    }
}

fn volume_tray_snapshot() -> VolumeTraySnapshot {
    let (config, output) = {
        let state = STATE.lock().unwrap();
        (state.volume.tray_icon.clone(), state.output_volume.clone())
    };
    let percent = output_level_percent(output.level);
    let key = VolumeTrayIconKey {
        percent,
        muted: output.muted,
        style: volume_icon_style(&config),
        size_px: volume_tray_icons::tray_icon_size_px(),
    };
    VolumeTraySnapshot {
        key,
        tip: volume_tray_tip(&output.device_name, percent, output.muted),
    }
}

fn take_volume_icon(key: &VolumeTrayIconKey) -> Option<HICON> {
    let mut icons = VOLUME_TRAY_ICONS.lock().unwrap();
    let style_or_size_changed = icons.shown_key.as_ref().is_some_and(|previous| {
        previous.style != key.style || previous.size_px != key.size_px
    });
    if style_or_size_changed {
        icons.cache.invalidate();
    }
    match icons
        .cache
        .hicon(&key.style, key.percent, key.muted, key.size_px)
    {
        Ok(icon) => Some(icon),
        Err(err) => {
            eprintln!("failed to render volume tray icon: {err:?}");
            None
        }
    }
}

fn output_level_percent(level: f32) -> u8 {
    (level.clamp(0.0, 1.0) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

fn volume_tray_tip(device_name: &str, percent: u8, muted: bool) -> String {
    let name = if device_name.trim().is_empty() {
        "Output"
    } else {
        device_name.trim()
    };
    if muted {
        format!("{name}: muted")
    } else {
        format!("{name}: {percent}%")
    }
}

pub(crate) fn volume_icon_style(config: &VolumeTrayIconConfig) -> volume_tray_icons::VolumeIconStyle {
    let accent = match config.color {
        TrayColorMode::Accent => volume_tray_icons::windows_accent_rgb(),
        TrayColorMode::Custom => parse_volume_tray_color(&config.custom_color),
    };
    volume_tray_icons::VolumeIconStyle::new(
        map_volume_tray_skin(config.skin),
        accent,
        volume_tray_icons::taskbar_uses_light_theme(),
    )
}

pub(crate) fn map_volume_tray_skin(skin: VolumeTraySkin) -> volume_tray_icons::VolumeTraySkin {
    match skin {
        VolumeTraySkin::Rings => volume_tray_icons::VolumeTraySkin::Rings,
        VolumeTraySkin::Digits => volume_tray_icons::VolumeTraySkin::Digits,
        VolumeTraySkin::Bar => volume_tray_icons::VolumeTraySkin::Bar,
        VolumeTraySkin::Speaker => volume_tray_icons::VolumeTraySkin::Speaker,
    }
}

pub(crate) fn parse_volume_tray_color(value: &str) -> [u8; 3] {
    let hex = value.trim().trim_start_matches('#');
    if hex.len() == 6
        && let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&hex[0..2], 16),
            u8::from_str_radix(&hex[2..4], 16),
            u8::from_str_radix(&hex[4..6], 16),
        )
    {
        return [r, g, b];
    }
    [0x5C, 0xA8, 0xFF]
}

fn handle_volume_tray_message(hwnd: HWND, event: u32) {
    match event {
        WM_LBUTTONUP => handle_volume_tray_left_click(hwnd),
        WM_LBUTTONDBLCLK => handle_volume_tray_double_click(hwnd),
        WM_MBUTTONUP => {
            let action = STATE.lock().unwrap().volume.clicks.middle;
            run_volume_click_action(action);
        }
        WM_RBUTTONUP => show_volume_tray_menu(hwnd),
        _ => {}
    }
}

fn handle_volume_tray_left_click(hwnd: HWND) {
    if SUPPRESS_NEXT_VOLUME_TRAY_LBUTTON_UP.swap(false, Ordering::SeqCst) {
        return;
    }
    let clicks = STATE.lock().unwrap().volume.clicks.clone();
    if clicks.double == VolumeClickAction::None {
        run_volume_click_action(clicks.left);
        return;
    }
    let delay = unsafe { GetDoubleClickTime() }.max(1);
    unsafe {
        let _ = KillTimer(hwnd, ID_VOLUME_TRAY_CLICK_TIMER);
        let _ = SetTimer(hwnd, ID_VOLUME_TRAY_CLICK_TIMER, delay, None);
    }
}

fn handle_volume_tray_double_click(hwnd: HWND) {
    let double = STATE.lock().unwrap().volume.clicks.double;
    unsafe {
        let _ = KillTimer(hwnd, ID_VOLUME_TRAY_CLICK_TIMER);
    }
    if double == VolumeClickAction::None {
        return;
    }
    SUPPRESS_NEXT_VOLUME_TRAY_LBUTTON_UP.store(true, Ordering::SeqCst);
    run_volume_click_action(double);
}

fn run_volume_click_action(action: VolumeClickAction) {
    match action {
        VolumeClickAction::None => {}
        VolumeClickAction::Popup => show_volume_popup(),
        VolumeClickAction::ToggleMute => {
            if let Err(err) = toggle_output_mute() {
                eprintln!("failed to toggle output mute: {err:?}");
            }
        }
        VolumeClickAction::OpenMixer => open_volume_mixer(),
        VolumeClickAction::OpenSoundSettings => open_sound_settings(),
        VolumeClickAction::NextOutputDevice => {
            if let Err(err) = cycle_output_device(true) {
                eprintln!("failed to cycle output device: {err:?}");
            }
        }
        VolumeClickAction::OpenSettings => open_volume_settings_window(),
    }
}

fn show_volume_popup() {
    if volume_popup::volume_popup_open() {
        volume_popup::volume_popup_close();
        return;
    }
    // This click already closed the popup by taking focus from it.
    if volume_popup::volume_popup_just_closed() {
        return;
    }
    let hwnd = STATE.lock().unwrap().hwnd;
    use windows::Win32::UI::Shell::{NOTIFYICONIDENTIFIER, Shell_NotifyIconGetRect};
    let identifier = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: ID_VOLUME_TRAY,
        ..Default::default()
    };
    let anchor = unsafe { Shell_NotifyIconGetRect(&identifier) }
        .ok()
        .filter(|rect| rect.right > rect.left && rect.bottom > rect.top)
        .unwrap_or_else(volume_popup::cursor_anchor);
    volume_popup::volume_popup_show(anchor, popup_state(&output_volume_state()));
}

fn open_volume_mixer() {
    if let Err(err) = Command::new("sndvol.exe").spawn() {
        eprintln!("failed to open volume mixer: {err:?}");
    }
}

fn open_sound_settings() {
    if let Err(err) = open_external("ms-settings:sound") {
        eprintln!("failed to open sound settings: {err:?}");
    }
}

fn open_volume_settings_window() {
    if focus_settings_window() {
        return;
    }
    launch_settings_window(Some("--volume"));
}

fn show_volume_tray_menu(hwnd: HWND) {
    let (name_display, muted) = {
        let state = STATE.lock().unwrap();
        (
            state.advanced.audio_device_name_display.clone(),
            state.output_volume.muted,
        )
    };
    let output_devices = render_devices().unwrap_or_default();
    let title = format!("silence! - v{}", env!("CARGO_PKG_VERSION"));
    let title_w = wide(&title);
    let mixer_w = wide("Volume mixer");
    let sound_w = wide("Sound settings");
    let settings_w = wide("Settings");
    let exit_w = wide("Exit");
    let icon_color = if windows_uses_light_system_theme() {
        (20, 20, 20)
    } else {
        (245, 245, 245)
    };

    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let mut bitmaps = Vec::new();
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0001),
            ID_MENU_VOLUME_TITLE,
            PCWSTR(title_w.as_ptr()),
        );
        append_device_menu_item(menu, ID_MENU_VOLUME_MUTE, "Mute output", muted);
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        {
            let mut commands = TRAY_DEVICE_COMMANDS.lock().unwrap();
            commands.clear();
            append_output_device_menu(menu, &output_devices, &name_display, &mut commands);
        }
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_VOLUME_MIXER,
            PCWSTR(mixer_w.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_VOLUME_SOUND,
            PCWSTR(sound_w.as_ptr()),
        );
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_VOLUME_SETTINGS,
            PCWSTR(settings_w.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_VOLUME_EXIT,
            PCWSTR(exit_w.as_ptr()),
        );

        if let Some(bitmap) = create_menu_app_bitmap() {
            let _ = SetMenuItemBitmaps(
                menu,
                ID_MENU_VOLUME_TITLE as u32,
                MENU_ITEM_FLAGS(0),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        for (command_id, svg) in [
            (
                ID_MENU_VOLUME_MUTE,
                include_str!("../../assets/icons/volume-loud-bold.svg"),
            ),
            (
                ID_MENU_VOLUME_MIXER,
                include_str!("../../assets/icons/volume-loud-linear.svg"),
            ),
            (
                ID_MENU_VOLUME_SOUND,
                include_str!("../../assets/icons/tuning-2-linear.svg"),
            ),
            (
                ID_MENU_VOLUME_SETTINGS,
                include_str!("../../assets/icons/settings-bold.svg"),
            ),
            (
                ID_MENU_VOLUME_EXIT,
                include_str!("../../assets/icons/exit-bold.svg"),
            ),
        ] {
            if let Some(bitmap) = create_menu_svg_bitmap(svg, icon_color) {
                let _ = SetMenuItemBitmaps(
                    menu,
                    command_id as u32,
                    MENU_ITEM_FLAGS(0),
                    bitmap,
                    bitmap,
                );
                bitmaps.push(bitmap);
            }
        }

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let command_id = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN | TPM_RETURNCMD,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        if command_id.0 != 0 {
            handle_volume_tray_menu_command(command_id.0 as usize);
        }
        let _ = DestroyMenu(menu);
        TRAY_DEVICE_COMMANDS.lock().unwrap().clear();
        for bitmap in bitmaps {
            let _ = DeleteObject(bitmap);
        }
    }
}

fn handle_volume_tray_menu_command(command_id: usize) {
    match command_id {
        ID_MENU_VOLUME_MUTE => {
            if let Err(err) = toggle_output_mute() {
                eprintln!("failed to toggle output mute: {err:?}");
            }
        }
        ID_MENU_VOLUME_MIXER => open_volume_mixer(),
        ID_MENU_VOLUME_SOUND => open_sound_settings(),
        ID_MENU_VOLUME_SETTINGS => open_volume_settings_window(),
        ID_MENU_VOLUME_EXIT => exit_all_processes(),
        command_id => handle_tray_device_command(command_id),
    }
}
