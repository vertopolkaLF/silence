#![windows_subsystem = "windows"]

use std::{
    collections::{HashMap, HashSet},
    ffi::c_void,
    fs,
    io::Cursor,
    mem::size_of,
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime},
};

use anyhow::{Context, Result};
use dioxus::desktop::{
    Config as DesktopConfig, LogicalSize, WindowBuilder,
    tao::{dpi::PhysicalPosition, platform::windows::WindowBuilderExtWindows},
};
use gilrs::{Button, EventType, Gilrs};
use once_cell::sync::Lazy;
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Source, buffer::SamplesBuffer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::{
    Win32::{
        Devices::FunctionDiscovery::PKEY_Device_FriendlyName,
        Foundation::{
            ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS, GetLastError, HINSTANCE,
            HWND, LPARAM, LRESULT, POINT, WPARAM,
        },
        Graphics::{
            Dwm::{
                DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
                DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
                DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, SelectObject,
            },
        },
        Media::Audio::{
            DEVICE_STATE_ACTIVE, EDataFlow, ERole, Endpoints::IAudioEndpointVolume, IMMDevice,
            IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications, eConsole,
            eMultimedia, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, STGM_READ,
            },
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
                RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
            },
            SystemInformation::GetTickCount,
            Threading::CreateMutexW,
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForSystem,
                SetProcessDpiAwarenessContext,
            },
            Input::{
                KeyboardAndMouse::{GetAsyncKeyState, GetLastInputInfo, LASTINPUTINFO},
                XboxController::{
                    XINPUT_GAMEPAD_A, XINPUT_GAMEPAD_B, XINPUT_GAMEPAD_BACK,
                    XINPUT_GAMEPAD_DPAD_DOWN, XINPUT_GAMEPAD_DPAD_LEFT, XINPUT_GAMEPAD_DPAD_RIGHT,
                    XINPUT_GAMEPAD_DPAD_UP, XINPUT_GAMEPAD_LEFT_SHOULDER,
                    XINPUT_GAMEPAD_LEFT_THUMB, XINPUT_GAMEPAD_RIGHT_SHOULDER,
                    XINPUT_GAMEPAD_RIGHT_THUMB, XINPUT_GAMEPAD_START, XINPUT_GAMEPAD_X,
                    XINPUT_GAMEPAD_Y, XINPUT_STATE, XInputGetState,
                },
            },
            Shell::{
                NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
                Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallNextHookEx, CallWindowProcW, CreateIcon, CreateIconFromResourceEx,
                CreatePopupMenu, CreateWindowExW, DI_NORMAL, DefWindowProcW, DestroyMenu,
                DestroyWindow, DispatchMessageW, DrawIconEx, FindWindowW, GWL_WNDPROC,
                GetCursorPos, GetMessageW, GetSystemMetrics, HHOOK, HICON, HMENU, IDC_ARROW,
                IDI_APPLICATION, IsIconic, KBDLLHOOKSTRUCT, KillTimer, LR_DEFAULTSIZE, LoadCursorW,
                LoadIconW, MENU_ITEM_FLAGS, MSG, MSLLHOOKSTRUCT, PostQuitMessage, RegisterClassW,
                RegisterWindowMessageW, SC_KEYMENU, SM_CXSCREEN, SM_CYSCREEN, SW_RESTORE, SW_SHOW,
                SendMessageW, SetForegroundWindow, SetMenuItemBitmaps, SetTimer, SetWindowLongPtrW,
                SetWindowsHookExW, ShowWindow, TPM_BOTTOMALIGN, TPM_LEFTALIGN, TPM_RETURNCMD,
                TrackPopupMenu, TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
                WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE,
                WM_DPICHANGED, WM_DWMCOMPOSITIONCHANGED, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK,
                WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN,
                WM_RBUTTONUP, WM_SETTINGCHANGE, WM_SYSCOMMAND, WM_THEMECHANGED, WM_TIMER,
                WM_WINDOWPOSCHANGED, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WNDPROC,
                WS_OVERLAPPED,
            },
        },
    },
    core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, PROPVARIANT, PWSTR, w},
};

mod gui;
mod native_overlay;
pub(crate) mod overlay_icons;
pub mod updater;

const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE_MUTE: u32 = WM_APP + 2;
const WM_OVERLAY_POSITIONING: u32 = WM_APP + 3;
const WM_MUTE: u32 = WM_APP + 4;
const WM_UNMUTE: u32 = WM_APP + 5;
const WM_OPEN_SETTINGS: u32 = WM_APP + 6;
const WM_UPDATE_NOW: u32 = WM_APP + 7;
const WM_EXIT_ALL: u32 = WM_APP + 8;
const WM_WHATS_NEW: u32 = WM_APP + 9;
const ID_TRAY: u32 = 1;
const ID_STATE_TIMER: usize = 10;
const ID_OVERLAY_HIDE_TIMER: usize = 11;
const ID_OVERLAY_DRAG_TIMER: usize = 12;
const ID_TRAY_CLICK_TIMER: usize = 13;
const ID_TRAY_ADD_RETRY_TIMER: usize = 14;
const ID_MENU_TOGGLE: usize = 1001;
const ID_MENU_SETTINGS: usize = 1002;
const ID_MENU_EXIT: usize = 1003;
const ID_MENU_TITLE: usize = 1004;
const ID_MENU_INPUT_DEVICE_BASE: usize = 2000;
const ID_MENU_OUTPUT_DEVICE_BASE: usize = 3000;
const MENU_POS_DEFAULT_INPUT: u32 = 2;
const MENU_POS_DEFAULT_OUTPUT: u32 = 3;
const SETTINGS_WINDOW_TITLE: &str = "silence!";
const APP_USER_MODEL_ID: &str = "Silence.SilenceV2";
const APP_PROTOCOL: &str = "silence";
const ARG_POST_INSTALL: &str = "--post-install";
const MAIN_INSTANCE_MUTEX: PCWSTR = w!("SilenceV2BackgroundApp");
const DWMWA_MICA_EFFECT: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(1029);
const TRAY_DOUBLE_CLICK_DELAY_MS: u32 = 500;
const TRAY_ADD_RETRY_MS: u32 = 2_000;
const ICON_RESOURCE_VERSION: u32 = 0x0003_0000;
const STARTUP_RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_RUN_VALUE: &str = "silence!";
const XINPUT_TRIGGER_PRESS_THRESHOLD: u8 = 160;
const XINPUT_TRIGGER_RELEASE_THRESHOLD: u8 = 96;
const MAX_GAMEPAD_COMBO_INPUTS: usize = 2;

pub(crate) const HOTKEY_TARGET_ALL_MICROPHONES: &str = "__all_microphones__";

const VK_SHIFT: u32 = 0x10;
const VK_CONTROL: u32 = 0x11;
const VK_MENU: u32 = 0x12;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_NUMPAD0: u32 = 0x60;
const VK_F1: u32 = 0x70;
const VK_LBUTTON: u32 = 0x01;
const VK_RBUTTON: u32 = 0x02;
const VK_MBUTTON: u32 = 0x04;
const VK_XBUTTON1: u32 = 0x05;
const VK_XBUTTON2: u32 = 0x06;
const XBUTTON1: u32 = 0x0001;
const XBUTTON2: u32 = 0x0002;
const LLKHF_EXTENDED: u32 = 0x01;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Shortcut {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    vk: u32,
    #[serde(default)]
    mouse_buttons: Vec<u32>,
}

impl Default for Shortcut {
    fn default() -> Self {
        Self {
            ctrl: true,
            alt: true,
            shift: false,
            win: false,
            vk: b'M' as u32,
            mouse_buttons: Vec::new(),
        }
    }
}

impl Shortcut {
    fn normalized(mut self) -> Self {
        self.mouse_buttons
            .retain(|button| is_supported_mouse_button(*button));
        self.mouse_buttons
            .sort_by_key(|button| mouse_button_sort_key(*button));
        self.mouse_buttons.dedup();
        if !self.mouse_buttons.is_empty() {
            self.vk = 0;
        }
        self
    }

    fn is_pressed(
        &self,
        vk: u32,
        ignore_modifiers: bool,
        modifiers: &ModifierState,
        mouse_buttons_down: &HashSet<u32>,
    ) -> bool {
        if !self.mouse_buttons.is_empty() {
            if !self
                .mouse_buttons
                .iter()
                .all(|button| mouse_buttons_down.contains(button))
            {
                return false;
            }
            let has_modifier = self.ctrl || self.alt || self.shift || self.win;
            let event_is_mouse_button = self.mouse_buttons.contains(&vk);
            let event_is_required_modifier = has_modifier && is_modifier(vk);
            if !event_is_mouse_button && !event_is_required_modifier {
                return false;
            }
            if ignore_modifiers {
                return true;
            }
            return self.ctrl == modifiers.ctrl
                && self.alt == modifiers.alt
                && self.shift == modifiers.shift
                && self.win == modifiers.win;
        }

        if self.vk == 0 {
            if ignore_modifiers {
                return (!self.ctrl || modifiers.ctrl)
                    && (!self.alt || modifiers.alt)
                    && (!self.shift || modifiers.shift)
                    && (!self.win || modifiers.win);
            }
            return self.ctrl == modifiers.ctrl
                && self.alt == modifiers.alt
                && self.shift == modifiers.shift
                && self.win == modifiers.win;
        }

        if ignore_modifiers {
            return self.vk == vk;
        }

        self.vk == vk
            && self.ctrl == modifiers.ctrl
            && self.alt == modifiers.alt
            && self.shift == modifiers.shift
            && self.win == modifiers.win
    }

    fn display(&self) -> String {
        let parts = self.parts();
        if parts.is_empty() {
            return "None".to_string();
        }
        parts.join(" + ")
    }

    pub fn parts(&self) -> Vec<String> {
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
        for button in &self.mouse_buttons {
            let button = *button;
            parts.push(mouse_button_name(button).to_string());
        }
        if self.vk != 0 {
            parts.push(vk_name(self.vk));
        }
        parts
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum GamepadButton {
    South,
    East,
    North,
    West,
    C,
    Z,
    LeftTrigger,
    LeftTrigger2,
    RightTrigger,
    RightTrigger2,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
}

impl GamepadButton {
    fn from_gilrs(button: Button) -> Option<Self> {
        Some(match button {
            Button::South => Self::South,
            Button::East => Self::East,
            Button::North => Self::North,
            Button::West => Self::West,
            Button::C => Self::C,
            Button::Z => Self::Z,
            Button::LeftTrigger => Self::LeftTrigger,
            Button::LeftTrigger2 => Self::LeftTrigger2,
            Button::RightTrigger => Self::RightTrigger,
            Button::RightTrigger2 => Self::RightTrigger2,
            Button::Select => Self::Select,
            Button::Start => Self::Start,
            Button::Mode => Self::Mode,
            Button::LeftThumb => Self::LeftThumb,
            Button::RightThumb => Self::RightThumb,
            Button::DPadUp => Self::DPadUp,
            Button::DPadDown => Self::DPadDown,
            Button::DPadLeft => Self::DPadLeft,
            Button::DPadRight => Self::DPadRight,
            Button::Unknown => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::South => "South",
            Self::East => "East",
            Self::North => "North",
            Self::West => "West",
            Self::C => "C",
            Self::Z => "Z",
            Self::LeftTrigger => "LB",
            Self::LeftTrigger2 => "LT",
            Self::RightTrigger => "RB",
            Self::RightTrigger2 => "RT",
            Self::Select => "Select",
            Self::Start => "Start",
            Self::Mode => "Mode",
            Self::LeftThumb => "Left Stick",
            Self::RightThumb => "Right Stick",
            Self::DPadUp => "D-Pad Up",
            Self::DPadDown => "D-Pad Down",
            Self::DPadLeft => "D-Pad Left",
            Self::DPadRight => "D-Pad Right",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    LeftZ,
    RightStickX,
    RightStickY,
    RightZ,
    DPadX,
    DPadY,
}

impl GamepadAxis {
    fn label(self) -> &'static str {
        match self {
            Self::LeftStickX => "Left Stick",
            Self::LeftStickY => "Left Stick",
            Self::LeftZ => "Left Trigger",
            Self::RightStickX => "Right Stick",
            Self::RightStickY => "Right Stick",
            Self::RightZ => "Right Trigger",
            Self::DPadX => "D-Pad",
            Self::DPadY => "D-Pad",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum GamepadAxisDirection {
    Positive,
    Negative,
}

impl GamepadAxisDirection {}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(tag = "kind")]
pub enum GamepadInput {
    Button {
        button: GamepadButton,
    },
    Axis {
        axis: GamepadAxis,
        direction: GamepadAxisDirection,
    },
}

impl GamepadInput {
    pub fn label(self) -> String {
        match self {
            Self::Button { button } => button.label().to_string(),
            Self::Axis { axis, direction } => {
                let suffix = match (axis, direction) {
                    (
                        GamepadAxis::LeftStickX | GamepadAxis::RightStickX | GamepadAxis::DPadX,
                        GamepadAxisDirection::Positive,
                    ) => "Right",
                    (
                        GamepadAxis::LeftStickX | GamepadAxis::RightStickX | GamepadAxis::DPadX,
                        GamepadAxisDirection::Negative,
                    ) => "Left",
                    (
                        GamepadAxis::LeftStickY | GamepadAxis::RightStickY | GamepadAxis::DPadY,
                        GamepadAxisDirection::Positive,
                    ) => "Up",
                    (
                        GamepadAxis::LeftStickY | GamepadAxis::RightStickY | GamepadAxis::DPadY,
                        GamepadAxisDirection::Negative,
                    ) => "Down",
                    (_, GamepadAxisDirection::Positive) => "+",
                    (_, GamepadAxisDirection::Negative) => "-",
                };
                format!("{} {suffix}", axis.label())
            }
        }
    }

    pub fn icon_id(self) -> Option<&'static str> {
        let Self::Button { button } = self else {
            return None;
        };
        match button {
            GamepadButton::South => Some("xbox_button_a"),
            GamepadButton::East => Some("xbox_button_b"),
            GamepadButton::North => Some("xbox_button_y"),
            GamepadButton::West => Some("xbox_button_x"),
            GamepadButton::LeftTrigger => Some("xbox_lb"),
            GamepadButton::LeftTrigger2 => Some("xbox_lt"),
            GamepadButton::RightTrigger => Some("xbox_rb"),
            GamepadButton::RightTrigger2 => Some("xbox_rt"),
            GamepadButton::Select => Some("xbox_button_view"),
            GamepadButton::Start => Some("xbox_button_menu"),
            GamepadButton::LeftThumb => Some("xbox_ls"),
            GamepadButton::RightThumb => Some("xbox_rs"),
            GamepadButton::DPadUp => Some("xbox_dpad_up_outline"),
            GamepadButton::DPadDown => Some("xbox_dpad_down_outline"),
            GamepadButton::DPadLeft => Some("xbox_dpad_left_outline"),
            GamepadButton::DPadRight => Some("xbox_dpad_right_outline"),
            GamepadButton::Mode => Some("xbox_button_share"),
            GamepadButton::C | GamepadButton::Z => None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct GamepadShortcut {
    #[serde(default)]
    pub inputs: Vec<GamepadInput>,
}

impl GamepadShortcut {
    pub fn parts(&self) -> Vec<String> {
        self.inputs.iter().map(|input| input.label()).collect()
    }

    fn normalized(mut self) -> Option<Self> {
        self.inputs
            .retain(|input| matches!(input, GamepadInput::Button { .. }));
        self.inputs.dedup();
        self.inputs.truncate(MAX_GAMEPAD_COMBO_INPUTS);
        if self.inputs.is_empty() {
            None
        } else {
            Some(self)
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum HotkeyAction {
    ToggleMute,
    Mute,
    Unmute,
    HoldToToggle,
    HoldToMute,
    HoldToUnmute,
    SetDefaultInputDevice,
    SetDefaultOutputDevice,
    ToggleDefaultInputDevice,
    ToggleDefaultOutputDevice,
    OpenSettings,
}

impl HotkeyAction {
    pub const ALL: &'static [Self] = &[
        Self::ToggleMute,
        Self::Mute,
        Self::Unmute,
        Self::HoldToToggle,
        Self::HoldToMute,
        Self::HoldToUnmute,
        Self::SetDefaultInputDevice,
        Self::ToggleDefaultInputDevice,
        Self::SetDefaultOutputDevice,
        Self::ToggleDefaultOutputDevice,
        Self::OpenSettings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ToggleMute => "Toggle mic state",
            Self::Mute => "Mute microphone",
            Self::Unmute => "Unmute microphone",
            Self::HoldToToggle => "Hold to toggle state",
            Self::HoldToMute => "Hold to mute",
            Self::HoldToUnmute => "Hold to unmute",
            Self::SetDefaultInputDevice => "Set default input device",
            Self::SetDefaultOutputDevice => "Set default output device",
            Self::ToggleDefaultInputDevice => "Toggle default input device",
            Self::ToggleDefaultOutputDevice => "Toggle default output device",
            Self::OpenSettings => "Open settings",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::ToggleMute => "ToggleMute",
            Self::Mute => "Mute",
            Self::Unmute => "Unmute",
            Self::HoldToToggle => "HoldToToggle",
            Self::HoldToMute => "HoldToMute",
            Self::HoldToUnmute => "HoldToUnmute",
            Self::SetDefaultInputDevice => "SetDefaultInputDevice",
            Self::SetDefaultOutputDevice => "SetDefaultOutputDevice",
            Self::ToggleDefaultInputDevice => "ToggleDefaultInputDevice",
            Self::ToggleDefaultOutputDevice => "ToggleDefaultOutputDevice",
            Self::OpenSettings => "OpenSettings",
        }
    }

    pub fn from_id(id: &str) -> Self {
        match id {
            "Mute" => Self::Mute,
            "Unmute" => Self::Unmute,
            "HoldToToggle" => Self::HoldToToggle,
            "HoldToMute" => Self::HoldToMute,
            "HoldToUnmute" => Self::HoldToUnmute,
            "SetDefaultInputDevice" => Self::SetDefaultInputDevice,
            "SetDefaultOutputDevice" => Self::SetDefaultOutputDevice,
            "ToggleDefaultInputDevice" => Self::ToggleDefaultInputDevice,
            "ToggleDefaultOutputDevice" => Self::ToggleDefaultOutputDevice,
            "OpenSettings" => Self::OpenSettings,
            _ => Self::ToggleMute,
        }
    }

    pub fn needs_target(self) -> bool {
        matches!(
            self,
            Self::ToggleMute
                | Self::Mute
                | Self::Unmute
                | Self::HoldToToggle
                | Self::HoldToMute
                | Self::HoldToUnmute
                | Self::SetDefaultInputDevice
                | Self::SetDefaultOutputDevice
                | Self::ToggleDefaultInputDevice
                | Self::ToggleDefaultOutputDevice
        )
    }

    pub fn requires_explicit_target(self) -> bool {
        matches!(
            self,
            Self::SetDefaultInputDevice
                | Self::SetDefaultOutputDevice
                | Self::ToggleDefaultInputDevice
                | Self::ToggleDefaultOutputDevice
        )
    }

    pub fn needs_second_target(self) -> bool {
        matches!(
            self,
            Self::ToggleDefaultInputDevice | Self::ToggleDefaultOutputDevice
        )
    }

    pub fn is_hold(self) -> bool {
        matches!(
            self,
            Self::HoldToToggle | Self::HoldToMute | Self::HoldToUnmute
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HotkeyBinding {
    #[serde(default = "default_hotkey_id")]
    pub id: String,
    #[serde(default)]
    pub action: HotkeyAction,
    #[serde(default)]
    pub shortcut: Shortcut,
    #[serde(default)]
    pub gamepad: Option<GamepadShortcut>,
    #[serde(default)]
    pub ignore_modifiers: bool,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_2: Option<String>,
}

impl Default for HotkeyAction {
    fn default() -> Self {
        Self::ToggleMute
    }
}

impl Default for HotkeyBinding {
    fn default() -> Self {
        Self {
            id: default_hotkey_id(),
            action: HotkeyAction::ToggleMute,
            shortcut: Shortcut::default(),
            gamepad: None,
            ignore_modifiers: false,
            target: None,
            target_2: None,
        }
    }
}

fn default_hotkey_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("hotkey-{nanos}")
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
struct Config {
    #[serde(default)]
    welcome_completed: bool,
    #[serde(default)]
    shortcut: Shortcut,
    #[serde(default)]
    hotkeys: Vec<HotkeyBinding>,
    #[serde(default)]
    hotkeys_paused: bool,
    #[serde(default)]
    startup: StartupSettings,
    #[serde(default)]
    sound_settings: SoundSettings,
    #[serde(default)]
    hold_to_mute: HoldToMuteSettings,
    #[serde(default)]
    auto_mute: AutoMuteSettings,
    #[serde(default)]
    overlay: OverlayConfig,
    #[serde(default)]
    tray_icon: TrayIconConfig,
    #[serde(default)]
    advanced: AdvancedSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            welcome_completed: false,
            shortcut: Shortcut::default(),
            hotkeys: vec![HotkeyBinding::default()],
            hotkeys_paused: false,
            startup: StartupSettings::default(),
            sound_settings: SoundSettings::default(),
            hold_to_mute: HoldToMuteSettings::default(),
            auto_mute: AutoMuteSettings::default(),
            overlay: OverlayConfig::default(),
            tray_icon: TrayIconConfig::default(),
            advanced: AdvancedSettings::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct StartupSettings {
    #[serde(default = "default_launch_on_startup")]
    pub launch_on_startup: bool,
}

impl Default for StartupSettings {
    fn default() -> Self {
        Self {
            launch_on_startup: default_launch_on_startup(),
        }
    }
}

fn default_launch_on_startup() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AdvancedSettings {
    #[serde(default)]
    pub disable_tray_double_click_settings: bool,
    #[serde(default = "default_enable_mica")]
    pub enable_mica: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            disable_tray_double_click_settings: false,
            enable_mica: default_enable_mica(),
        }
    }
}

fn default_enable_mica() -> bool {
    settings_mica_available()
}

pub(crate) fn settings_mica_available() -> bool {
    windows_build_number()
        .map(|build| build >= 22_000)
        .unwrap_or(false)
}

pub(crate) fn effective_settings_mica_enabled(config: &Config) -> bool {
    config.advanced.enable_mica && settings_mica_available()
}

fn windows_build_number() -> Option<u32> {
    let mut data = [0_u16; 32];
    let mut data_size = (data.len() * size_of::<u16>()) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
            w!("CurrentBuildNumber"),
            RRF_RT_REG_SZ,
            None,
            Some(data.as_mut_ptr() as *mut c_void),
            Some(&mut data_size),
        )
    };
    if status != ERROR_SUCCESS || data_size < size_of::<u16>() as u32 {
        return None;
    }

    let len = data
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(data.len());
    String::from_utf16_lossy(&data[..len]).parse().ok()
}

struct AppState {
    hwnd: HWND,
    hook: HHOOK,
    mouse_hook: HHOOK,
    shortcut: Shortcut,
    hotkeys: Vec<HotkeyBinding>,
    hotkeys_paused: bool,
    sound_settings: SoundSettings,
    hold_to_mute: HoldToMuteSettings,
    auto_mute: AutoMuteSettings,
    overlay: OverlayConfig,
    tray_icon: TrayIconConfig,
    advanced: AdvancedSettings,
    welcome_completed: bool,
    modifiers: ModifierState,
    muted: bool,
    shortcut_down: bool,
    hotkeys_down: HashSet<String>,
    mouse_buttons_down: HashSet<u32>,
    gamepad_inputs_down: HashSet<GamepadInput>,
    gamepad_hotkeys_down: HashSet<String>,
    active_hold_hotkeys: HashMap<String, ActiveHoldHotkey>,
    last_auto_mute_input_tick: u32,
    auto_muted_by_inactivity: bool,
    auto_mute_cursor_position: POINT,
    config_modified: Option<SystemTime>,
}

#[derive(Clone, Debug)]
struct ActiveHoldHotkey {
    target: Option<String>,
    previous_muted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NotificationAction {
    ToggleMute,
    Mute,
    Unmute,
    OpenSettings,
    UpdateNow,
    ViewUpdate,
    WhatsNew,
    ExitAll,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LastUpdateNotification {
    version: String,
    release_url: String,
    #[serde(default)]
    shown: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct ModifierState {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct OverlayConfig {
    #[serde(default = "default_overlay_enabled")]
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
    #[serde(default = "default_overlay_variant")]
    pub variant: String,
    #[serde(default = "crate::overlay_icons::default_overlay_icon_pair")]
    pub icon_pair: String,
    #[serde(default = "default_overlay_icon_style")]
    pub icon_style: String,
    #[serde(default = "default_overlay_background_style")]
    pub background_style: String,
    #[serde(default = "default_overlay_background_opacity")]
    pub background_opacity: u8,
    #[serde(default = "default_overlay_content_opacity")]
    pub content_opacity: u8,
    #[serde(default = "default_overlay_border_radius")]
    pub border_radius: u8,
    #[serde(default = "default_overlay_show_border")]
    pub show_border: bool,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: default_overlay_enabled(),
            visibility: default_overlay_visibility(),
            position_x: default_overlay_position_x(),
            position_y: default_overlay_position_y(),
            duration_secs: default_overlay_duration_secs(),
            scale: default_overlay_scale(),
            show_text: false,
            variant: default_overlay_variant(),
            icon_pair: crate::overlay_icons::default_overlay_icon_pair(),
            icon_style: default_overlay_icon_style(),
            background_style: default_overlay_background_style(),
            background_opacity: default_overlay_background_opacity(),
            content_opacity: default_overlay_content_opacity(),
            border_radius: default_overlay_border_radius(),
            show_border: default_overlay_show_border(),
        }
    }
}

fn default_overlay_enabled() -> bool {
    true
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

fn default_overlay_variant() -> String {
    "MicIcon".to_string()
}

fn default_overlay_icon_style() -> String {
    "Colored".to_string()
}

fn default_overlay_background_style() -> String {
    "Dark".to_string()
}

fn default_overlay_background_opacity() -> u8 {
    90
}

fn default_overlay_content_opacity() -> u8 {
    100
}

fn default_overlay_border_radius() -> u8 {
    6
}

fn default_overlay_show_border() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrayIconConfig {
    #[serde(default = "default_tray_icon_variant")]
    pub variant: String,
    #[serde(default = "crate::overlay_icons::default_overlay_icon_pair")]
    pub icon_pair: String,
    #[serde(default = "default_tray_icon_status_style")]
    pub status_style: String,
}

impl Default for TrayIconConfig {
    fn default() -> Self {
        Self {
            variant: default_tray_icon_variant(),
            icon_pair: crate::overlay_icons::default_overlay_icon_pair(),
            status_style: default_tray_icon_status_style(),
        }
    }
}

fn default_tray_icon_variant() -> String {
    "StatusMic".to_string()
}

fn default_tray_icon_status_style() -> String {
    "Colored".to_string()
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
    #[serde(default)]
    pub custom_sounds: Vec<CustomSound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_mute_sound: Option<CustomSound>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_unmute_sound: Option<CustomSound>,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            enabled: default_sounds_enabled(),
            volume: default_sound_volume(),
            mute_theme: default_sound_theme(),
            unmute_theme: default_sound_theme(),
            custom_sounds: Vec::new(),
            custom_mute_sound: None,
            custom_unmute_sound: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CustomSound {
    #[serde(default = "default_custom_sound_id")]
    pub id: String,
    pub path: PathBuf,
    pub original_file_name: String,
}

fn default_custom_sound_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("custom-{nanos}")
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct HoldToMuteSettings {
    #[serde(default = "default_hold_to_mute_play_sounds")]
    pub play_sounds: bool,
    #[serde(default = "default_hold_to_mute_show_overlay")]
    pub show_overlay: bool,
    #[serde(default)]
    pub volume_override: Option<u8>,
    #[serde(default)]
    pub mute_theme_override: Option<String>,
    #[serde(default)]
    pub unmute_theme_override: Option<String>,
}

impl Default for HoldToMuteSettings {
    fn default() -> Self {
        Self {
            play_sounds: default_hold_to_mute_play_sounds(),
            show_overlay: default_hold_to_mute_show_overlay(),
            volume_override: None,
            mute_theme_override: None,
            unmute_theme_override: None,
        }
    }
}

fn default_hold_to_mute_play_sounds() -> bool {
    true
}

fn default_hold_to_mute_show_overlay() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AutoMuteSettings {
    #[serde(default)]
    pub mute_on_startup: bool,
    #[serde(default)]
    pub after_inactivity_enabled: bool,
    #[serde(default = "default_auto_mute_after_inactivity_minutes")]
    pub after_inactivity_minutes: u16,
    #[serde(default)]
    pub unmute_on_activity: bool,
    #[serde(default = "default_auto_mute_play_sounds")]
    pub play_sounds: bool,
}

impl Default for AutoMuteSettings {
    fn default() -> Self {
        Self {
            mute_on_startup: false,
            after_inactivity_enabled: false,
            after_inactivity_minutes: default_auto_mute_after_inactivity_minutes(),
            unmute_on_activity: false,
            play_sounds: default_auto_mute_play_sounds(),
        }
    }
}

fn default_auto_mute_after_inactivity_minutes() -> u16 {
    5
}

fn default_auto_mute_play_sounds() -> bool {
    true
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct WindowsAccent {
    accent: (u8, u8, u8),
}

const FALLBACK_WINDOWS_ACCENT: (u8, u8, u8) = (250, 121, 48);

impl Default for WindowsAccent {
    fn default() -> Self {
        Self {
            accent: FALLBACK_WINDOWS_ACCENT,
        }
    }
}

impl WindowsAccent {
    pub fn load() -> Self {
        let fallback = Self::default();
        Self {
            accent: read_windows_accent_dword()
                .map(windows_accent_to_rgb)
                .filter(|accent| *accent != (0, 0, 0))
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

fn windows_uses_light_system_theme() -> bool {
    read_registry_dword(
        w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
        w!("SystemUsesLightTheme"),
    )
    .unwrap_or(0)
        != 0
}

fn state_accent(muted: bool) -> (u8, u8, u8) {
    if muted { (220, 53, 69) } else { (40, 167, 69) }
}

impl Default for AppState {
    fn default() -> Self {
        let config = load_config().unwrap_or_default();
        Self {
            hwnd: HWND(null_mut()),
            hook: HHOOK(null_mut()),
            mouse_hook: HHOOK(null_mut()),
            shortcut: config.shortcut.clone(),
            hotkeys: config.hotkeys,
            hotkeys_paused: config.hotkeys_paused,
            sound_settings: config.sound_settings,
            hold_to_mute: config.hold_to_mute,
            auto_mute: config.auto_mute,
            overlay: config.overlay,
            tray_icon: config.tray_icon,
            advanced: config.advanced,
            welcome_completed: config.welcome_completed,
            modifiers: ModifierState::default(),
            muted: false,
            shortcut_down: false,
            hotkeys_down: HashSet::new(),
            mouse_buttons_down: HashSet::new(),
            gamepad_inputs_down: HashSet::new(),
            gamepad_hotkeys_down: HashSet::new(),
            active_hold_hotkeys: HashMap::new(),
            last_auto_mute_input_tick: 0,
            auto_muted_by_inactivity: false,
            auto_mute_cursor_position: POINT::default(),
            config_modified: config_modified_time(),
        }
    }
}

unsafe impl Send for AppState {}

static STATE: Lazy<Mutex<AppState>> = Lazy::new(|| Mutex::new(AppState::default()));
static AUDIO_ENGINE: Lazy<Mutex<Option<AudioEngine>>> = Lazy::new(|| Mutex::new(None));
static SETTINGS_HOTKEY_RECORDING: AtomicBool = AtomicBool::new(false);
static SETTINGS_ALT_SPACE_RECORDED: AtomicBool = AtomicBool::new(false);
static SETTINGS_GAMEPAD_RECORDING: AtomicBool = AtomicBool::new(false);
static MOUSE_HOTKEYS_ENABLED: AtomicBool = AtomicBool::new(true);
static SUPPRESS_NEXT_TRAY_LBUTTON_UP: AtomicBool = AtomicBool::new(false);
static TRAY_ICON_ADDED: AtomicBool = AtomicBool::new(false);
static SETTINGS_MICA_ENABLED: AtomicBool = AtomicBool::new(false);
static TASKBAR_CREATED_MESSAGE: Lazy<u32> =
    Lazy::new(|| unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) });
static SETTINGS_GAMEPAD_HELD: Lazy<Mutex<HashSet<GamepadInput>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));
static SETTINGS_MOUSE_HELD: Lazy<Mutex<Vec<u32>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SETTINGS_MOUSE_PRESSED_SHORTCUT: Lazy<Mutex<Option<Shortcut>>> =
    Lazy::new(|| Mutex::new(None));
static TRAY_DEVICE_COMMANDS: Lazy<Mutex<HashMap<usize, TrayDeviceCommand>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static PENDING_NOTIFICATION_ACTION: Lazy<Mutex<Option<NotificationAction>>> =
    Lazy::new(|| Mutex::new(None));
static SETTINGS_ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static GILRS_MONITOR_STARTED: AtomicBool = AtomicBool::new(false);
static XINPUT_MONITOR_STARTED: AtomicBool = AtomicBool::new(false);

#[cfg(target_pointer_width = "32")]
type WindowLongPtrValue = i32;
#[cfg(target_pointer_width = "64")]
type WindowLongPtrValue = isize;

#[repr(transparent)]
#[derive(Clone, PartialEq, Eq)]
struct IPolicyConfig(IUnknown);

unsafe impl Interface for IPolicyConfig {
    type Vtable = IPolicyConfigVtbl;
    const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
}

impl core::ops::Deref for IPolicyConfig {
    type Target = IUnknown;

    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}

#[repr(C)]
#[allow(non_snake_case)]
struct IPolicyConfigVtbl {
    base__: IUnknown_Vtbl,
    GetMixFormat: unsafe extern "system" fn(*mut c_void, PCWSTR, *mut *mut c_void) -> HRESULT,
    GetDeviceFormat:
        unsafe extern "system" fn(*mut c_void, PCWSTR, i32, *mut *mut c_void) -> HRESULT,
    ResetDeviceFormat: unsafe extern "system" fn(*mut c_void, PCWSTR) -> HRESULT,
    SetDeviceFormat:
        unsafe extern "system" fn(*mut c_void, PCWSTR, *const c_void, *const c_void) -> HRESULT,
    GetProcessingPeriod:
        unsafe extern "system" fn(*mut c_void, PCWSTR, i32, *mut i64, *mut i64) -> HRESULT,
    SetProcessingPeriod: unsafe extern "system" fn(*mut c_void, PCWSTR, *const i64) -> HRESULT,
    GetShareMode: unsafe extern "system" fn(*mut c_void, PCWSTR, *mut c_void) -> HRESULT,
    SetShareMode: unsafe extern "system" fn(*mut c_void, PCWSTR, *const c_void) -> HRESULT,
    GetPropertyValue:
        unsafe extern "system" fn(*mut c_void, PCWSTR, *const c_void, *mut PROPVARIANT) -> HRESULT,
    SetPropertyValue: unsafe extern "system" fn(
        *mut c_void,
        PCWSTR,
        *const c_void,
        *const PROPVARIANT,
    ) -> HRESULT,
    SetDefaultEndpoint: unsafe extern "system" fn(*mut c_void, PCWSTR, ERole) -> HRESULT,
    SetEndpointVisibility: unsafe extern "system" fn(*mut c_void, PCWSTR, i32) -> HRESULT,
}

const CLSID_POLICY_CONFIG_CLIENT: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

#[derive(Clone, Debug)]
enum TrayDeviceCommand {
    Input(String),
    Output(String),
}

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
                dioxus::desktop::icon_from_memory(include_bytes!("../assets/app.png"))
                    .expect("load app icon"),
            )
            .with_custom_head(gui::settings_startup_head())
            .with_background_color((0, 0, 0, 0));
        MOUSE_HOTKEYS_ENABLED.store(false, Ordering::Relaxed);
        install_mouse_hook(unsafe { GetModuleHandleW(None)? }.into())?;
        dioxus::LaunchBuilder::desktop().with_cfg(cfg).launch(|| {
            start_gamepad_monitor(false);
            start_xinput_monitor(false);
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
        NotificationAction::UpdateNow => WM_OPEN_SETTINGS,
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
            for user_index in 0..4 {
                let mut state = XINPUT_STATE::default();
                let result = unsafe { XInputGetState(user_index, &mut state) };
                let connected = result == ERROR_SUCCESS.0;
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
                    for (mask, label, input) in xinput_button_inputs() {
                        if changed & mask != 0 {
                            let down = buttons & mask != 0;
                            eprintln!(
                                "xinput controller {} button {} {}",
                                user_index,
                                label,
                                if down { "pressed" } else { "released" }
                            );
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
                    eprintln!(
                        "xinput controller {} trigger LT {}",
                        user_index,
                        if left_trigger { "pressed" } else { "released" }
                    );
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
                    eprintln!(
                        "xinput controller {} trigger RT {}",
                        user_index,
                        if right_trigger { "pressed" } else { "released" }
                    );
                    handle_gamepad_input_change(
                        GamepadInput::Button {
                            button: GamepadButton::RightTrigger2,
                        },
                        right_trigger,
                        enable_hotkeys,
                    );
                }
            }
            thread::sleep(Duration::from_millis(16));
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

fn xinput_button_inputs() -> [(u16, &'static str, GamepadInput); 14] {
    [
        (
            XINPUT_GAMEPAD_A.0,
            "A",
            GamepadInput::Button {
                button: GamepadButton::South,
            },
        ),
        (
            XINPUT_GAMEPAD_B.0,
            "B",
            GamepadInput::Button {
                button: GamepadButton::East,
            },
        ),
        (
            XINPUT_GAMEPAD_X.0,
            "X",
            GamepadInput::Button {
                button: GamepadButton::West,
            },
        ),
        (
            XINPUT_GAMEPAD_Y.0,
            "Y",
            GamepadInput::Button {
                button: GamepadButton::North,
            },
        ),
        (
            XINPUT_GAMEPAD_LEFT_SHOULDER.0,
            "LB",
            GamepadInput::Button {
                button: GamepadButton::LeftTrigger,
            },
        ),
        (
            XINPUT_GAMEPAD_RIGHT_SHOULDER.0,
            "RB",
            GamepadInput::Button {
                button: GamepadButton::RightTrigger,
            },
        ),
        (
            XINPUT_GAMEPAD_LEFT_THUMB.0,
            "LeftThumb",
            GamepadInput::Button {
                button: GamepadButton::LeftThumb,
            },
        ),
        (
            XINPUT_GAMEPAD_RIGHT_THUMB.0,
            "RightThumb",
            GamepadInput::Button {
                button: GamepadButton::RightThumb,
            },
        ),
        (
            XINPUT_GAMEPAD_BACK.0,
            "Back",
            GamepadInput::Button {
                button: GamepadButton::Select,
            },
        ),
        (
            XINPUT_GAMEPAD_START.0,
            "Start",
            GamepadInput::Button {
                button: GamepadButton::Start,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_UP.0,
            "DPadUp",
            GamepadInput::Button {
                button: GamepadButton::DPadUp,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_DOWN.0,
            "DPadDown",
            GamepadInput::Button {
                button: GamepadButton::DPadDown,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_LEFT.0,
            "DPadLeft",
            GamepadInput::Button {
                button: GamepadButton::DPadLeft,
            },
        ),
        (
            XINPUT_GAMEPAD_DPAD_RIGHT.0,
            "DPadRight",
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
    let overlay_config = STATE.lock().unwrap().overlay.clone();
    {
        let mut state = STATE.lock().unwrap();
        state.hwnd = hwnd;
        state.muted = muted;
    }
    register_notification_integration();
    updater::cleanup_downloads_after_startup();
    maybe_show_updated_notification();
    if !launched_from_installer() {
        start_update_check();
    }
    let sound_settings = STATE.lock().unwrap().sound_settings.clone();
    native_overlay::init(instance.into(), muted, &overlay_config)?;
    apply_overlay_visibility();
    prime_sound_assets(&sound_settings);

    install_keyboard_hook(instance.into())?;
    install_mouse_hook(instance.into())?;
    start_gamepad_monitor(true);
    start_xinput_monitor(true);
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
        eprintln!(
            "gilrs initialized; connected gamepads: {}",
            gilrs.gamepads().count()
        );
        for (id, gamepad) in gilrs.gamepads() {
            eprintln!(
                "gamepad connected at startup: {:?} - {}",
                id,
                gamepad.name()
            );
        }

        loop {
            while let Some(event) = gilrs.next_event() {
                eprintln!(
                    "gilrs event: gamepad={:?}, event={:?}",
                    event.id, event.event
                );
                handle_gamepad_event(event.event, enable_hotkeys);
            }
            thread::sleep(Duration::from_millis(8));
        }
    });
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

fn add_tray_icon(hwnd: HWND) -> Result<()> {
    if TRAY_ICON_ADDED.load(Ordering::Relaxed) {
        return Ok(());
    }

    let config = STATE.lock().unwrap().tray_icon.clone();
    let muted = STATE.lock().unwrap().muted;
    let icon = load_tray_icon(&config, muted)
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

    let (hwnd, muted, config) = {
        let state = STATE.lock().unwrap();
        if state.hwnd.0.is_null() {
            return;
        }
        (state.hwnd, state.muted, state.tray_icon.clone())
    };
    let Some(icon) = load_tray_icon(&config, muted).or_else(load_app_icon) else {
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

fn load_tray_icon(config: &TrayIconConfig, muted: bool) -> Option<HICON> {
    match config.variant.as_str() {
        "StatusMic" => create_status_mic_icon(config, muted),
        "ColorDot" => create_color_dot_icon(muted),
        _ => load_app_icon(),
    }
}

fn load_app_icon() -> Option<HICON> {
    let icon_bytes = include_bytes!("../assets/app.ico");
    let image = best_ico_image(icon_bytes, 16)?;
    unsafe {
        CreateIconFromResourceEx(image, true, ICON_RESOURCE_VERSION, 0, 0, LR_DEFAULTSIZE).ok()
    }
}

fn create_status_mic_icon(config: &TrayIconConfig, muted: bool) -> Option<HICON> {
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
    create_argb_icon(32, 32, &pixels)
}

fn create_color_dot_icon(muted: bool) -> Option<HICON> {
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
    create_argb_icon(size as i32, size as i32, &pixels)
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
        let muted = STATE.lock().unwrap().muted;
        let status = if muted {
            "Unmute Microphone"
        } else {
            "Mute Microphone"
        };
        let title_w = wide(&format!("silence! - v{}", env!("CARGO_PKG_VERSION")));
        let status_w = wide(status);
        let input_w = wide("Default Input");
        let output_w = wide("Default Output");
        let settings_w = wide("Open Settings");
        let exit_w = wide("Exit");
        let input_devices = capture_devices().unwrap_or_default();
        let output_devices = render_devices().unwrap_or_default();

        {
            let mut commands = TRAY_DEVICE_COMMANDS.lock().unwrap();
            commands.clear();
            append_input_device_menu(input_menu, &input_devices, &mut commands);
            append_output_device_menu(output_menu, &output_devices, &mut commands);
        }

        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0001),
            ID_MENU_TITLE,
            PCWSTR(title_w.as_ptr()),
        );
        let _ = AppendMenuW(menu, MENU_ITEM_FLAGS(0x0000_0800), 0, PCWSTR(null()));
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0010),
            input_menu.0 as usize,
            PCWSTR(input_w.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MENU_ITEM_FLAGS(0x0000_0010),
            output_menu.0 as usize,
            PCWSTR(output_w.as_ptr()),
        );
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
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../assets/icons/microphone-3-bold.svg"),
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
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../assets/icons/microphone-3-linear.svg"),
            icon_color,
        ) {
            let _ = SetMenuItemBitmaps(
                menu,
                MENU_POS_DEFAULT_INPUT,
                MENU_ITEM_FLAGS(0x0000_0400),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../assets/icons/volume-loud-linear.svg"),
            icon_color,
        ) {
            let _ = SetMenuItemBitmaps(
                menu,
                MENU_POS_DEFAULT_OUTPUT,
                MENU_ITEM_FLAGS(0x0000_0400),
                bitmap,
                bitmap,
            );
            bitmaps.push(bitmap);
        }
        if let Some(bitmap) = create_menu_svg_bitmap(
            include_str!("../assets/icons/settings-bold.svg"),
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
            create_menu_svg_bitmap(include_str!("../assets/icons/exit-bold.svg"), icon_color)
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
        TRAY_DEVICE_COMMANDS.lock().unwrap().clear();
        for bitmap in bitmaps {
            let _ = DeleteObject(bitmap);
        }
    }
}

fn append_input_device_menu(
    menu: HMENU,
    devices: &[MicDevice],
    commands: &mut HashMap<usize, TrayDeviceCommand>,
) {
    if devices.is_empty() {
        append_disabled_menu_item(menu, "No active input devices");
        return;
    }

    for (index, device) in devices.iter().enumerate() {
        let command_id = ID_MENU_INPUT_DEVICE_BASE + index;
        commands.insert(command_id, TrayDeviceCommand::Input(device.id.clone()));
        append_device_menu_item(menu, command_id, &device.name, device.is_default);
    }
}

fn append_output_device_menu(
    menu: HMENU,
    devices: &[AudioDevice],
    commands: &mut HashMap<usize, TrayDeviceCommand>,
) {
    if devices.is_empty() {
        append_disabled_menu_item(menu, "No active output devices");
        return;
    }

    for (index, device) in devices.iter().enumerate() {
        let command_id = ID_MENU_OUTPUT_DEVICE_BASE + index;
        commands.insert(command_id, TrayDeviceCommand::Output(device.id.clone()));
        append_device_menu_item(menu, command_id, &device.name, device.is_default);
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
        None => {}
    }
}

fn handle_tray_menu_command(command_id: usize) {
    match command_id {
        ID_MENU_TOGGLE => toggle_mute(),
        ID_MENU_SETTINGS => open_settings_window(),
        ID_MENU_EXIT => {
            exit_all_processes();
        }
        command_id => handle_tray_device_command(command_id),
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

fn exit_all_processes() {
    close_settings_window();
    let hwnd = STATE.lock().unwrap().hwnd;
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    let kb = unsafe { *(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let vk = normalized_keyboard_vk(kb.vkCode, kb.scanCode, kb.flags.0);
    let is_down = event == WM_KEYDOWN || event == 0x0104;
    let is_up = event == WM_KEYUP || event == 0x0105;

    if {
        let state = STATE.lock().unwrap();
        hotkeys_blocked(&state)
    } {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    if is_down {
        let mut actions = Vec::new();
        let mut suppress_key = false;
        let modifiers;
        {
            let mut state = STATE.lock().unwrap();
            update_modifier_state(&mut state.modifiers, vk, true);
            modifiers = state.modifiers;
        }
        record_mouse_shortcut_with_modifiers(modifiers);
        {
            let mut state = STATE.lock().unwrap();
            let matching_hotkeys: Vec<HotkeyBinding> = state
                .hotkeys
                .iter()
                .filter(|hotkey| hotkey.gamepad.is_none())
                .filter(|hotkey| {
                    if hotkey.shortcut.vk == 0 && !is_modifier(vk) {
                        return false;
                    }
                    hotkey.shortcut.is_pressed(
                        vk,
                        hotkey.ignore_modifiers,
                        &modifiers,
                        &state.mouse_buttons_down,
                    )
                })
                .cloned()
                .collect();
            let exact_keys: HashSet<u32> = matching_hotkeys
                .iter()
                .filter(|hotkey| !hotkey.ignore_modifiers)
                .map(|hotkey| hotkey.shortcut.vk)
                .collect();

            for hotkey in matching_hotkeys {
                if hotkey.shortcut.vk == 0 && !is_modifier(vk) {
                    continue;
                }
                if hotkey.ignore_modifiers && exact_keys.contains(&hotkey.shortcut.vk) {
                    continue;
                }
                if shortcut_is_alt_space(&hotkey.shortcut) {
                    suppress_key = true;
                }
                if !state.hotkeys_down.contains(&hotkey.id) {
                    state.hotkeys_down.insert(hotkey.id.clone());
                    actions.push(hotkey_action_request(&hotkey));
                }
            }
        }
        for action in actions {
            run_hotkey_action(action);
        }
        if suppress_key {
            return LRESULT(1);
        }
    }

    if is_up {
        let mut actions = Vec::new();
        let mut suppress_key = false;
        let mut state = STATE.lock().unwrap();
        update_modifier_state(&mut state.modifiers, vk, false);
        let modifiers = state.modifiers;
        let released: Vec<HotkeyBinding> = state
            .hotkeys
            .iter()
            .filter(|hotkey| hotkey.gamepad.is_none())
            .filter(|hotkey| {
                hotkey.shortcut.vk == vk
                    || (hotkey.shortcut.vk == 0
                        && is_modifier(vk)
                        && !hotkey.shortcut.is_pressed(
                            vk,
                            hotkey.ignore_modifiers,
                            &modifiers,
                            &state.mouse_buttons_down,
                        ))
            })
            .cloned()
            .collect();
        for hotkey in released {
            if shortcut_is_alt_space(&hotkey.shortcut) {
                suppress_key = true;
            }
            state.hotkeys_down.remove(&hotkey.id);
            if hotkey.action.is_hold() {
                actions.push(HotkeyRequest::ReleaseHold {
                    id: hotkey.id.clone(),
                });
            }
        }
        if state.shortcut.vk == vk {
            state.shortcut_down = false;
        }
        drop(state);

        for action in actions {
            run_hotkey_action(action);
        }
        if suppress_key {
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn normalized_keyboard_vk(vk: u32, scan_code: u32, flags: u32) -> u32 {
    if flags & LLKHF_EXTENDED != 0 {
        return vk;
    }

    match scan_code {
        0x52 => VK_NUMPAD0,
        0x4F => VK_NUMPAD0 + 1,
        0x50 => VK_NUMPAD0 + 2,
        0x51 => VK_NUMPAD0 + 3,
        0x4B => VK_NUMPAD0 + 4,
        0x4C => VK_NUMPAD0 + 5,
        0x4D => VK_NUMPAD0 + 6,
        0x47 => VK_NUMPAD0 + 7,
        0x48 => VK_NUMPAD0 + 8,
        0x49 => VK_NUMPAD0 + 9,
        0x53 => 0x6E,
        _ => vk,
    }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = wparam.0 as u32;
    let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
    let Some(button) = mouse_button_from_event(event, mouse.mouseData) else {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    };
    let down = mouse_button_event_is_down(event);

    if SETTINGS_HOTKEY_RECORDING.load(Ordering::Relaxed) {
        if down {
            let mut held = SETTINGS_MOUSE_HELD.lock().unwrap();
            if !held.contains(&button) {
                held.push(button);
            }
            drop(held);
            record_mouse_shortcut_with_modifiers(current_modifier_state());
        } else {
            SETTINGS_MOUSE_HELD
                .lock()
                .unwrap()
                .retain(|held| *held != button);
        }
    }

    if !MOUSE_HOTKEYS_ENABLED.load(Ordering::Relaxed) {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let mut actions = Vec::new();
    {
        let mut state = STATE.lock().unwrap();
        if hotkeys_blocked(&state) {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }

        state.modifiers = current_modifier_state();
        if down {
            state.mouse_buttons_down.insert(button);
            actions.extend(mouse_press_actions(&mut state, button));
        } else {
            state.mouse_buttons_down.remove(&button);
            actions.extend(mouse_release_actions(&mut state, button));
        }
    }

    for action in actions {
        run_hotkey_action(action);
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn record_mouse_shortcut_with_modifiers(modifiers: ModifierState) {
    if !SETTINGS_HOTKEY_RECORDING.load(Ordering::Relaxed)
        || !(modifiers.ctrl || modifiers.alt || modifiers.shift || modifiers.win)
    {
        return;
    }

    let mut mouse_buttons = SETTINGS_MOUSE_HELD.lock().unwrap().clone();
    if mouse_buttons.is_empty() {
        return;
    }

    mouse_buttons.truncate(2);
    *SETTINGS_MOUSE_PRESSED_SHORTCUT.lock().unwrap() = Some(Shortcut {
        ctrl: modifiers.ctrl,
        alt: modifiers.alt,
        shift: modifiers.shift,
        win: modifiers.win,
        vk: 0,
        mouse_buttons,
    });
}

fn mouse_press_actions(state: &mut AppState, button: u32) -> Vec<HotkeyRequest> {
    let modifiers = state.modifiers;
    let mut matches: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| hotkey.gamepad.is_none())
        .filter(|hotkey| {
            hotkey.shortcut.is_pressed(
                button,
                hotkey.ignore_modifiers,
                &modifiers,
                &state.mouse_buttons_down,
            )
        })
        .cloned()
        .collect();

    let has_combo_match = matches
        .iter()
        .any(|hotkey| hotkey.shortcut.mouse_buttons.len() > 1);
    matches.retain(|hotkey| !has_combo_match || hotkey.shortcut.mouse_buttons.len() > 1);

    matches
        .into_iter()
        .filter_map(|hotkey| {
            if state.hotkeys_down.insert(hotkey.id.clone()) {
                Some(hotkey_action_request(&hotkey))
            } else {
                None
            }
        })
        .collect()
}

fn mouse_release_actions(state: &mut AppState, button: u32) -> Vec<HotkeyRequest> {
    let released: Vec<HotkeyBinding> = state
        .hotkeys
        .iter()
        .filter(|hotkey| {
            state.hotkeys_down.contains(&hotkey.id)
                && hotkey.shortcut.mouse_buttons.contains(&button)
        })
        .cloned()
        .collect();

    let mut actions = Vec::new();
    for hotkey in released {
        state.hotkeys_down.remove(&hotkey.id);
        if hotkey.action.is_hold() {
            actions.push(HotkeyRequest::ReleaseHold {
                id: hotkey.id.clone(),
            });
        }
    }
    actions
}

enum HotkeyRequest {
    ToggleMute {
        target: Option<String>,
    },
    SetMute {
        target: Option<String>,
        muted: bool,
    },
    StartHold {
        id: String,
        target: Option<String>,
        muted: bool,
    },
    StartHoldToggle {
        id: String,
        target: Option<String>,
    },
    ReleaseHold {
        id: String,
    },
    SetDefaultInput {
        target: Option<String>,
    },
    SetDefaultOutput {
        target: Option<String>,
    },
    ToggleDefaultInput {
        target_1: Option<String>,
        target_2: Option<String>,
    },
    ToggleDefaultOutput {
        target_1: Option<String>,
        target_2: Option<String>,
    },
    OpenSettings,
}

fn hotkey_action_request(hotkey: &HotkeyBinding) -> HotkeyRequest {
    match hotkey.action {
        HotkeyAction::ToggleMute => HotkeyRequest::ToggleMute {
            target: hotkey.target.clone(),
        },
        HotkeyAction::Mute => HotkeyRequest::SetMute {
            target: hotkey.target.clone(),
            muted: true,
        },
        HotkeyAction::Unmute => HotkeyRequest::SetMute {
            target: hotkey.target.clone(),
            muted: false,
        },
        HotkeyAction::HoldToToggle => HotkeyRequest::StartHoldToggle {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
        },
        HotkeyAction::HoldToMute => HotkeyRequest::StartHold {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
            muted: true,
        },
        HotkeyAction::HoldToUnmute => HotkeyRequest::StartHold {
            id: hotkey.id.clone(),
            target: hotkey.target.clone(),
            muted: false,
        },
        HotkeyAction::SetDefaultInputDevice => HotkeyRequest::SetDefaultInput {
            target: hotkey.target.clone(),
        },
        HotkeyAction::SetDefaultOutputDevice => HotkeyRequest::SetDefaultOutput {
            target: hotkey.target.clone(),
        },
        HotkeyAction::ToggleDefaultInputDevice => HotkeyRequest::ToggleDefaultInput {
            target_1: hotkey.target.clone(),
            target_2: hotkey.target_2.clone(),
        },
        HotkeyAction::ToggleDefaultOutputDevice => HotkeyRequest::ToggleDefaultOutput {
            target_1: hotkey.target.clone(),
            target_2: hotkey.target_2.clone(),
        },
        HotkeyAction::OpenSettings => HotkeyRequest::OpenSettings,
    }
}

fn run_hotkey_action(action: HotkeyRequest) {
    match action {
        HotkeyRequest::ToggleMute { target } => toggle_mute_target(target.as_deref()),
        HotkeyRequest::SetMute { target, muted } => set_mute_target(target.as_deref(), muted),
        HotkeyRequest::StartHold { id, target, muted } => {
            start_hold_hotkey(&id, target, muted);
        }
        HotkeyRequest::StartHoldToggle { id, target } => start_hold_toggle_hotkey(&id, target),
        HotkeyRequest::ReleaseHold { id } => release_hold_hotkey(&id),
        HotkeyRequest::SetDefaultInput { target } => {
            if let Some(device_id) = target {
                if let Err(err) = set_default_capture_device(&device_id) {
                    eprintln!("failed to set default input device from hotkey: {err:?}");
                }
            }
        }
        HotkeyRequest::SetDefaultOutput { target } => {
            if let Some(device_id) = target {
                if let Err(err) = set_default_render_device(&device_id) {
                    eprintln!("failed to set default output device from hotkey: {err:?}");
                }
            }
        }
        HotkeyRequest::ToggleDefaultInput { target_1, target_2 } => {
            if let Err(err) = toggle_default_audio_device(eCapture, target_1, target_2) {
                eprintln!("failed to toggle default input device from hotkey: {err:?}");
            }
        }
        HotkeyRequest::ToggleDefaultOutput { target_1, target_2 } => {
            if let Err(err) = toggle_default_audio_device(eRender, target_1, target_2) {
                eprintln!("failed to toggle default output device from hotkey: {err:?}");
            }
        }
        HotkeyRequest::OpenSettings => open_settings_window(),
    }
}

fn toggle_mute() {
    toggle_mute_target(None);
}

fn toggle_mute_target(device_id: Option<&str>) {
    match set_mute_to_inverse(device_id) {
        Ok(target_muted) => {
            play_mute_sound(target_muted);
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, true);
        }
        Err(err) => eprintln!("failed to toggle microphone mute: {err:?}"),
    }
}

fn set_mute_target(device_id: Option<&str>, muted: bool) {
    match set_mute(device_id, muted) {
        Ok(target_muted) => {
            play_mute_sound(target_muted);
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, true);
        }
        Err(err) => eprintln!("failed to set microphone mute: {err:?}"),
    }
}

fn apply_startup_auto_mute() {
    let settings = STATE.lock().unwrap().auto_mute.clone();
    if !settings.mute_on_startup {
        return;
    }

    if let Err(err) = apply_auto_mute(settings.play_sounds, false) {
        eprintln!("failed to apply startup auto-mute: {err:?}");
    }
}

fn start_hold_hotkey(id: &str, target: Option<String>, muted: bool) {
    let previous_muted = match target_mute_state(target.as_deref()) {
        Ok(previous_muted) => previous_muted,
        Err(err) => {
            eprintln!("failed to read hold hotkey state: {err:?}");
            return;
        }
    };

    match set_mute(target.as_deref(), muted) {
        Ok(target_muted) => {
            {
                let mut state = STATE.lock().unwrap();
                state.active_hold_hotkeys.insert(
                    id.to_string(),
                    ActiveHoldHotkey {
                        target,
                        previous_muted,
                    },
                );
            }

            let changed = previous_muted != target_muted;
            if changed {
                play_hold_to_mute_sound(target_muted);
            }
            let show_overlay = STATE.lock().unwrap().hold_to_mute.show_overlay;
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, changed && show_overlay);
        }
        Err(err) => eprintln!("failed to apply hold hotkey mute state: {err:?}"),
    }
}

fn start_hold_toggle_hotkey(id: &str, target: Option<String>) {
    let previous_muted = match target_mute_state(target.as_deref()) {
        Ok(previous_muted) => previous_muted,
        Err(err) => {
            eprintln!("failed to read hold toggle hotkey state: {err:?}");
            return;
        }
    };
    start_hold_hotkey(id, target, !previous_muted);
}

fn release_hold_hotkey(id: &str) {
    let active_hold = {
        let mut state = STATE.lock().unwrap();
        state.active_hold_hotkeys.remove(id)
    };
    let Some(active_hold) = active_hold else {
        return;
    };

    let current_muted =
        target_mute_state(active_hold.target.as_deref()).unwrap_or(active_hold.previous_muted);
    match set_mute(active_hold.target.as_deref(), active_hold.previous_muted) {
        Ok(target_muted) => {
            let changed = current_muted != target_muted;
            if changed {
                play_hold_to_mute_sound(target_muted);
            }
            let show_overlay = STATE.lock().unwrap().hold_to_mute.show_overlay;
            let global_muted = current_mute_state().unwrap_or(target_muted);
            set_global_mute_state(global_muted, changed && show_overlay);
        }
        Err(err) => eprintln!("failed to restore hold hotkey mute state: {err:?}"),
    }
}

fn set_global_mute_state(muted: bool, trigger_overlay: bool) {
    let changed = {
        let mut state = STATE.lock().unwrap();
        let changed = state.muted != muted;
        state.muted = muted;
        changed
    };

    if !changed && !trigger_overlay {
        return;
    }

    refresh_tray_icon();
    let overlay = STATE.lock().unwrap().overlay.clone();
    if trigger_overlay && overlay.enabled && overlay.visibility == "AfterToggle" {
        let millis = (overlay.duration_secs.clamp(0.1, 10.0) * 1000.0) as u32;
        show_overlay_temporarily(millis);
    } else {
        apply_overlay_visibility();
    }
}

pub fn send_test_push_notification() {
    register_notification_integration();

    let mut toast = winrt_toast::Toast::new();
    apply_toast_logo(&mut toast);
    toast
        .text1("silence! push")
        .text2("System notification pipeline is alive.")
        .duration(winrt_toast::ToastDuration::Short)
        .launch("open-settings")
        .action(
            winrt_toast::Action::new("Toggle mute", "silence://toggle-mute", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        )
        .action(
            winrt_toast::Action::new("Settings", "silence://open-settings", "button")
                .with_activation_type(winrt_toast::content::action::ActivationType::Protocol),
        );

    let manager = winrt_toast::ToastManager::new(APP_USER_MODEL_ID);
    let activated = Some(Box::new(move |arguments: winrt_toast::Result<String>| {
        match arguments
            .ok()
            .and_then(|arguments| notification_action_from_text(&arguments))
        {
            Some(action) => handle_notification_action(action),
            None => open_settings_window(),
        }
    })
        as Box<dyn FnMut(winrt_toast::Result<String>) + Send + 'static>);

    if let Err(err) = manager.show_with_callbacks(&toast, activated, None, None) {
        eprintln!("failed to show push notification: {err:?}");
    }
}

pub async fn check_for_update() -> Result<updater::UpdateCheck> {
    updater::check_for_update().await
}

pub async fn download_and_install_update(
    update: updater::UpdateInfo,
    on_progress: impl FnMut(f32),
) -> Result<()> {
    let installer = updater::download_update(&update, on_progress).await?;
    write_last_update_notification(&update)?;
    updater::install_update(installer)?;
    request_exit_all_processes();
    std::process::exit(0);
}

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
            Ok(updater::UpdateCheck::Available(update))
                if updater::should_prompt_update(&update) =>
            {
                show_update_notification(&update);
            }
            Ok(_) => {}
            Err(err) => eprintln!("update check failed: {err:?}"),
        }
    });
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
    let (muted, overlay) = {
        let state = STATE.lock().unwrap();
        (state.muted, state.overlay.clone())
    };

    if native_overlay::is_positioning() {
        native_overlay::update(muted, &overlay);
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
        native_overlay::update(muted, &overlay);
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
    let volume = capture_volume(None)?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

pub fn mic_mute_state(device_id: Option<&str>) -> Result<bool> {
    let volume = capture_volume(device_id)?;
    let muted = unsafe { volume.GetMute()? };
    Ok(muted.as_bool())
}

fn target_mute_state(device_id: Option<&str>) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        return current_mute_state();
    }
    mic_mute_state(device_id.filter(|id| !id.is_empty()))
}

fn set_mute_to_inverse(device_id: Option<&str>) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        let next = !current_mute_state()?;
        set_all_capture_devices_mute(next)?;
        return Ok(next);
    }
    let volume = target_capture_volume(device_id)?;
    unsafe {
        let muted = volume.GetMute()?;
        let next = !muted.as_bool();
        volume.SetMute(next, null())?;
        Ok(next)
    }
}

fn set_mute(device_id: Option<&str>, muted: bool) -> Result<bool> {
    if is_all_microphones_target(device_id) {
        set_all_capture_devices_mute(muted)?;
        return Ok(muted);
    }
    let volume = target_capture_volume(device_id)?;
    unsafe {
        volume.SetMute(muted, null())?;
    }
    Ok(muted)
}

fn play_mute_sound(muted: bool) {
    let settings = STATE.lock().unwrap().sound_settings.clone();
    if !settings.enabled {
        return;
    }
    if let Err(err) = play_configured_sound(&settings, muted, settings.volume) {
        eprintln!("failed to play mute sound: {err:?}");
    }
}

fn play_hold_to_mute_sound(muted: bool) {
    let (sound_settings, hold_settings) = {
        let state = STATE.lock().unwrap();
        (state.sound_settings.clone(), state.hold_to_mute.clone())
    };
    if !sound_settings.enabled || !hold_settings.play_sounds {
        return;
    }

    let volume = hold_settings
        .volume_override
        .unwrap_or(sound_settings.volume)
        .min(100);
    let result = if muted {
        if let Some(theme) = hold_settings.mute_theme_override.as_deref() {
            play_theme_sound(theme, muted, volume)
        } else {
            play_configured_sound(&sound_settings, muted, volume)
        }
    } else if let Some(theme) = hold_settings.unmute_theme_override.as_deref() {
        play_theme_sound(theme, muted, volume)
    } else {
        play_configured_sound(&sound_settings, muted, volume)
    };

    if let Err(err) = result {
        eprintln!("failed to play hold-to-mute sound: {err:?}");
    }
}

fn play_auto_mute_sound() {
    let (sound_settings, auto_mute) = {
        let state = STATE.lock().unwrap();
        (state.sound_settings.clone(), state.auto_mute.clone())
    };
    if !sound_settings.enabled || !auto_mute.play_sounds {
        return;
    }

    if let Err(err) = play_configured_sound(&sound_settings, true, sound_settings.volume) {
        eprintln!("failed to play auto-mute sound: {err:?}");
    }
}

pub fn preview_sound(selection: &str, muted: bool, volume: u8) -> Result<u64> {
    let settings = STATE.lock().unwrap().sound_settings.clone();
    play_preview_sound_selection(selection, &settings, muted, volume)
}

pub fn stop_preview_sound() {
    if let Ok(mut audio) = AUDIO_ENGINE.lock()
        && let Some(engine) = audio.as_mut()
    {
        engine.stop_preview_sound();
    }
}

pub fn choose_custom_sounds() -> Result<Vec<CustomSound>> {
    let Some(sources) = rfd::FileDialog::new()
        .set_title("Add custom sound")
        .add_filter("Audio", &["mp3", "wav", "ogg", "flac"])
        .pick_files()
    else {
        return Ok(Vec::new());
    };

    sources
        .iter()
        .map(|source| import_custom_sound(source))
        .collect()
}

fn import_custom_sound(source: &Path) -> Result<CustomSound> {
    let extension = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .filter(|extension| matches!(extension.as_str(), "mp3" | "wav" | "ogg" | "flac"))
        .context("selected file is not a supported audio format")?;
    let original_file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("custom sound")
        .to_string();
    let id = default_custom_sound_id();
    let destination_dir = custom_sounds_dir()?;
    fs::create_dir_all(&destination_dir)?;
    let destination = destination_dir.join(format!("{id}.{extension}"));
    fs::copy(source, &destination).with_context(|| {
        format!(
            "copy custom sound from {} to {}",
            source.display(),
            destination.display()
        )
    })?;

    Ok(CustomSound {
        id,
        path: destination,
        original_file_name,
    })
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

pub fn sound_selection_label<'a>(selection: &str, settings: &'a SoundSettings) -> &'a str {
    custom_sound_from_selection(selection, settings)
        .map(|sound| sound.original_file_name.as_str())
        .or_else(|| {
            SOUND_THEMES
                .iter()
                .find(|theme| theme.id == selection)
                .map(|theme| theme.label)
        })
        .unwrap_or(SOUND_THEMES[0].label)
}

fn prime_sound_assets(settings: &SoundSettings) {
    if !settings.enabled {
        return;
    }

    for muted in [true, false] {
        if let Err(err) = load_configured_sound(settings, muted) {
            eprintln!("failed to preload sound asset: {err:?}");
        }
    }
}

fn load_configured_sound(settings: &SoundSettings, muted: bool) -> Result<SamplesBuffer> {
    load_sound_selection(sound_selection_for(settings, muted), settings, muted)
}

fn play_configured_sound(settings: &SoundSettings, muted: bool, volume: u8) -> Result<()> {
    play_sound_selection(
        sound_selection_for(settings, muted),
        settings,
        muted,
        volume,
    )
}

fn sound_selection_for(settings: &SoundSettings, muted: bool) -> &str {
    if muted {
        settings.mute_theme.as_str()
    } else {
        settings.unmute_theme.as_str()
    }
}

fn play_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
    volume: u8,
) -> Result<()> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_sound_selection(selection, settings, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    let engine = audio.as_mut().expect("audio engine initialized");
    engine.play_sound(sound, volume).map(|_| ())
}

fn play_preview_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
    volume: u8,
) -> Result<u64> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_sound_selection(selection, settings, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }
    let engine = audio.as_mut().expect("audio engine initialized");
    let duration = engine.play_preview_sound(sound, volume)?;
    Ok(duration.as_millis().max(1) as u64)
}

fn load_sound_selection(
    selection: &str,
    settings: &SoundSettings,
    muted: bool,
) -> Result<SamplesBuffer> {
    if let Some(custom_sound) = custom_sound_from_selection(selection, settings) {
        match load_custom_sound(custom_sound) {
            Ok(sound) => return Ok(sound),
            Err(err) => {
                eprintln!(
                    "failed to load custom {} sound, falling back to theme: {err:?}",
                    if muted { "mute" } else { "unmute" }
                );
            }
        }
    }

    load_theme_sound(theme_from_selection(selection), muted)
}

fn custom_sound_from_selection<'a>(
    selection: &str,
    settings: &'a SoundSettings,
) -> Option<&'a CustomSound> {
    let id = custom_sound_id(selection)?;
    settings.custom_sounds.iter().find(|sound| sound.id == id)
}

fn custom_sound_id(selection: &str) -> Option<&str> {
    selection.strip_prefix("custom:")
}

fn custom_sound_value(id: &str) -> String {
    format!("custom:{id}")
}

fn theme_from_selection(selection: &str) -> &str {
    if custom_sound_id(selection).is_some() {
        SOUND_THEMES[0].id
    } else {
        selection
    }
}

fn load_theme_sound(theme: &str, muted: bool) -> Result<SamplesBuffer> {
    let file = sound_file_name(theme, muted);
    if let Some(bytes) = sound_asset_bytes(&file) {
        return load_decoded_sound_bytes(file, bytes);
    }

    let path = sound_asset_path(&file).with_context(|| format!("find sound asset {file}"))?;
    load_decoded_sound(file, &path)
}

fn load_custom_sound(custom_sound: &CustomSound) -> Result<SamplesBuffer> {
    let cache_key = custom_sound_cache_key(&custom_sound.path)?;
    load_decoded_sound(cache_key, &custom_sound.path)
}

fn load_decoded_sound(cache_key: String, path: &Path) -> Result<SamplesBuffer> {
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }

    let engine = audio.as_mut().expect("audio engine initialized");
    engine.decoded_sound(&cache_key, path)
}

fn load_decoded_sound_bytes(cache_key: String, bytes: &'static [u8]) -> Result<SamplesBuffer> {
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    if audio.is_none() {
        *audio = Some(AudioEngine::new()?);
    }

    let engine = audio.as_mut().expect("audio engine initialized");
    engine.decoded_sound_bytes(&cache_key, bytes.to_vec())
}

fn play_theme_sound(theme: &str, muted: bool, volume: u8) -> Result<()> {
    let volume = f32::from(volume.min(100)) / 100.0;
    let sound = load_theme_sound(theme, muted)?;
    let mut audio = AUDIO_ENGINE.lock().unwrap();
    let engine = audio.as_mut().expect("audio engine initialized");
    engine.play_sound(sound, volume).map(|_| ())
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

fn sound_asset_bytes(file: &str) -> Option<&'static [u8]> {
    Some(match file {
        "8bit_mute.mp3" => include_bytes!("../assets/sounds/8bit_mute.mp3"),
        "8bit_unmute.mp3" => include_bytes!("../assets/sounds/8bit_unmute.mp3"),
        "blob_mute.mp3" => include_bytes!("../assets/sounds/blob_mute.mp3"),
        "blob_unmute.mp3" => include_bytes!("../assets/sounds/blob_unmute.mp3"),
        "digital_mute.mp3" => include_bytes!("../assets/sounds/digital_mute.mp3"),
        "digital_unmute.mp3" => include_bytes!("../assets/sounds/digital_unmute.mp3"),
        "discord_mute.mp3" => include_bytes!("../assets/sounds/discord_mute.mp3"),
        "discord_unmute.mp3" => include_bytes!("../assets/sounds/discord_unmute.mp3"),
        "pop_mute.mp3" => include_bytes!("../assets/sounds/pop_mute.mp3"),
        "pop_unmute.mp3" => include_bytes!("../assets/sounds/pop_unmute.mp3"),
        "punchy_mute.mp3" => include_bytes!("../assets/sounds/punchy_mute.mp3"),
        "punchy_unmute.mp3" => include_bytes!("../assets/sounds/punchy_unmute.mp3"),
        "scifi_mute.mp3" => include_bytes!("../assets/sounds/scifi_mute.mp3"),
        "scifi_unmute.mp3" => include_bytes!("../assets/sounds/scifi_unmute.mp3"),
        "vibrant_mute.mp3" => include_bytes!("../assets/sounds/vibrant_mute.mp3"),
        "vibrant_unmute.mp3" => include_bytes!("../assets/sounds/vibrant_unmute.mp3"),
        _ => return None,
    })
}

fn custom_sound_cache_key(path: &Path) -> Result<String> {
    let metadata = path
        .metadata()
        .with_context(|| format!("read custom sound metadata {}", path.display()))?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(format!(
        "custom:{}:{}:{modified}",
        path.display(),
        metadata.len()
    ))
}

fn custom_sounds_dir() -> Result<PathBuf> {
    Ok(app_config_dir()?.join("custom_sounds"))
}

fn is_all_microphones_target(device_id: Option<&str>) -> bool {
    matches!(device_id, Some(id) if id == HOTKEY_TARGET_ALL_MICROPHONES)
}

fn target_capture_volume(device_id: Option<&str>) -> Result<IAudioEndpointVolume> {
    capture_volume(device_id.filter(|id| !id.is_empty()))
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

fn active_capture_device_volumes() -> Result<Vec<IAudioEndpointVolume>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create audio device enumerator")?;
        let collection = enumerator
            .EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)
            .context("enumerate capture endpoints")?;
        let count = collection.GetCount().context("count capture endpoints")?;
        let mut volumes = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index).context("get capture endpoint")?;
            let volume = device
                .Activate(CLSCTX_ALL, None)
                .context("activate endpoint volume")?;
            volumes.push(volume);
        }

        Ok(volumes)
    }
}

fn set_all_capture_devices_mute(muted: bool) -> Result<()> {
    for volume in active_capture_device_volumes()? {
        unsafe {
            volume.SetMute(muted, null())?;
        }
    }
    Ok(())
}

pub fn capture_devices() -> Result<Vec<MicDevice>> {
    endpoint_devices(eCapture, "Microphone").map(|devices| {
        devices
            .into_iter()
            .map(|device| MicDevice {
                id: device.id,
                name: device.name,
                is_default: device.is_default,
            })
            .collect()
    })
}

pub fn render_devices() -> Result<Vec<AudioDevice>> {
    endpoint_devices(eRender, "Speaker")
}

fn endpoint_devices(flow: EDataFlow, fallback_name: &str) -> Result<Vec<AudioDevice>> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .context("create audio device enumerator")?;
        let default_id = endpoint_device_id(
            &enumerator
                .GetDefaultAudioEndpoint(flow, eConsole)
                .context("get default audio endpoint")?,
        )
        .ok();
        let collection = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .context("enumerate audio endpoints")?;
        let count = collection.GetCount().context("count audio endpoints")?;
        let mut devices = Vec::with_capacity(count as usize);

        for index in 0..count {
            let device = collection.Item(index).context("get audio endpoint")?;
            let id = endpoint_device_id(&device)?;
            let name = endpoint_device_name(&device).unwrap_or_else(|| fallback_name.to_string());
            let is_default = default_id.as_deref() == Some(id.as_str());
            devices.push(AudioDevice {
                id,
                name,
                is_default,
            });
        }

        Ok(devices)
    }
}

pub fn set_default_capture_device(device_id: &str) -> Result<()> {
    set_default_audio_device(device_id)
}

pub fn set_default_render_device(device_id: &str) -> Result<()> {
    set_default_audio_device(device_id)
}

fn set_default_audio_device(device_id: &str) -> Result<()> {
    let device_id = wide(device_id);
    unsafe {
        let policy: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG_CLIENT, None, CLSCTX_ALL)
            .context("create policy config client")?;
        for role in [eConsole, eMultimedia, eCommunications] {
            (Interface::vtable(&policy).SetDefaultEndpoint)(
                Interface::as_raw(&policy),
                PCWSTR(device_id.as_ptr()),
                role,
            )
            .ok()
            .context("set default audio endpoint")?;
        }
    }
    Ok(())
}

fn toggle_default_audio_device(
    flow: EDataFlow,
    target_1: Option<String>,
    target_2: Option<String>,
) -> Result<()> {
    let Some(target_1) = target_1.filter(|target| !target.is_empty()) else {
        return Ok(());
    };
    let Some(target_2) = target_2.filter(|target| !target.is_empty()) else {
        return Ok(());
    };

    let devices = endpoint_devices(flow, "Device")?;
    let next = if devices
        .iter()
        .any(|device| device.is_default && device.id == target_1)
    {
        target_2
    } else {
        target_1
    };

    set_default_audio_device(&next)
}

pub fn default_mic_label(devices: &[MicDevice]) -> String {
    devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone())
        .unwrap_or_else(|| "Default microphone".to_string())
}

unsafe fn endpoint_device_id(device: &IMMDevice) -> Result<String> {
    let id = unsafe { device.GetId()? };
    let text = unsafe { pwstr_to_string(id) };
    unsafe { CoTaskMemFree(Some(id.0 as *const c_void)) };
    Ok(text)
}

unsafe fn endpoint_device_name(device: &IMMDevice) -> Option<String> {
    let store = unsafe { device.OpenPropertyStore(STGM_READ).ok()? };
    let value = unsafe { store.GetValue(&PKEY_Device_FriendlyName).ok()? };
    let name = value.to_string();
    if name.is_empty() { None } else { Some(name) }
}

fn refresh_runtime_state() {
    reload_config_if_changed();
    evaluate_auto_mute_inactivity();
    refresh_mute_state();
}

fn refresh_mute_state() {
    let Ok(muted) = current_mute_state() else {
        return;
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
    config.auto_mute.after_inactivity_minutes =
        config.auto_mute.after_inactivity_minutes.clamp(1, 1440);
    Ok(config)
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
        binding.target_2 = None;
    } else {
        config.hotkeys.insert(
            0,
            HotkeyBinding {
                action: HotkeyAction::ToggleMute,
                shortcut: shortcut.clone(),
                gamepad: None,
                target: None,
                target_2: None,
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
        target_2: None,
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
    refresh_tray_icon();
    apply_overlay_visibility();
    prime_sound_assets(&config.sound_settings);
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

fn cleanup() {
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
