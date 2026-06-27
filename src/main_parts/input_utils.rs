fn cleanup() {
    shutdown_audio_notification_registration();
    native_overlay::destroy();
    remove_tray_icon();
    let (hook, mouse_hook) = {
        let state = STATE.lock().unwrap();
        (state.hook, state.mouse_hook)
    };
    if !hook.0.is_null() {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
    if !mouse_hook.0.is_null() {
        unsafe {
            let _ = UnhookWindowsHookEx(mouse_hook);
        }
    }
}

fn key_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn is_modifier(vk: u32) -> bool {
    modifier_kind(vk).is_some()
}

fn update_modifier_state(modifiers: &mut ModifierState, vk: u32, pressed: bool) {
    match modifier_kind(vk) {
        Some(ModifierKind::Ctrl) => modifiers.ctrl = pressed,
        Some(ModifierKind::Alt) => modifiers.alt = pressed,
        Some(ModifierKind::Shift) => modifiers.shift = pressed,
        Some(ModifierKind::Win) => modifiers.win = pressed,
        None => {}
    }
}

fn current_modifier_state() -> ModifierState {
    ModifierState {
        ctrl: key_down(VK_CONTROL) || key_down(0xA2) || key_down(0xA3),
        alt: key_down(VK_MENU) || key_down(0xA4) || key_down(0xA5),
        shift: key_down(VK_SHIFT) || key_down(0xA0) || key_down(0xA1),
        win: key_down(VK_LWIN) || key_down(VK_RWIN),
    }
}

#[derive(Clone, Copy)]
enum ModifierKind {
    Ctrl,
    Alt,
    Shift,
    Win,
}

fn modifier_kind(vk: u32) -> Option<ModifierKind> {
    match vk {
        VK_SHIFT | 0xA0 | 0xA1 => Some(ModifierKind::Shift),
        VK_CONTROL | 0xA2 | 0xA3 => Some(ModifierKind::Ctrl),
        VK_MENU | 0xA4 | 0xA5 => Some(ModifierKind::Alt),
        VK_LWIN | VK_RWIN => Some(ModifierKind::Win),
        _ => None,
    }
}

fn mouse_button_from_event(event: u32, mouse_data: u32) -> Option<u32> {
    match event {
        WM_LBUTTONDOWN | WM_LBUTTONUP => Some(VK_LBUTTON),
        WM_RBUTTONDOWN | WM_RBUTTONUP => Some(VK_RBUTTON),
        WM_MBUTTONDOWN | WM_MBUTTONUP => Some(VK_MBUTTON),
        WM_XBUTTONDOWN | WM_XBUTTONUP => match (mouse_data >> 16) & 0xffff {
            XBUTTON1 => Some(VK_XBUTTON1),
            XBUTTON2 => Some(VK_XBUTTON2),
            _ => None,
        },
        _ => None,
    }
}

fn mouse_button_event_is_down(event: u32) -> bool {
    matches!(
        event,
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN | WM_XBUTTONDOWN
    )
}

fn mouse_button_sort_key(button: u32) -> u32 {
    match button {
        VK_LBUTTON => 0,
        VK_RBUTTON => 1,
        VK_MBUTTON => 2,
        VK_XBUTTON1 => 3,
        VK_XBUTTON2 => 4,
        _ => button + 100,
    }
}

pub(crate) fn mouse_button_name(button: u32) -> &'static str {
    match button {
        VK_LBUTTON => "Left Click",
        VK_RBUTTON => "Right Click",
        VK_MBUTTON => "Middle Click",
        VK_XBUTTON1 => "Mouse 4",
        VK_XBUTTON2 => "Mouse 5",
        _ => "Mouse",
    }
}

fn is_supported_mouse_button(button: u32) -> bool {
    matches!(
        button,
        VK_LBUTTON | VK_RBUTTON | VK_MBUTTON | VK_XBUTTON1 | VK_XBUTTON2
    )
}

pub(crate) fn vk_from_keyboard_code(code: &str) -> Option<u32> {
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
        if let Some(vk) = digit
            .as_bytes()
            .first()
            .filter(|byte| byte.is_ascii_digit())
            .map(|byte| VK_NUMPAD0 + (*byte - b'0') as u32)
        {
            return Some(vk);
        }
    }
    if let Some(number) = code.strip_prefix('F') {
        let n = number.parse::<u32>().ok()?;
        if (1..=24).contains(&n) {
            return Some(VK_F1 + n - 1);
        }
    }
    match code {
        "Backspace" => Some(0x08),
        "Tab" => Some(0x09),
        "Enter" => Some(0x0D),
        "ShiftLeft" | "ShiftRight" | "Shift" => Some(VK_SHIFT),
        "ControlLeft" | "ControlRight" | "Control" => Some(VK_CONTROL),
        "AltLeft" | "AltRight" | "Alt" => Some(VK_MENU),
        "Pause" => Some(0x13),
        "CapsLock" => Some(0x14),
        "Escape" => Some(0x1B),
        "Space" => Some(0x20),
        "PageUp" => Some(0x21),
        "PageDown" => Some(0x22),
        "End" => Some(0x23),
        "Home" => Some(0x24),
        "ArrowLeft" => Some(0x25),
        "ArrowUp" => Some(0x26),
        "ArrowRight" => Some(0x27),
        "ArrowDown" => Some(0x28),
        "PrintScreen" => Some(0x2C),
        "Insert" => Some(0x2D),
        "Delete" => Some(0x2E),
        "MetaLeft" | "MetaRight" | "Meta" => Some(VK_LWIN),
        "ContextMenu" => Some(0x5D),
        "NumpadMultiply" => Some(0x6A),
        "NumpadAdd" => Some(0x6B),
        "NumpadComma" | "NumpadDecimal" => Some(0x6E),
        "NumpadSubtract" => Some(0x6D),
        "NumpadDivide" => Some(0x6F),
        "NumLock" => Some(0x90),
        "ScrollLock" => Some(0x91),
        "Semicolon" => Some(0xBA),
        "Equal" => Some(0xBB),
        "Comma" => Some(0xBC),
        "Minus" => Some(0xBD),
        "Period" => Some(0xBE),
        "Slash" => Some(0xBF),
        "Backquote" => Some(0xC0),
        "BracketLeft" => Some(0xDB),
        "Backslash" => Some(0xDC),
        "BracketRight" => Some(0xDD),
        "Quote" => Some(0xDE),
        "IntlBackslash" | "IntlRo" | "IntlYen" => Some(0xE2),
        "BrowserBack" => Some(0xA6),
        "BrowserForward" => Some(0xA7),
        "BrowserRefresh" => Some(0xA8),
        "BrowserStop" => Some(0xA9),
        "BrowserSearch" => Some(0xAA),
        "BrowserFavorites" => Some(0xAB),
        "BrowserHome" => Some(0xAC),
        "AudioVolumeMute" => Some(0xAD),
        "AudioVolumeDown" => Some(0xAE),
        "AudioVolumeUp" => Some(0xAF),
        "MediaTrackNext" => Some(0xB0),
        "MediaTrackPrevious" => Some(0xB1),
        "MediaStop" => Some(0xB2),
        "MediaPlayPause" => Some(0xB3),
        "LaunchMail" => Some(0xB4),
        "LaunchMediaPlayer" => Some(0xB5),
        "LaunchApp1" => Some(0xB6),
        "LaunchApp2" => Some(0xB7),
        _ => None,
    }
}

fn vk_name(vk: u32) -> String {
    match vk {
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x13 => "Pause".to_string(),
        0x14 => "Caps Lock".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x21 => "Page Up".to_string(),
        0x22 => "Page Down".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2C => "Print Screen".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        0x5D => "Menu".to_string(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        VK_NUMPAD0..=0x69 => format!("Numpad {}", vk - VK_NUMPAD0),
        0x6A => "Numpad *".to_string(),
        0x6B => "Numpad +".to_string(),
        0x6D => "Numpad -".to_string(),
        0x6E => "Numpad .".to_string(),
        0x6F => "Numpad /".to_string(),
        VK_F1..=0x87 => format!("F{}", vk - VK_F1 + 1),
        0x90 => "Num Lock".to_string(),
        0x91 => "Scroll Lock".to_string(),
        0xA6 => "Browser Back".to_string(),
        0xA7 => "Browser Forward".to_string(),
        0xA8 => "Browser Refresh".to_string(),
        0xA9 => "Browser Stop".to_string(),
        0xAA => "Browser Search".to_string(),
        0xAB => "Browser Favorites".to_string(),
        0xAC => "Browser Home".to_string(),
        0xAD => "Volume Mute".to_string(),
        0xAE => "Volume Down".to_string(),
        0xAF => "Volume Up".to_string(),
        0xB0 => "Next Track".to_string(),
        0xB1 => "Previous Track".to_string(),
        0xB2 => "Media Stop".to_string(),
        0xB3 => "Play/Pause".to_string(),
        0xB4 => "Mail".to_string(),
        0xB5 => "Media Player".to_string(),
        0xB6 => "App 1".to_string(),
        0xB7 => "App 2".to_string(),
        0xBA => ";".to_string(),
        0xBB => "=".to_string(),
        0xBC => ",".to_string(),
        0xBD => "-".to_string(),
        0xBE => ".".to_string(),
        0xBF => "/".to_string(),
        0xC0 => "`".to_string(),
        0xDB => "[".to_string(),
        0xDC => "\\".to_string(),
        0xDD => "]".to_string(),
        0xDE => "'".to_string(),
        0xE2 => "Intl".to_string(),
        _ => format!("VK {vk}"),
    }
}

fn write_wide_buf<const N: usize>(buf: &mut [u16; N], text: &str) {
    let wide = wide(text);
    let len = (wide.len() - 1).min(N - 1);
    buf[..len].copy_from_slice(&wide[..len]);
    buf[len] = 0;
}

fn write_packed_wide_buf<const N: usize>(buf: *mut [u16; N], text: &str) {
    let wide = wide(text);
    let len = (wide.len() - 1).min(N - 1);
    let ptr = buf.cast::<u16>();

    unsafe {
        for (index, value) in wide.iter().take(len).copied().enumerate() {
            std::ptr::write_unaligned(ptr.add(index), value);
        }
        std::ptr::write_unaligned(ptr.add(len), 0);
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe fn pwstr_to_string(value: PWSTR) -> String {
    if value.0.is_null() {
        return String::new();
    }

    let mut len = 0usize;
    while unsafe { *value.0.add(len) } != 0 {
        len += 1;
    }
    let slice = unsafe { std::slice::from_raw_parts(value.0, len) };
    String::from_utf16_lossy(slice)
}
