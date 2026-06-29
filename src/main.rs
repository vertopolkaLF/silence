#![windows_subsystem = "windows"]

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
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
        Devices::{
            Display::{
                DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER,
                DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME,
                DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS,
                QueryDisplayConfig,
            },
            FunctionDiscovery::{PKEY_Device_DeviceDesc, PKEY_Device_FriendlyName},
        },
        Foundation::{
            BOOL, CloseHandle, ERROR_ALREADY_EXISTS, ERROR_FILE_NOT_FOUND, ERROR_SUCCESS,
            GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
        },
        Graphics::{
            Dwm::{
                DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMWA_SYSTEMBACKDROP_TYPE,
                DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE, DwmSetWindowAttribute,
            },
            Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
                DEFAULT_CHARSET, DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors,
                EnumFontFamiliesExW, GetMonitorInfoW, HBITMAP, HDC, HMONITOR, LOGFONTW,
                MONITORINFO, SelectObject, TEXTMETRICW,
            },
        },
        Media::Audio::{
            AUDIO_VOLUME_NOTIFICATION_DATA, AudioSessionStateActive, DEVICE_STATE,
            DEVICE_STATE_ACTIVE, EDataFlow, ERole,
            Endpoints::{IAudioEndpointVolume, IAudioEndpointVolumeCallback},
            IAudioSessionControl, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
            IMMDeviceEnumerator, IMMNotificationClient, MMDeviceEnumerator, eCapture,
            eCommunications, eConsole, eMultimedia, eRender,
        },
        System::{
            Com::{
                CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, STGM_READ, STGM_READWRITE, StructuredStorage::PropVariantChangeType,
            },
            LibraryLoader::GetModuleHandleW,
            Registry::{
                HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
                RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
            },
            SystemInformation::GetTickCount,
            Threading::{
                CreateMutexW, OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
            Variant::VT_LPWSTR,
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
                ExtractIconExW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                AppendMenuW, CallNextHookEx, CallWindowProcW, CreateIcon, CreateIconFromResourceEx,
                CreatePopupMenu, CreateWindowExW, DI_NORMAL, DefWindowProcW, DestroyIcon,
                DestroyMenu, DestroyWindow, DispatchMessageW, DrawIconEx, EnumWindows, FindWindowW,
                GWL_WNDPROC, GetCursorPos, GetMessageW, GetSystemMetrics, GetWindowThreadProcessId,
                HHOOK, HICON, HMENU, IDC_ARROW, IDI_APPLICATION, IsIconic, IsWindowVisible,
                KBDLLHOOKSTRUCT, KillTimer, LR_DEFAULTSIZE, LoadCursorW, LoadIconW,
                MENU_ITEM_FLAGS, MSG, MSLLHOOKSTRUCT, PostMessageW, PostQuitMessage,
                RegisterClassW, RegisterWindowMessageW, SC_KEYMENU, SM_CXSCREEN, SM_CYSCREEN,
                SW_RESTORE, SW_SHOW, SendMessageW, SetForegroundWindow, SetMenuItemBitmaps,
                SetTimer, SetWindowLongPtrW, SetWindowsHookExW, ShowWindow, TPM_BOTTOMALIGN,
                TPM_LEFTALIGN, TPM_RETURNCMD, TrackPopupMenu, TranslateMessage,
                UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WM_APP,
                WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DISPLAYCHANGE, WM_DPICHANGED,
                WM_DWMCOMPOSITIONCHANGED, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
                WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_RBUTTONDOWN, WM_RBUTTONUP,
                WM_SETTINGCHANGE, WM_SYSCOMMAND, WM_THEMECHANGED, WM_TIMER, WM_WINDOWPOSCHANGED,
                WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSW, WNDPROC, WS_OVERLAPPED,
            },
        },
    },
    core::{GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, PROPVARIANT, PWSTR, w},
};

mod gui;
mod native_overlay;
pub(crate) mod overlay_icons;
pub mod updater;

include!("main_parts/constants.rs");
include!("main_parts/models.rs");
include!("main_parts/globals.rs");
include!("main_parts/app_boot.rs");
include!("main_parts/gamepad.rs");
include!("main_parts/tray.rs");
include!("main_parts/hotkeys_mute.rs");
include!("main_parts/updates_overlay.rs");
include!("main_parts/audio.rs");
include!("main_parts/runtime_config.rs");
include!("main_parts/input_utils.rs");
