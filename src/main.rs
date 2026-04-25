#![windows_subsystem = "windows"]

use std::{
    ffi::c_void,
    fs,
    mem::size_of,
    path::PathBuf,
    ptr::{null, null_mut},
    sync::Mutex,
};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use windows::{
    core::{PCWSTR, w},
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
        Graphics::Gdi::UpdateWindow,
        Media::Audio::{
            Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture,
            eConsole,
        },
        System::{
            Com::{CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx},
            LibraryLoader::GetModuleHandleW,
        },
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyMenu, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetCursorPos,
                GetMessageW, GetWindowLongPtrW, HHOOK, HMENU, IDC_ARROW, IDI_APPLICATION,
                KBDLLHOOKSTRUCT, LoadCursorW, LoadIconW, MENU_ITEM_FLAGS, MSG, PostMessageW,
                PostQuitMessage, RegisterClassW, SW_HIDE, SW_SHOW, SendMessageW, SetForegroundWindow,
                SetWindowLongPtrW, SetWindowsHookExW, ShowWindow, TPM_BOTTOMALIGN,
                TPM_LEFTALIGN, TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, WINDOW_EX_STYLE,
                WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_KEYDOWN, WM_KEYUP,
                WM_LBUTTONDBLCLK, WM_RBUTTONUP, WM_SETTEXT, WNDCLASSW, WS_BORDER,
                WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
                WH_KEYBOARD_LL,
            },
        },
    },
};

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE_MUTE: u32 = WM_APP + 2;
const WM_REFRESH_UI: u32 = WM_APP + 3;

const ID_TRAY: u32 = 1;
const ID_MENU_TOGGLE: usize = 1001;
const ID_MENU_SETTINGS: usize = 1002;
const ID_MENU_EXIT: usize = 1003;
const ID_BTN_RECORD: usize = 2001;
const ID_BTN_SAVE: usize = 2002;
const ID_BTN_CANCEL: usize = 2003;
const ID_LABEL_SHORTCUT: isize = 3001;
const ID_LABEL_STATUS: isize = 3002;

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_NUMPAD0: u32 = 0x60;
const VK_F1: u32 = 0x70;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Shortcut {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    vk: u32,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            vk: b'M' as u32,
        }
    }
}

impl Shortcut {
    fn from_current_modifiers(vk: u32) -> Self {
        Self {
            ctrl: key_down(VK_CONTROL),
            alt: key_down(VK_MENU),
            shift: key_down(VK_SHIFT),
            win: key_down(VK_LWIN) || key_down(VK_RWIN),
            vk,
        }
    }

    fn is_pressed(&self, vk: u32) -> bool {
        self.vk == vk
            && self.ctrl == key_down(VK_CONTROL)
            && self.alt == key_down(VK_MENU)
            && self.shift == key_down(VK_SHIFT)
            && self.win == (key_down(VK_LWIN) || key_down(VK_RWIN))
    }

    fn display(self) -> String {
        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        if self.win {
            parts.push("Win".to_string());
        }
        parts.push(vk_name(self.vk));
        parts.join(" + ")
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    shortcut: Shortcut,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
        }
    }
}

#[derive(Default)]
struct AppState {
    hwnd: HWND,
    settings_hwnd: HWND,
    shortcut_label: HWND,
    status_label: HWND,
    hook: HHOOK,
    shortcut: Shortcut,
    pending_shortcut: Shortcut,
    muted: bool,
    recording: bool,
    shortcut_down: bool,
}

unsafe impl Send for AppState {}

static STATE: Lazy<Mutex<AppState>> = Lazy::new(|| {
    let config = load_config().unwrap_or_default();
    Mutex::new(AppState {
        shortcut: config.shortcut,
        pending_shortcut: config.shortcut,
        ..Default::default()
    })
});

fn main() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    let instance = unsafe { GetModuleHandleW(None)? };
    register_classes(instance.into())?;
    let hwnd = create_message_window(instance.into())?;
    {
        let mut state = STATE.lock().unwrap();
        state.hwnd = hwnd;
        state.muted = current_mute_state().unwrap_or(false);
    }

    install_keyboard_hook(instance.into())?;
    add_tray_icon(hwnd)?;

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

fn register_classes(instance: HINSTANCE) -> Result<()> {
    unsafe {
        let cursor = LoadCursorW(None, IDC_ARROW)?;
        let main = WNDCLASSW {
            hCursor: cursor,
            hInstance: instance,
            lpszClassName: w!("SilenceV2Hidden"),
            lpfnWndProc: Some(main_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&main);

        let settings = WNDCLASSW {
            hCursor: cursor,
            hInstance: instance,
            lpszClassName: w!("SilenceV2Settings"),
            lpfnWndProc: Some(settings_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&settings);
    }
    Ok(())
}

fn create_message_window(instance: HINSTANCE) -> Result<HWND> {
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SilenceV2Hidden"),
            w!("Silence"),
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
    ensure_hwnd(hwnd, "create hidden window")
}

fn install_keyboard_hook(instance: HINSTANCE) -> Result<()> {
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), instance, 0) }
        .context("install low-level keyboard hook")?;
    STATE.lock().unwrap().hook = hook;
    Ok(())
}

fn add_tray_icon(hwnd: HWND) -> Result<()> {
    let icon = unsafe { LoadIconW(None, IDI_APPLICATION)? };
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ID_TRAY,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    write_wide_buf(&mut nid.szTip, "Silence: microphone shortcut");
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
    }
    refresh_tray_icon();
    Ok(())
}

fn refresh_tray_icon() {
    let state = STATE.lock().unwrap();
    if is_null_hwnd(state.hwnd) {
        return;
    }
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: state.hwnd,
        uID: ID_TRAY,
        uFlags: NIF_TIP,
        ..Default::default()
    };
    let tip = if state.muted {
        format!("Silence: microphone muted ({})", state.shortcut.display())
    } else {
        format!("Silence: microphone on ({})", state.shortcut.display())
    };
    write_wide_buf(&mut nid.szTip, &tip);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
    }
}

fn remove_tray_icon() {
    let state = STATE.lock().unwrap();
    if is_null_hwnd(state.hwnd) {
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
}

unsafe extern "system" fn main_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam.0 as u32 {
                WM_RBUTTONUP => show_tray_menu(hwnd),
                WM_LBUTTONDBLCLK => show_settings_window(hwnd),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                ID_MENU_TOGGLE => toggle_mute(),
                ID_MENU_SETTINGS => show_settings_window(hwnd),
                ID_MENU_EXIT => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TOGGLE_MUTE => {
            toggle_mute();
            LRESULT(0)
        }
        WM_REFRESH_UI => {
            refresh_settings_labels();
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
        let status = if STATE.lock().unwrap().muted {
            "Unmute microphone"
        } else {
            "Mute microphone"
        };
        let status_w = wide(status);
        let settings_w = wide("Settings...");
        let exit_w = wide("Exit");
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0), ID_MENU_TOGGLE, PCWSTR(status_w.as_ptr()));
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0), ID_MENU_SETTINGS, PCWSTR(settings_w.as_ptr()));
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0), ID_MENU_EXIT, PCWSTR(exit_w.as_ptr()));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_LEFTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

fn show_settings_window(owner: HWND) {
    let instance = unsafe { GetModuleHandleW(None).ok().map(HINSTANCE::from) };
    let Some(instance) = instance else {
        return;
    };

    let mut state = STATE.lock().unwrap();
    if !is_null_hwnd(state.settings_hwnd) {
        unsafe {
            let _ = ShowWindow(state.settings_hwnd, SW_SHOW);
            let _ = SetForegroundWindow(state.settings_hwnd);
        }
        return;
    }
    state.pending_shortcut = state.shortcut;
    state.recording = false;
    drop(state);

    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("SilenceV2Settings"),
            w!("Silence Settings"),
            WS_CAPTION | WS_SYSMENU,
            420,
            260,
            390,
            190,
            owner,
            None,
            instance,
            None,
        )
    }
    .unwrap_or_default();
    if !is_null_hwnd(hwnd) {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = UpdateWindow(hwnd);
        }
    }
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            create_settings_controls(hwnd);
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                ID_BTN_RECORD => {
                    let mut state = STATE.lock().unwrap();
                    state.recording = true;
                    set_label_text(state.shortcut_label, "Press a shortcut...");
                }
                ID_BTN_SAVE => {
                    let shortcut = {
                        let mut state = STATE.lock().unwrap();
                        state.shortcut = state.pending_shortcut;
                        state.shortcut
                    };
                    if let Err(err) = save_config(&Config { shortcut }) {
                        eprintln!("failed to save config: {err:?}");
                    }
                    refresh_settings_labels();
                    refresh_tray_icon();
                }
                ID_BTN_CANCEL => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = unsafe { ShowWindow(hwnd, SW_HIDE) };
            LRESULT(0)
        }
        WM_DESTROY => {
            let mut state = STATE.lock().unwrap();
            state.settings_hwnd = HWND(null_mut());
            state.shortcut_label = HWND(null_mut());
            state.status_label = HWND(null_mut());
            state.recording = false;
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn create_settings_controls(hwnd: HWND) {
    unsafe {
        let font = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
        let label = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("Shortcut:"),
            WS_CHILD | WS_VISIBLE,
            18,
            20,
            90,
            24,
            hwnd,
            None,
            None,
            None,
        )
        .unwrap_or_default();
        SetWindowLongPtrW(label, GWLP_USERDATA, font);

        let shortcut_label = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!(""),
            WS_CHILD | WS_VISIBLE | WS_BORDER,
            105,
            18,
            245,
            26,
            hwnd,
            hmenu(ID_LABEL_SHORTCUT as usize),
            None,
            None,
        )
        .unwrap_or_default();

        let record = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Record shortcut"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            105,
            55,
            145,
            30,
            hwnd,
            hmenu(ID_BTN_RECORD),
            None,
            None,
        )
        .unwrap_or_default();

        let status_label = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!(""),
            WS_CHILD | WS_VISIBLE,
            18,
            96,
            340,
            24,
            hwnd,
            hmenu(ID_LABEL_STATUS as usize),
            None,
            None,
        )
        .unwrap_or_default();

        let save = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Save"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            188,
            125,
            80,
            30,
            hwnd,
            hmenu(ID_BTN_SAVE),
            None,
            None,
        )
        .unwrap_or_default();
        let cancel = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("BUTTON"),
            w!("Close"),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
            275,
            125,
            80,
            30,
            hwnd,
            hmenu(ID_BTN_CANCEL),
            None,
            None,
        )
        .unwrap_or_default();

        let mut state = STATE.lock().unwrap();
        state.settings_hwnd = hwnd;
        state.shortcut_label = shortcut_label;
        state.status_label = status_label;
        drop(state);

        let _ = (label, record, save, cancel);
        refresh_settings_labels();
    }
}

fn refresh_settings_labels() {
    let state = STATE.lock().unwrap();
    if is_null_hwnd(state.settings_hwnd) {
        return;
    }
    set_label_text(state.shortcut_label, &state.pending_shortcut.display());
    let status = if state.muted {
        "Microphone is muted"
    } else {
        "Microphone is on"
    };
    set_label_text(state.status_label, status);
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = kb.vkCode;
    let is_down = event == WM_KEYDOWN || event == 0x0104;
    let is_up = event == WM_KEYUP || event == 0x0105;

    if is_down && !is_modifier(vk) {
        let mut trigger = false;
        let mut consumed = false;
        {
            let mut state = STATE.lock().unwrap();
            if state.recording {
                let shortcut = Shortcut::from_current_modifiers(vk);
                state.pending_shortcut = shortcut;
                state.recording = false;
                set_label_text(state.shortcut_label, &shortcut.display());
                consumed = true;
            } else if state.shortcut.is_pressed(vk) && !state.shortcut_down {
                state.shortcut_down = true;
                trigger = true;
                consumed = true;
            }
        }
        if trigger {
            let hwnd = STATE.lock().unwrap().hwnd;
            let _ = unsafe { PostMessageW(hwnd, WM_TOGGLE_MUTE, WPARAM(0), LPARAM(0)) };
        }
        if consumed {
            return LRESULT(1);
        }
    }

    if is_up {
        let mut state = STATE.lock().unwrap();
        if state.shortcut.vk == vk {
            state.shortcut_down = false;
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn toggle_mute() {
    match set_mute_to_inverse() {
        Ok(muted) => {
            let hwnd = {
                let mut state = STATE.lock().unwrap();
                state.muted = muted;
                state.hwnd
            };
            unsafe {
                let _ = PostMessageW(hwnd, WM_REFRESH_UI, WPARAM(0), LPARAM(0));
            }
        }
        Err(err) => eprintln!("failed to toggle microphone mute: {err:?}"),
    }
}

fn current_mute_state() -> Result<bool> {
    let volume = default_capture_volume()?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

fn set_mute_to_inverse() -> Result<bool> {
    let volume = default_capture_volume()?;
    unsafe {
        let muted = volume.GetMute()?;
        let next = !muted.as_bool();
        volume.SetMute(next, null())?;
        Ok(next)
    }
}

fn default_capture_volume() -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create audio device enumerator")?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eCapture, eConsole)
            .context("get default capture endpoint")?;
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")
    }
}

fn load_config() -> Result<Config> {
    let path = config_path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let content = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&content)?)
}

fn save_config(config: &Config) -> Result<()> {
    let path = config_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(config)?)?;
    Ok(())
}

fn config_path() -> Result<PathBuf> {
    let appdata = std::env::var_os("APPDATA").context("APPDATA is not set")?;
    Ok(PathBuf::from(appdata).join("SilenceV2").join("config.json"))
}

fn cleanup() {
    remove_tray_icon();
    let hook = STATE.lock().unwrap().hook;
    if !hook.0.is_null() {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

fn key_down(vk: u32) -> bool {
    unsafe { (GetAsyncKeyState(vk as i32) as u16 & 0x8000) != 0 }
}

fn is_modifier(vk: u32) -> bool {
    matches!(vk, VK_SHIFT | VK_CONTROL | VK_MENU | VK_LWIN | VK_RWIN)
}

fn vk_name(vk: u32) -> String {
    match vk {
        VK_BACK => "Backspace".to_string(),
        VK_TAB => "Tab".to_string(),
        VK_RETURN => "Enter".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_SPACE => "Space".to_string(),
        VK_PRIOR => "Page Up".to_string(),
        VK_NEXT => "Page Down".to_string(),
        VK_END => "End".to_string(),
        VK_HOME => "Home".to_string(),
        VK_LEFT => "Left".to_string(),
        VK_UP => "Up".to_string(),
        VK_RIGHT => "Right".to_string(),
        VK_DOWN => "Down".to_string(),
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        VK_NUMPAD0..=0x69 => format!("Numpad {}", vk - VK_NUMPAD0),
        VK_F1..=0x87 => format!("F{}", vk - VK_F1 + 1),
        _ => format!("VK {vk}"),
    }
}

fn set_label_text(hwnd: HWND, text: &str) {
    if is_null_hwnd(hwnd) {
        return;
    }
    let wide = wide(text);
    unsafe {
        let _ = SendMessageW(hwnd, WM_SETTEXT, WPARAM(0), LPARAM(wide.as_ptr() as isize));
    }
}

fn write_wide_buf<const N: usize>(buf: &mut [u16; N], text: &str) {
    let wide = wide(text);
    let len = (wide.len() - 1).min(N - 1);
    buf[..len].copy_from_slice(&wide[..len]);
    buf[len] = 0;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn ensure_hwnd(hwnd: HWND, action: &str) -> Result<HWND> {
    if is_null_hwnd(hwnd) {
        anyhow::bail!("failed to {action}");
    }
    Ok(hwnd)
}

fn hmenu(id: usize) -> HMENU {
    HMENU(id as *mut c_void)
}

fn is_null_hwnd(hwnd: HWND) -> bool {
    hwnd.0.is_null()
}
