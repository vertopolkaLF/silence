#![windows_subsystem = "windows"]

use std::{
    ffi::c_void,
    fs,
    mem::size_of,
    path::PathBuf,
    process::Command,
    ptr::{null, null_mut},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::SystemTime,
};

use anyhow::{Context, Result};
use dioxus::desktop::{Config as DesktopConfig, LogicalSize, WindowBuilder};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{
            ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT,
            POINT, WPARAM,
        },
        Media::Audio::{
            DEVICE_STATE_ACTIVE, Endpoints::IAudioEndpointVolume, IMMDevice, IMMDeviceEnumerator,
            MMDeviceEnumerator, eCapture, eConsole,
        },
        Media::Multimedia::mciSendStringW,
        System::{
            Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, STGM_READ,
            },
            LibraryLoader::GetModuleHandleW,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
            Threading::CreateMutexW,
        },
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallNextHookEx, CreateIconFromResourceEx, CreatePopupMenu,
                CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
                FindWindowW, GetCursorPos, GetMessageW, HHOOK, HICON, IDC_ARROW, IDI_APPLICATION,
                IsIconic, KBDLLHOOKSTRUCT, KillTimer, LR_DEFAULTSIZE, LoadCursorW, LoadIconW,
                MENU_ITEM_FLAGS, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_RESTORE,
                SendMessageW, SetForegroundWindow, SetTimer, SetWindowsHookExW, ShowWindow,
                TPM_BOTTOMALIGN, TPM_LEFTALIGN, TrackPopupMenu, TranslateMessage,
                UnhookWindowsHookEx, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WM_APP, WM_COMMAND,
                WM_DESTROY, WM_DISPLAYCHANGE, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_RBUTTONUP,
                WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
            },
        },
    },
    core::{PCWSTR, PWSTR, w},
};

mod gui;
mod native_overlay;

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE_MUTE: u32 = WM_APP + 2;
const WM_OVERLAY_POSITIONING: u32 = WM_APP + 3;
const ID_TRAY: u32 = 1;
const ID_CONFIG_TIMER: usize = 10;
const ID_OVERLAY_HIDE_TIMER: usize = 11;
const ID_OVERLAY_DRAG_TIMER: usize = 12;
const ID_MENU_TOGGLE: usize = 1001;
const ID_MENU_SETTINGS: usize = 1002;
const ID_MENU_EXIT: usize = 1003;
const SETTINGS_WINDOW_TITLE: &str = "silence!";
const ICON_RESOURCE_VERSION: u32 = 0x0003_0000;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default)]
    shortcut: Shortcut,
    #[serde(default)]
    mic_device_id: Option<String>,
    #[serde(default)]
    sound_settings: SoundSettings,
    #[serde(default)]
    overlay: OverlayConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            shortcut: Shortcut::default(),
            mic_device_id: None,
            sound_settings: SoundSettings::default(),
            overlay: OverlayConfig::default(),
        }
    }
}

struct AppState {
    hwnd: HWND,
    hook: HHOOK,
    shortcut: Shortcut,
    mic_device_id: Option<String>,
    sound_settings: SoundSettings,
    overlay: OverlayConfig,
    muted: bool,
    shortcut_down: bool,
    config_modified: Option<SystemTime>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OverlayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_overlay_visibility")]
    pub visibility: String,
    #[serde(default = "default_overlay_position_x")]
    pub position_x: f64,
    #[serde(default = "default_overlay_position_y")]
    pub position_y: f64,
    #[serde(default = "default_overlay_duration_secs")]
    pub duration_secs: f64,
    #[serde(default = "default_overlay_scale")]
    pub scale: u32,
    #[serde(default)]
    pub show_text: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            visibility: default_overlay_visibility(),
            position_x: default_overlay_position_x(),
            position_y: default_overlay_position_y(),
            duration_secs: default_overlay_duration_secs(),
            scale: default_overlay_scale(),
            show_text: false,
        }
    }
}

fn default_overlay_visibility() -> String {
    "WhenMuted".to_string()
}

fn default_overlay_position_x() -> f64 {
    50.0
}

fn default_overlay_position_y() -> f64 {
    80.0
}

fn default_overlay_duration_secs() -> f64 {
    2.0
}

fn default_overlay_scale() -> u32 {
    100
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct SoundSettings {
    #[serde(default = "default_sounds_enabled")]
    pub enabled: bool,
    #[serde(default = "default_sound_volume")]
    pub volume: u8,
    #[serde(default = "default_sound_theme")]
    pub mute_theme: String,
    #[serde(default = "default_sound_theme")]
    pub unmute_theme: String,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: default_sounds_enabled(),
            volume: default_sound_volume(),
            mute_theme: default_sound_theme(),
            unmute_theme: default_sound_theme(),
        }
    }
}

fn default_sounds_enabled() -> bool {
    true
}

fn default_sound_volume() -> u8 {
    20
}

fn default_sound_theme() -> String {
    "8bit".to_string()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoundTheme {
    pub id: &'static str,
    pub label: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowsAccent {
    accent: (u8, u8, u8),
}

impl Default for WindowsAccent {
    fn default() -> Self {
        Self {
            accent: (250, 121, 48),
        }
    }
}

impl WindowsAccent {
    pub fn load() -> Self {
        let fallback = Self::default();
        Self {
            accent: read_windows_accent_dword()
                .map(windows_accent_to_rgb)
                .unwrap_or(fallback.accent),
        }
    }

    pub fn css_vars(self) -> String {
        let (r, g, b) = self.accent;
        format!(":root {{ --windows-accent: rgb({r}, {g}, {b}); }}")
    }
}

fn read_windows_accent_dword() -> Option<u32> {
    read_registry_dword(
        w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent"),
        w!("AccentColorMenu"),
    )
    .or_else(|| read_registry_dword(w!(r"Software\Microsoft\Windows\DWM"), w!("AccentColor")))
    .or_else(|| {
        read_registry_dword(
            w!(r"Software\Microsoft\Windows\DWM"),
            w!("ColorizationColor"),
        )
    })
}

fn read_registry_dword(subkey: PCWSTR, value_name: PCWSTR) -> Option<u32> {
    let mut data = 0u32;
    let mut data_size = size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value_name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut _ as *mut c_void),
            Some(&mut data_size),
        )
    };
    if status == ERROR_SUCCESS {
        Some(data)
    } else {
        None
    }
}

fn windows_accent_to_rgb(value: u32) -> (u8, u8, u8) {
    (
        (value & 0xff) as u8,
        ((value >> 8) & 0xff) as u8,
        ((value >> 16) & 0xff) as u8,
    )
}

impl Default for AppState {
    fn default() -> Self {
        let config = load_config().unwrap_or_default();
        Self {
            hwnd: HWND(null_mut()),
            hook: HHOOK(null_mut()),
            shortcut: config.shortcut,
            mic_device_id: config.mic_device_id,
            sound_settings: config.sound_settings,
            overlay: config.overlay,
            muted: false,
            shortcut_down: false,
            config_modified: config_modified_time(),
        }
    }
}

unsafe impl Send for AppState {}

static STATE: Lazy<Mutex<AppState>> = Lazy::new(|| Mutex::new(AppState::default()));
static SOUND_ALIAS_COUNTER: AtomicUsize = AtomicUsize::new(1);

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
    if std::env::args().any(|arg| arg == "--settings") {
        let settings_mutex = unsafe { CreateMutexW(None, true, w!("SilenceV2SettingsWindow"))? };
        if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
            focus_settings_window();
            return Ok(());
        }

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok();
        }
        let cfg = DesktopConfig::new()
            .with_window(
                WindowBuilder::new()
                    .with_title(SETTINGS_WINDOW_TITLE)
                    .with_decorations(false)
                    .with_resizable(true)
                    .with_inner_size(LogicalSize::new(760.0, 590.0)),
            )
            .with_icon(
                dioxus::desktop::icon_from_memory(include_bytes!("../assets/app.png"))
                    .expect("load app icon"),
            )
            .with_custom_head(
                r#"<link rel="icon" href="/assets/app.ico" type="image/x-icon">"#.to_string(),
            )
            .with_background_color((18, 18, 18, 255));
        dioxus::LaunchBuilder::desktop()
            .with_cfg(cfg)
            .launch(gui::settings_app);
        let _settings_mutex = settings_mutex;
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
    let muted = current_mute_state().unwrap_or(false);
    let overlay_config = STATE.lock().unwrap().overlay.clone();
    {
        let mut state = STATE.lock().unwrap();
        state.hwnd = hwnd;
        state.muted = muted;
    }
    native_overlay::init(instance.into(), muted, &overlay_config)?;
    apply_overlay_visibility();

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
    let icon = load_app_icon().or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION).ok() });
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
    write_wide_buf(&mut nid.szTip, "Silence");
    unsafe {
        Shell_NotifyIconW(NIM_ADD, &nid).ok()?;
    }
    refresh_tray_tip();
    Ok(())
}

fn load_app_icon() -> Option<HICON> {
    let icon_bytes = include_bytes!("../assets/app.ico");
    let image = best_ico_image(icon_bytes, 16)?;
    unsafe {
        CreateIconFromResourceEx(image, true, ICON_RESOURCE_VERSION, 0, 0, LR_DEFAULTSIZE).ok()
    }
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
            } else if wparam.0 == ID_OVERLAY_HIDE_TIMER {
                let _ = unsafe { KillTimer(hwnd, ID_OVERLAY_HIDE_TIMER) };
                apply_overlay_visibility();
            } else if wparam.0 == ID_OVERLAY_DRAG_TIMER {
                if let Some((x, y)) = native_overlay::process_drag() {
                    save_overlay_position(x, y);
                }
            }
            LRESULT(0)
        }
        WM_TOGGLE_MUTE => {
            toggle_mute();
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
    if focus_settings_window() {
        return;
    }

    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let _ = Command::new(exe).arg("--settings").spawn();
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
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
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
            play_mute_sound(muted);
            STATE.lock().unwrap().muted = muted;
            refresh_tray_tip();
            let overlay = STATE.lock().unwrap().overlay.clone();
            native_overlay::update(muted, &overlay);
            if overlay.enabled && overlay.visibility == "AfterToggle" {
                let millis = (overlay.duration_secs.clamp(0.1, 10.0) * 1000.0) as u32;
                show_overlay_temporarily(millis);
            } else {
                apply_overlay_visibility();
            }
        }
        Err(err) => eprintln!("failed to toggle microphone mute: {err:?}"),
    }
}

fn apply_overlay_visibility() {
    let (muted, overlay) = {
        let state = STATE.lock().unwrap();
        (state.muted, state.overlay.clone())
    };

    native_overlay::update(muted, &overlay);
    if native_overlay::is_positioning() {
        native_overlay::show();
        return;
    }

    if !overlay.enabled {
        native_overlay::hide();
        return;
    }

    let should_show = match overlay.visibility.as_str() {
        "Always" => true,
        "WhenMuted" => muted,
        "WhenUnmuted" => !muted,
        "AfterToggle" => false,
        _ => muted,
    };

    if should_show {
        native_overlay::show();
    } else {
        native_overlay::hide();
    }
}

fn show_overlay_temporarily(duration_ms: u32) {
    let (hwnd, muted, overlay) = {
        let state = STATE.lock().unwrap();
        (state.hwnd, state.muted, state.overlay.clone())
    };
    native_overlay::update(muted, &overlay);
    native_overlay::show();
    unsafe {
        let _ = KillTimer(hwnd, ID_OVERLAY_HIDE_TIMER);
        let _ = SetTimer(hwnd, ID_OVERLAY_HIDE_TIMER, duration_ms, None);
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

fn save_overlay_position(position_x: f64, position_y: f64) {
    let mut config = load_config().unwrap_or_default();
    config.overlay.position_x = position_x;
    config.overlay.position_y = position_y;
    let _ = save_config(&config);

    let mut state = STATE.lock().unwrap();
    state.overlay.position_x = position_x;
    state.overlay.position_y = position_y;
    state.config_modified = config_modified_time();
}

fn current_mute_state() -> Result<bool> {
    let volume = selected_capture_volume()?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

pub fn mic_mute_state(device_id: Option<&str>) -> Result<bool> {
    let volume = capture_volume(device_id)?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

fn set_mute_to_inverse() -> Result<bool> {
    let volume = selected_capture_volume()?;
    unsafe {
        let muted = volume.GetMute()?;
        let next = !muted.as_bool();
        volume.SetMute(next, null())?;
        Ok(next)
    }
}

fn play_mute_sound(muted: bool) {
    let settings = STATE.lock().unwrap().sound_settings.clone();
    if !settings.enabled {
        return;
    }
    let theme = if muted {
        settings.mute_theme.as_str()
    } else {
        settings.unmute_theme.as_str()
    };
    if let Err(err) = play_sound(theme, muted, settings.volume) {
        eprintln!("failed to play mute sound: {err:?}");
    }
}

pub fn preview_sound(theme: &str, muted: bool, volume: u8) -> Result<()> {
    play_sound(theme, muted, volume)
}

pub fn sound_themes() -> &'static [SoundTheme] {
    SOUND_THEMES
}

pub fn sound_theme_label(theme_id: &str) -> &'static str {
    SOUND_THEMES
        .iter()
        .find(|theme| theme.id == theme_id)
        .map(|theme| theme.label)
        .unwrap_or(SOUND_THEMES[0].label)
}

fn play_sound(theme: &str, muted: bool, volume: u8) -> Result<()> {
    let file = sound_file_name(theme, muted);
    let path = sound_asset_path(&file).with_context(|| format!("find sound asset {file}"))?;
    let volume = (u16::from(volume.min(100)) * 10).to_string();
    std::thread::spawn(move || {
        let alias_number = SOUND_ALIAS_COUNTER.fetch_add(1, Ordering::Relaxed);
        let alias = format!("silence_sfx_{alias_number}");
        let path = path.to_string_lossy();
        let open = format!(r#"open "{}" type mpegvideo alias {}"#, path, alias);
        if unsafe { mci_send(&open) } != 0 {
            return;
        }
        let _ = unsafe { mci_send(&format!("setaudio {alias} volume to {volume}")) };
        let _ = unsafe { mci_send(&format!("play {alias} wait")) };
        let _ = unsafe { mci_send(&format!("close {alias}")) };
    });
    Ok(())
}

fn sound_file_name(theme: &str, muted: bool) -> String {
    let theme = if SOUND_THEMES.iter().any(|known| known.id == theme) {
        theme
    } else {
        SOUND_THEMES[0].id
    };
    let action = if muted { "mute" } else { "unmute" };
    format!("{theme}_{action}.mp3")
}

fn sound_asset_path(file: &str) -> Option<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(parent) = exe.parent()
    {
        roots.extend(parent.ancestors().map(PathBuf::from));
    }
    roots.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")));

    roots
        .into_iter()
        .map(|root| root.join("assets").join("sounds").join(file))
        .find(|path| path.exists())
}

unsafe fn mci_send(command: &str) -> u32 {
    let command = wide(command);
    unsafe { mciSendStringW(PCWSTR(command.as_ptr()), None, HWND(null_mut())) }
}

fn selected_capture_volume() -> Result<IAudioEndpointVolume> {
    let device_id = STATE.lock().unwrap().mic_device_id.clone();
    capture_volume(device_id.as_deref())
}

fn capture_volume(device_id: Option<&str>) -> Result<IAudioEndpointVolume> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create audio device enumerator")?;
        let device = capture_device(&enumerator, device_id)?;
        device
            .Activate(CLSCTX_ALL, None)
            .context("activate endpoint volume")
    }
}

unsafe fn capture_device(
    enumerator: &IMMDeviceEnumerator,
    device_id: Option<&str>,
) -> Result<IMMDevice> {
    if let Some(device_id) = device_id.filter(|id| !id.is_empty()) {
        let id = wide(device_id);
        if let Ok(device) = unsafe { enumerator.GetDevice(PCWSTR(id.as_ptr())) } {
            if unsafe { device.GetState()? } == DEVICE_STATE_ACTIVE {
                return Ok(device);
            }
        }
    }

    unsafe { enumerator.GetDefaultAudioEndpoint(eCapture, eConsole) }
        .context("get default capture endpoint")
}

pub fn capture_devices() -> Result<Vec<MicDevice>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create audio device enumerator")?;
        let default_id = capture_device_id(
            &enumerator
                .GetDefaultAudioEndpoint(eCapture, eConsole)
                .context("get default capture endpoint")?,
        )
        .ok();
        let collection = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .context("enumerate capture endpoints")?;
        let count = collection.GetCount().context("count capture endpoints")?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index).context("get capture endpoint")?;
            let id = capture_device_id(&device)?;
            let name = capture_device_name(&device).unwrap_or_else(|| "Microphone".to_string());
            let is_default = default_id.as_deref() == Some(id.as_str());
            devices.push(MicDevice {
                id,
                name,
                is_default,
            });
        }

        Ok(devices)
    }
}

pub fn selected_mic_label(selected_id: Option<&str>, devices: &[MicDevice]) -> String {
    selected_id
        .and_then(|id| devices.iter().find(|device| device.id == id))
        .map(|device| device.name.clone())
        .or_else(|| {
            devices
                .iter()
                .find(|device| device.is_default)
                .map(|device| format!("{} (default)", device.name))
        })
        .unwrap_or_else(|| "Default input device".to_string())
}

unsafe fn capture_device_id(device: &IMMDevice) -> Result<String> {
    let id = unsafe { device.GetId()? };
    let text = unsafe { pwstr_to_string(id) };
    unsafe { CoTaskMemFree(Some(id.0 as *const c_void)) };
    Ok(text)
}

unsafe fn capture_device_name(device: &IMMDevice) -> Option<String> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ).ok()? };
    let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName).ok()? };
    let name = value.to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn reload_config_if_changed() {
    let modified = config_modified_time();
    let mut state = STATE.lock().unwrap();
    if modified == state.config_modified {
        return;
    }
    if let Ok(config) = load_config() {
        let muted = capture_volume(config.mic_device_id.as_deref())
            .and_then(|volume| unsafe { Ok(volume.GetMute()?.as_bool()) })
            .unwrap_or(state.muted);
        state.shortcut = config.shortcut;
        state.mic_device_id = config.mic_device_id;
        state.sound_settings = config.sound_settings;
        state.overlay = config.overlay.clone();
        state.muted = muted;
        state.config_modified = modified;
        state.shortcut_down = false;
        drop(state);
        refresh_tray_tip();
        native_overlay::update(muted, &config.overlay);
        apply_overlay_visibility();
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
    native_overlay::destroy();
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
