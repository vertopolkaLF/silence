fn add_tray_icon(hwnd: HWND) -> Result<()> {
    if TRAY_ICON_ADDED.load(Ordering::Relaxed) {
        return Ok(());
    }

    let (config, muted, mic_in_use) = {
        let state = STATE.lock().unwrap();
        (state.tray_icon.clone(), state.muted, state.mic_in_use)
    };
    let icon = load_tray_icon(&config, muted, mic_in_use)
        .or_else(load_app_icon)
        .or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION).ok() });
    let icon = icon.context("load tray icon")?;
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_TRAY,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    write_packed_wide_buf(std::ptr::addr_of_mut!(nid.szTip), "silence!");
    unsafe {
        if Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
            TRAY_ICON_ADDED.store(true, Ordering::Relaxed);
            let _ = KillTimer(hwnd, ID_TRAY_ADD_RETRY_TIMER);
        } else {
            let _ = SetTimer(hwnd, ID_TRAY_ADD_RETRY_TIMER, TRAY_ADD_RETRY_MS, None);
            return Ok(());
        }
    }
    refresh_tray_icon();
    Ok(())
}

fn refresh_tray_icon() {
    if !TRAY_ICON_ADDED.load(Ordering::Relaxed) {
        return;
    }

    let (hwnd, muted, mic_in_use, config) = {
        let state = STATE.lock().unwrap();
        if state.hwnd.0.is_null() {
            return;
        }
        (
            state.hwnd,
            state.muted,
            state.mic_in_use,
            state.tray_icon.clone(),
        )
    };
    let Some(icon) = load_tray_icon(&config, muted, mic_in_use).or_else(load_app_icon) else {
        refresh_tray_tip();
        return;
    };
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_TRAY,
        uFlags: NIF_ICON,
        hIcon: icon,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
    refresh_tray_tip();
}

fn load_tray_icon(config: &TrayIconConfig, muted: bool, mic_in_use: bool) -> Option<HICON> {
    match config.variant.as_str() {
        "StatusMic" => create_status_mic_icon(config, muted, mic_in_use),
        "ColorDot" => create_color_dot_icon(config, muted, mic_in_use),
        _ => create_badged_app_icon(config, mic_in_use).or_else(load_app_icon),
    }
}

fn load_app_icon() -> Option<HICON> {
    let icon_bytes = include_bytes!("../../assets/app.ico");
    let image = best_ico_image(icon_bytes, 16)?;
    unsafe {
        CreateIconFromResourceEx(image, true, ICON_RESOURCE_VERSION, 0, 0, LR_DEFAULTSIZE).ok()
    }
}

fn create_badged_app_icon(config: &TrayIconConfig, mic_in_use: bool) -> Option<HICON> {
    if !config.show_mic_in_use || !mic_in_use {
        return load_app_icon();
    }

    let icon = load_app_icon()?;
    let mut pixels = icon_pixels(icon, 32, 32)?;
    draw_mic_in_use_badge(&mut pixels, 32, config, mic_in_use);
    create_argb_icon(32, 32, &pixels)
}

fn icon_pixels(icon: HICON, width: i32, height: i32) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }

    let mut bits: *mut c_void = null_mut();
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let Ok(bitmap) = CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(hdc);
            return None;
        };
        let previous = SelectObject(hdc, bitmap);
        let drawn = DrawIconEx(hdc, 0, 0, icon, width, height, 0, None, DI_NORMAL).is_ok();
        let byte_len = (width * height * 4) as usize;
        let pixels = if drawn && !bits.is_null() {
            Some(std::slice::from_raw_parts(bits as *const u8, byte_len).to_vec())
        } else {
            None
        };
        let _ = SelectObject(hdc, previous);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(hdc);
        pixels
    }
}

fn create_status_mic_icon(config: &TrayIconConfig, muted: bool, mic_in_use: bool) -> Option<HICON> {
    let color = match config.status_style.as_str() {
        "Monochrome" => {
            if windows_uses_light_system_theme() {
                (0, 0, 0)
            } else {
                (245, 245, 245)
            }
        }
        "SystemColor" => WindowsAccent::load().accent,
        _ => state_accent(muted),
    };
    let mask = fit_alpha_mask(
        &render_svg_alpha(
            crate::overlay_icons::overlay_icon_svg(&config.icon_pair, muted),
            64,
        )?,
        64,
        64,
        32,
        30,
    )?;
    let mut pixels = vec![0u8; 32 * 32 * 4];
    for (index, alpha) in mask.into_iter().enumerate() {
        let offset = index * 4;
        pixels[offset] = color.2;
        pixels[offset + 1] = color.1;
        pixels[offset + 2] = color.0;
        pixels[offset + 3] = alpha;
    }
    draw_mic_in_use_badge(&mut pixels, 32, config, mic_in_use);
    create_argb_icon(32, 32, &pixels)
}

fn create_color_dot_icon(config: &TrayIconConfig, muted: bool, mic_in_use: bool) -> Option<HICON> {
    let color = state_accent(muted);
    let size = 32usize;
    let center = (size as f64 - 1.0) / 2.0;
    let radius = 13.25;
    let feather = 1.25;
    let mut pixels = vec![0u8; size * size * 4];
    for y in 0..size {
        for x in 0..size {
            let distance = ((x as f64 - center).powi(2) + (y as f64 - center).powi(2)).sqrt();
            let alpha = ((radius + feather - distance) / feather).clamp(0.0, 1.0);
            let offset = (y * size + x) * 4;
            pixels[offset] = color.2;
            pixels[offset + 1] = color.1;
            pixels[offset + 2] = color.0;
            pixels[offset + 3] = (alpha * 255.0).round() as u8;
        }
    }
    draw_mic_in_use_badge(&mut pixels, size, config, mic_in_use);
    create_argb_icon(size as i32, size as i32, &pixels)
}

fn draw_mic_in_use_badge(
    pixels: &mut [u8],
    size: usize,
    config: &TrayIconConfig,
    mic_in_use: bool,
) {
    if !config.show_mic_in_use || !mic_in_use || size == 0 || pixels.len() < size * size * 4 {
        return;
    }

    let light_theme = windows_uses_light_system_theme();
    let accent = if config.status_style == "SystemColor" {
        if light_theme { (0, 0, 0) } else { (255, 255, 255) }
    } else {
        WindowsAccent::load().accent
    };
    let backing = if light_theme {
        (255, 255, 255)
    } else {
        (18, 18, 18)
    };
    let center_x = size as f64 - 6.5;
    let center_y = size as f64 - 6.5;
    let ring_radius = 7.75;
    let dot_radius = 5.5;

    for y in 0..size {
        for x in 0..size {
            let distance = ((x as f64 + 0.5 - center_x).powi(2)
                + (y as f64 + 0.5 - center_y).powi(2))
            .sqrt();
            let color = if distance <= dot_radius {
                Some(accent)
            } else if distance <= ring_radius {
                Some(backing)
            } else {
                None
            };
            if let Some((r, g, b)) = color {
                let offset = (y * size + x) * 4;
                pixels[offset] = b;
                pixels[offset + 1] = g;
                pixels[offset + 2] = r;
                pixels[offset + 3] = 255;
            }
        }
    }
}

fn render_svg_alpha(svg: &str, size: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default()).ok()?;
    let svg_size = tree.size().to_int_size();
    let scale = (size as f32 / svg_size.width() as f32).min(size as f32 / svg_size.height() as f32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .take_demultiplied()
            .chunks_exact(4)
            .map(|pixel| pixel[3])
            .collect(),
    )
}

fn fit_alpha_mask(
    mask: &[u8],
    source_width: usize,
    source_height: usize,
    target_size: usize,
    content_size: usize,
) -> Option<Vec<u8>> {
    if source_width == 0
        || source_height == 0
        || target_size == 0
        || content_size == 0
        || mask.len() < source_width * source_height
    {
        return None;
    }

    let mut min_x = source_width;
    let mut min_y = source_height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    for y in 0..source_height {
        for x in 0..source_width {
            if mask[y * source_width + x] == 0 {
                continue;
            }
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if min_x > max_x || min_y > max_y {
        return Some(vec![0; target_size * target_size]);
    }

    let bounds_width = max_x - min_x + 1;
    let bounds_height = max_y - min_y + 1;
    let fitted_width = if bounds_width >= bounds_height {
        content_size
    } else {
        ((bounds_width as f64 / bounds_height as f64) * content_size as f64)
            .round()
            .max(1.0) as usize
    };
    let fitted_height = if bounds_height >= bounds_width {
        content_size
    } else {
        ((bounds_height as f64 / bounds_width as f64) * content_size as f64)
            .round()
            .max(1.0) as usize
    };
    let offset_x = (target_size.saturating_sub(fitted_width)) / 2;
    let offset_y = (target_size.saturating_sub(fitted_height)) / 2;
    let mut fitted = vec![0; target_size * target_size];

    for y in 0..fitted_height {
        for x in 0..fitted_width {
            let source_x = min_x
                + ((x as f64 + 0.5) * bounds_width as f64 / fitted_width as f64)
                    .floor()
                    .min((bounds_width - 1) as f64) as usize;
            let source_y = min_y
                + ((y as f64 + 0.5) * bounds_height as f64 / fitted_height as f64)
                    .floor()
                    .min((bounds_height - 1) as f64) as usize;
            fitted[(offset_y + y) * target_size + offset_x + x] =
                mask[source_y * source_width + source_x];
        }
    }

    Some(fitted)
}

fn create_argb_icon(width: i32, height: i32, pixels: &[u8]) -> Option<HICON> {
    let and_mask = vec![0u8; ((width * height) / 8).max(1) as usize];
    unsafe {
        CreateIcon(
            None,
            width,
            height,
            1,
            32,
            and_mask.as_ptr(),
            pixels.as_ptr(),
        )
        .ok()
    }
}

fn create_argb_bitmap(width: i32, height: i32, pixels: &[u8]) -> Option<HBITMAP> {
    if width <= 0 || height <= 0 || pixels.len() < (width * height * 4) as usize {
        return None;
    }

    let mut bits: *mut c_void = null_mut();
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width,
        biHeight: -height,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()? };
    if bits.is_null() {
        unsafe {
            let _ = DeleteObject(bitmap);
        }
        return None;
    }

    unsafe {
        let target = std::slice::from_raw_parts_mut(bits as *mut u8, (width * height * 4) as usize);
        for (source, target) in pixels.chunks_exact(4).zip(target.chunks_exact_mut(4)) {
            let alpha = source[3] as u16;
            target[0] = ((source[0] as u16 * alpha) / 255) as u8;
            target[1] = ((source[1] as u16 * alpha) / 255) as u8;
            target[2] = ((source[2] as u16 * alpha) / 255) as u8;
            target[3] = source[3];
        }
    }
    Some(bitmap)
}

fn create_menu_app_bitmap() -> Option<HBITMAP> {
    let icon = load_app_icon()?;
    let size = 16;
    let mut bits: *mut c_void = null_mut();
    let mut info = BITMAPINFO::default();
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size,
        biHeight: -size,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };

    unsafe {
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        if bits.is_null() {
            let _ = DeleteObject(bitmap);
            return None;
        }

        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            let _ = DeleteObject(bitmap);
            return None;
        }

        let old_bitmap = SelectObject(hdc, bitmap);
        std::ptr::write_bytes(bits as *mut u8, 0, (size * size * 4) as usize);
        let _ = DrawIconEx(hdc, 0, 0, icon, size, size, 0, None, DI_NORMAL);
        let _ = SelectObject(hdc, old_bitmap);
        let _ = DeleteDC(hdc);
        Some(bitmap)
    }
}

fn create_menu_svg_bitmap(svg: &str, color: (u8, u8, u8)) -> Option<HBITMAP> {
    let size = 16usize;
    let mask = render_svg_alpha(svg, 64)?;
    let mask = fit_alpha_mask(&mask, 64, 64, size, 14)?;
    let mut pixels = vec![0u8; size * size * 4];
    for (index, alpha) in mask.into_iter().enumerate() {
        let offset = index * 4;
        pixels[offset] = color.2;
        pixels[offset + 1] = color.1;
        pixels[offset + 2] = color.0;
        pixels[offset + 3] = alpha;
    }
    create_argb_bitmap(size as i32, size as i32, &pixels)
}

fn best_ico_image(bytes: &[u8], target: u32) -> Option<&[u8]> {
    if bytes.len() < 6 || u16::from_le_bytes([bytes[2], bytes[3]]) != 1 {
        return None;
    }

    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    let mut best: Option<(u32, usize, usize)> = None;
    for index in 0..count {
        let offset = 6 + index * 16;
        if offset + 16 > bytes.len() {
            return None;
        }

        let width = if bytes[offset] == 0 {
            256
        } else {
            bytes[offset] as u32
        };
        let size = u32::from_le_bytes([
            bytes[offset + 8],
            bytes[offset + 9],
            bytes[offset + 10],
            bytes[offset + 11],
        ]) as usize;
        let image_offset = u32::from_le_bytes([
            bytes[offset + 12],
            bytes[offset + 13],
            bytes[offset + 14],
            bytes[offset + 15],
        ]) as usize;
        if image_offset + size > bytes.len() {
            continue;
        }

        let score = width.abs_diff(target);
        if best
            .map(|(best_score, best_size, _)| {
                score < best_score || (score == best_score && size > best_size)
            })
            .unwrap_or(true)
        {
            best = Some((score, size, image_offset));
        }
    }

    let (_, size, image_offset) = best?;
    Some(&bytes[image_offset..image_offset + size])
}

fn refresh_tray_tip() {
    let state = STATE.lock().unwrap();
    if state.hwnd.0.is_null() {
        return;
    }
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: state.hwnd,
        uID: ID_TRAY,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    let primary_shortcut = state
        .hotkeys
        .iter()
        .find(|hotkey| hotkey.action == HotkeyAction::ToggleMute)
        .map(|hotkey| hotkey.shortcut.display())
        .unwrap_or_else(|| "no hotkey".to_string());
    let tip = if state.muted {
        format!("silence!: microphone muted ({primary_shortcut})")
    } else {
        format!("silence!: microphone on ({primary_shortcut})")
    };
    write_packed_wide_buf(std::ptr::addr_of_mut!(nid.szTip), &tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn remove_tray_icon() {
    let state = STATE.lock().unwrap();
    if state.hwnd.0.is_null() {
        return;
    }
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: state.hwnd,
        uID: ID_TRAY,
        ..Default::default()
    };
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
    TRAY_ICON_ADDED.store(false, Ordering::Relaxed);
}

fn tray_double_click_disabled() -> bool {
    STATE
        .lock()
        .unwrap()
        .advanced
        .disable_tray_double_click_settings
}

fn handle_tray_left_click(hwnd: HWND) {
    if SUPPRESS_NEXT_TRAY_LBUTTON_UP.swap(false, Ordering::SeqCst) {
        return;
    }

    if tray_double_click_disabled() {
        toggle_mute();
        return;
    }

    unsafe {
        let _ = KillTimer(hwnd, ID_TRAY_CLICK_TIMER);
        let _ = SetTimer(hwnd, ID_TRAY_CLICK_TIMER, TRAY_DOUBLE_CLICK_DELAY_MS, None);
    }
}

fn handle_tray_double_click(hwnd: HWND) {
    unsafe {
        let _ = KillTimer(hwnd, ID_TRAY_CLICK_TIMER);
    }

    if !tray_double_click_disabled() {
        SUPPRESS_NEXT_TRAY_LBUTTON_UP.store(true, Ordering::SeqCst);
        open_settings_window();
    }
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == *TASKBAR_CREATED_MESSAGE {
        TRAY_ICON_ADDED.store(false, Ordering::Relaxed);
        let _ = add_tray_icon(hwnd);
        return LRESULT(0);
    }

    match msg {
        WM_TRAY => {
            match lparam.0 as u32 {
                WM_RBUTTONUP => show_tray_menu(hwnd),
                WM_LBUTTONUP => handle_tray_left_click(hwnd),
                WM_LBUTTONDBLCLK => handle_tray_double_click(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            handle_tray_menu_command(wparam.0 & 0xffff);
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == ID_STATE_TIMER {
                refresh_runtime_state();
            } else if wparam.0 == ID_OVERLAY_HIDE_TIMER {
                let _ = unsafe { KillTimer(hwnd, ID_OVERLAY_HIDE_TIMER) };
                apply_overlay_visibility();
            } else if wparam.0 == ID_OVERLAY_DRAG_TIMER {
                if let Some((x, y)) = native_overlay::process_drag() {
                    save_overlay_position(x, y);
                }
            } else if wparam.0 == ID_TRAY_CLICK_TIMER {
                let _ = unsafe { KillTimer(hwnd, ID_TRAY_CLICK_TIMER) };
                toggle_mute();
            } else if wparam.0 == ID_TRAY_ADD_RETRY_TIMER {
                let _ = add_tray_icon(hwnd);
            }
            LRESULT(0)
        }
        WM_TOGGLE_MUTE => {
            toggle_mute();
            LRESULT(0)
        }
        WM_MUTE => {
            set_mute_target(None, true);
            LRESULT(0)
        }
        WM_UNMUTE => {
            set_mute_target(None, false);
            LRESULT(0)
        }
        WM_OPEN_SETTINGS => {
            launch_settings_window(Some("--about"));
            LRESULT(0)
        }
        WM_UPDATE_NOW => {
            launch_settings_window(Some("--about-update"));
            LRESULT(0)
        }
        WM_EXIT_ALL => {
            exit_all_processes();
            LRESULT(0)
        }
        WM_WHATS_NEW => {
            open_last_update_release();
            LRESULT(0)
        }
        WM_AUDIO_MUTE_STATE_CHANGED => {
            refresh_mute_state();
            LRESULT(0)
        }
        WM_AUDIO_ENDPOINT_CHANGED => {
            ensure_audio_notification_registration();
            refresh_mute_state();
            LRESULT(0)
        }
        WM_OVERLAY_POSITIONING => {
            let active = wparam.0 != 0;
            if let Some((x, y)) = native_overlay::set_positioning(active) {
                save_overlay_position(x, y);
            }
            unsafe {
                if active {
                    let _ = KillTimer(hwnd, ID_OVERLAY_HIDE_TIMER);
                    let _ = SetTimer(hwnd, ID_OVERLAY_DRAG_TIMER, 16, None);
                } else {
                    let _ = KillTimer(hwnd, ID_OVERLAY_DRAG_TIMER);
                    apply_overlay_visibility();
                }
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE => {
            native_overlay::reposition();
            LRESULT(0)
        }
        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            refresh_tray_icon();
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn show_tray_menu(hwnd: HWND) {
    unsafe {
        let menu = CreatePopupMenu().unwrap_or_default();
        let input_menu = CreatePopupMenu().unwrap_or_default();
        let output_menu = CreatePopupMenu().unwrap_or_default();
        let mic_apps_menu = CreatePopupMenu().unwrap_or_default();
        let (muted, ungroup_devices, device_name_display, update_available) = {
            let state = STATE.lock().unwrap();
            (
                state.muted,
                state.advanced.ungroup_tray_devices,
                state.advanced.audio_device_name_display.clone(),
                state.available_update.is_some(),
            )
        };
        let status = if muted {
            "Unmute Microphone"
        } else {
            "Mute Microphone"
        };
        let title_w = wide(&format!("silence! - v{}", env!("CARGO_PKG_VERSION")));
        let status_w = wide(status);
        let output_w = wide("Default Output");
        let input_w = wide("Default Input");
        let mic_apps_w = wide("Apps that use mic");
        let settings_w = wide("Open Settings");
        let install_update_w = wide("Install Update");
        let exit_w = wide("Exit");
        let input_devices = capture_devices().unwrap_or_default();
        let output_devices = render_devices().unwrap_or_default();
        let mic_apps = mic_using_apps().unwrap_or_default();

        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0001),
            ID_MENU_TITLE,
            PCWSTR(title_w.as_ptr()),
        );
        if update_available {
            let _ = AppendMenuW(
                menu,
                MENU_ITEM_FLAGS(0),
                ID_MENU_INSTALL_UPDATE,
                PCWSTR(install_update_w.as_ptr()),
            );
        }
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));

        {
            let mut commands = TRAY_DEVICE_COMMANDS.lock().unwrap();
            commands.clear();
            if ungroup_devices {
                append_output_device_menu(
                    menu,
                    &output_devices,
                    &device_name_display,
                    &mut commands,
                );
                let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
                append_input_device_menu(menu, &input_devices, &device_name_display, &mut commands);
            } else {
                append_output_device_menu(
                    output_menu,
                    &output_devices,
                    &device_name_display,
                    &mut commands,
                );
                append_input_device_menu(
                    input_menu,
                    &input_devices,
                    &device_name_display,
                    &mut commands,
                );
            }
        }

        if !ungroup_devices {
            let _ = AppendMenuW(
                menu,
                MENU_ITEM_FLAGS(0x0000_0010),
                output_menu.0 as usize,
                PCWSTR(output_w.as_ptr()),
            );
            let _ = AppendMenuW(
                menu,
                MENU_ITEM_FLAGS(0x0000_0010),
                input_menu.0 as usize,
                PCWSTR(input_w.as_ptr()),
            );
            let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        }

        if !mic_apps.is_empty() {
            {
                let mut commands = TRAY_DEVICE_COMMANDS.lock().unwrap();
                append_mic_apps_menu(mic_apps_menu, &mic_apps, &mut commands);
            }
            if ungroup_devices {
                let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
            }
            let _ = AppendMenuW(
                menu,
                MENU_ITEM_FLAGS(0x0000_0010),
                mic_apps_menu.0 as usize,
                PCWSTR(mic_apps_w.as_ptr()),
            );
        }

        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_TOGGLE,
            PCWSTR(status_w.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_SETTINGS,
            PCWSTR(settings_w.as_ptr()),
        );
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_EXIT,
            PCWSTR(exit_w.as_ptr()),
        );

        let icon_color = if windows_uses_light_system_theme() {
            (20, 20, 20)
        } else {
            (245, 245, 245)
        };
        let default_output_pos = MENU_POS_DEFAULT_OUTPUT + u32::from(update_available);
        let default_input_pos = MENU_POS_DEFAULT_INPUT + u32::from(update_available);
        let mut bitmaps = Vec::new();
        if let Some(bitmap) = create_menu_app_bitmap() {
            let _ = SetMenuItemBitmaps(
                menu,
                ID_MENU_TITLE as u32,
                MENU_ITEM_FLAGS(0),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        if update_available {
            if let Some(bitmap) = create_menu_svg_bitmap(
                include_str!("../../assets/icons/download-minimalistic-bold.svg"),
                icon_color,
            ) {
                let _ = SetMenuItemBitmaps(
                    menu,
                    ID_MENU_INSTALL_UPDATE as u32,
                    MENU_ITEM_FLAGS(0),
                    bitmap,
                    bitmap,
                );
                bitmaps.push(bitmap);
            }
        }
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../../assets/icons/microphone-3-bold.svg"),
            state_accent(muted),
        ) {
            let _ = SetMenuItemBitmaps(
                menu,
                ID_MENU_TOGGLE as u32,
                MENU_ITEM_FLAGS(0),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        if !ungroup_devices {
            if let Some(bitmap) = create_menu_svg_bitmap(
                include_str!("../../assets/icons/volume-loud-linear.svg"),
                icon_color,
            ) {
                let _ = SetMenuItemBitmaps(
                    menu,
                    default_output_pos,
                    MENU_ITEM_FLAGS(0x0000_0400),
                    bitmap,
                    bitmap,
                );
                bitmaps.push(bitmap);
            }
            if let Some(bitmap) = create_menu_svg_bitmap(
                include_str!("../../assets/icons/microphone-3-linear.svg"),
                icon_color,
            ) {
                let _ = SetMenuItemBitmaps(
                    menu,
                    default_input_pos,
                    MENU_ITEM_FLAGS(0x0000_0400),
                    bitmap,
                    bitmap,
                );
                bitmaps.push(bitmap);
            }
        }
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../../assets/icons/settings-bold.svg"),
            icon_color,
        ) {
            let _ = SetMenuItemBitmaps(
                menu,
                ID_MENU_SETTINGS as u32,
                MENU_ITEM_FLAGS(0),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        if let Some(bitmap) =
            create_menu_svg_bitmap(include_str!("../../assets/icons/exit-bold.svg"), icon_color)
        {
            let _ = SetMenuItemBitmaps(
                menu,
                ID_MENU_EXIT as u32,
                MENU_ITEM_FLAGS(0),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
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
            handle_tray_menu_command(command_id.0 as usize);
        }
        let _ = DestroyMenu(menu);
        if ungroup_devices {
            let _ = DestroyMenu(output_menu);
            let _ = DestroyMenu(input_menu);
        }
        if mic_apps.is_empty() {
            let _ = DestroyMenu(mic_apps_menu);
        }
        TRAY_DEVICE_COMMANDS.lock().unwrap().clear();
        for bitmap in bitmaps {
            let _ = DeleteObject(bitmap);
        }
    }
}

fn append_input_device_menu(
    menu: HMENU,
    devices: &[MicDevice],
    name_display: &str,
    commands: &mut HashMap<usize, TrayDeviceCommand>,
) {
    if devices.is_empty() {
        append_disabled_menu_item(menu, "No active input devices");
        return;
    }

    for (index, device) in devices.iter().enumerate() {
        let command_id = ID_MENU_INPUT_DEVICE_BASE + index;
        commands.insert(command_id, TrayDeviceCommand::Input(device.id.clone()));
        append_device_menu_item(
            menu,
            command_id,
            &device.display_name(name_display),
            device.is_default,
        );
    }
}

fn append_output_device_menu(
    menu: HMENU,
    devices: &[AudioDevice],
    name_display: &str,
    commands: &mut HashMap<usize, TrayDeviceCommand>,
) {
    if devices.is_empty() {
        append_disabled_menu_item(menu, "No active output devices");
        return;
    }

    for (index, device) in devices.iter().enumerate() {
        let command_id = ID_MENU_OUTPUT_DEVICE_BASE + index;
        commands.insert(command_id, TrayDeviceCommand::Output(device.id.clone()));
        append_device_menu_item(
            menu,
            command_id,
            &device.display_name(name_display),
            device.is_default,
        );
    }
}

fn append_mic_apps_menu(
    menu: HMENU,
    apps: &[MicUsingApp],
    commands: &mut HashMap<usize, TrayDeviceCommand>,
) {
    let mut name_counts = HashMap::<&str, usize>::new();
    for app in apps {
        *name_counts.entry(app.name.as_str()).or_default() += 1;
    }

    for (index, app) in apps.iter().enumerate() {
        let command_id = ID_MENU_MIC_APP_BASE + index;
        commands.insert(command_id, TrayDeviceCommand::MicApp(app.pid));
        let label = if name_counts.get(app.name.as_str()).copied().unwrap_or(0) > 1 {
            format!("{} ({})", app.name, app.pid)
        } else {
            app.name.clone()
        };
        append_device_menu_item(menu, command_id, &label, false);
    }
}

fn append_device_menu_item(menu: HMENU, command_id: usize, label: &str, checked: bool) {
    let label_w = wide(label);
    let flags = if checked {
        MENU_ITEM_FLAGS(0x0000_0008)
    } else {
        MENU_ITEM_FLAGS(0)
    };
    unsafe {
        let _ = AppendMenuW(menu, flags, command_id, PCWSTR(label_w.as_ptr()));
    }
}

fn append_disabled_menu_item(menu: HMENU, label: &str) {
    let label_w = wide(label);
    unsafe {
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0002 | 0x0000_0001),
            0,
            PCWSTR(label_w.as_ptr()),
        );
    }
}

fn handle_tray_device_command(command_id: usize) {
    let command = TRAY_DEVICE_COMMANDS
        .lock()
        .unwrap()
        .get(&command_id)
        .cloned();
    match command {
        Some(TrayDeviceCommand::Input(device_id)) => {
            if let Err(err) = set_default_capture_device(&device_id) {
                eprintln!("failed to set default input device: {err:?}");
            }
        }
        Some(TrayDeviceCommand::Output(device_id)) => {
            if let Err(err) = set_default_render_device(&device_id) {
                eprintln!("failed to set default output device: {err:?}");
            }
        }
        Some(TrayDeviceCommand::MicApp(pid)) => {
            if !focus_process_window(pid) {
                eprintln!("failed to focus microphone app process {pid}");
            }
        }
        None => {}
    }
}

fn handle_tray_menu_command(command_id: usize) {
    match command_id {
        ID_MENU_TOGGLE => toggle_mute(),
        ID_MENU_SETTINGS => open_settings_window(),
        ID_MENU_INSTALL_UPDATE => run_update_now_action(),
        ID_MENU_EXIT => {
            exit_all_processes();
        }
        command_id => handle_tray_device_command(command_id),
    }
}

fn focus_process_window(pid: u32) -> bool {
    #[derive(Default)]
    struct WindowSearch {
        pid: u32,
        hwnd: HWND,
    }

    unsafe extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = unsafe { &mut *(lparam.0 as *mut WindowSearch) };
        if !unsafe { IsWindowVisible(hwnd).as_bool() } {
            return BOOL(1);
        }

        let mut window_pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(hwnd, Some(&mut window_pid));
        }
        if window_pid != search.pid {
            return BOOL(1);
        }

        search.hwnd = hwnd;
        BOOL(0)
    }

    let mut search = WindowSearch {
        pid,
        hwnd: HWND(null_mut()),
    };
    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM((&mut search as *mut WindowSearch) as isize),
        );
        if search.hwnd.0.is_null() {
            return false;
        }
        if IsIconic(search.hwnd).as_bool() {
            let _ = ShowWindow(search.hwnd, SW_RESTORE);
        } else {
            let _ = ShowWindow(search.hwnd, SW_SHOW);
        }
        SetForegroundWindow(search.hwnd).as_bool()
    }
}

fn open_settings_window() {
    if focus_settings_window() {
        return;
    }

    launch_settings_window(None);
}

fn launch_settings_window(tab_arg: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    if tab_arg == Some("--about-update") {
        close_settings_window();
    }
    let mut command = Command::new(exe);
    command.arg("--settings");
    if let Some(tab_arg) = tab_arg {
        command.arg(tab_arg);
    }
    let _ = command.spawn();
}

fn focus_settings_window() -> bool {
    let title = wide(SETTINGS_WINDOW_TITLE);
    let hwnd = unsafe { FindWindowW(PCWSTR(null()), PCWSTR(title.as_ptr())) };
    let Ok(hwnd) = hwnd else {
        return false;
    };

    if hwnd.0.is_null() {
        return false;
    }

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

fn close_settings_window() {
    let title = wide(SETTINGS_WINDOW_TITLE);
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR(null()), PCWSTR(title.as_ptr())) }) else {
        return;
    };
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = SendMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn request_exit_all_processes() {
    if dispatch_notification_action(NotificationAction::ExitAll) {
        return;
    }
    exit_all_processes();
}
