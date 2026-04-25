#![windows_subsystem = "windows"]

use std::{
    fs,
    mem::size_of,
    path::PathBuf,
    process::Command,
    ptr::{null, null_mut},
    sync::Mutex,
    time::SystemTime,
};

use anyhow::{Context, Result};
use dioxus::desktop::{Config as DesktopConfig, LogicalSize, WindowBuilder};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM},
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
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallNextHookEx, CreatePopupMenu, CreateWindowExW, DefWindowProcW,
                DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, HHOOK,
                IDC_ARROW, IDI_APPLICATION, KBDLLHOOKSTRUCT, LoadCursorW, LoadIconW,
                MENU_ITEM_FLAGS, MSG, PostMessageW, PostQuitMessage, RegisterClassW,
                SetForegroundWindow, SetTimer, SetWindowsHookExW, TPM_BOTTOMALIGN, TPM_LEFTALIGN,
                TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL,
                WINDOW_EX_STYLE, WM_APP, WM_COMMAND, WM_DESTROY, WM_KEYDOWN, WM_KEYUP,
                WM_LBUTTONDBLCLK, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    },
    core::{PCWSTR, w},
};

mod gui;

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE_MUTE: u32 = WM_APP + 2;
const ID_TRAY: u32 = 1;
const ID_CONFIG_TIMER: usize = 10;
const ID_MENU_TOGGLE: usize = 1001;
const ID_MENU_SETTINGS: usize = 1002;
const ID_MENU_EXIT: usize = 1003;

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
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

struct AppState {
    hwnd: HWND,
    hook: HHOOK,
    shortcut: Shortcut,
    muted: bool,
    shortcut_down: bool,
    config_modified: Option<SystemTime>,
}

impl Default for AppState {
    fn default() -> Self {
        let config = load_config().unwrap_or_default();
        Self {
            hwnd: HWND(null_mut()),
            hook: HHOOK(null_mut()),
            shortcut: config.shortcut,
            muted: false,
            shortcut_down: false,
            config_modified: config_modified_time(),
        }
    }
}

unsafe impl Send for AppState {}

static STATE: Lazy<Mutex<AppState>> = Lazy::new(|| Mutex::new(AppState::default()));

fn main() -> Result<()> {
    if std::env::args().any(|arg| arg == "--settings") {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
        }
        let cfg = DesktopConfig::new()
            .with_window(
                WindowBuilder::new()
                    .with_title("silence!")
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_inner_size(LogicalSize::new(760.0, 590.0)),
            )
            .with_background_color((35, 28, 26, 255));
        dioxus::LaunchBuilder::desktop()
            .with_cfg(cfg)
            .launch(gui::settings_app);
        return Ok(());
    }

    run_background_app()
}

fn run_background_app() -> Result<()> {
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
    }

    let instance = unsafe { GetModuleHandleW(None)? };
    register_class(instance.into())?;
    let hwnd = create_message_window(instance.into())?;
    {
        let mut state = STATE.lock().unwrap();
        state.hwnd = hwnd;
        state.muted = current_mute_state().unwrap_or(false);
    }

    install_keyboard_hook(instance.into())?;
    add_tray_icon(hwnd)?;
    unsafe {
        let _ = SetTimer(hwnd, ID_CONFIG_TIMER, 1000, None);
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
    write_wide_buf(&mut nid.szTip, "Silence");
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
    }
    refresh_tray_tip();
    Ok(())
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
                WM_LBUTTONDBLCLK => open_settings_window(),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 & 0xffff {
                ID_MENU_TOGGLE => toggle_mute(),
                ID_MENU_SETTINGS => open_settings_window(),
                ID_MENU_EXIT => {
                    let _ = unsafe { DestroyWindow(hwnd) };
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == ID_CONFIG_TIMER {
                reload_config_if_changed();
            }
            LRESULT(0)
        }
        WM_TOGGLE_MUTE => {
            toggle_mute();
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
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0),
            ID_MENU_EXIT,
            PCWSTR(exit_w.as_ptr()),
        );

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(
            menu,
            TPM_LEFTALIGN | TPM_BOTTOMALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
    }
}

fn open_settings_window() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = Command::new(exe).arg("--settings").spawn();
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
            if state.shortcut.is_pressed(vk) && !state.shortcut_down {
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
            STATE.lock().unwrap().muted = muted;
            refresh_tray_tip();
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

fn reload_config_if_changed() {
    let modified = config_modified_time();
    let mut state = STATE.lock().unwrap();
    if modified == state.config_modified {
        return;
    }
    if let Ok(config) = load_config() {
        state.shortcut = config.shortcut;
        state.config_modified = modified;
        state.shortcut_down = false;
        drop(state);
        refresh_tray_tip();
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

fn config_modified_time() -> Option<SystemTime> {
    config_path().ok()?.metadata().ok()?.modified().ok()
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
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
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
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(vk).unwrap().to_string(),
        VK_NUMPAD0..=0x69 => format!("Numpad {}", vk - VK_NUMPAD0),
        VK_F1..=0x87 => format!("F{}", vk - VK_F1 + 1),
        _ => format!("VK {vk}"),
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
