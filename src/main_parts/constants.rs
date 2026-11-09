const WM_TRAY: u32 = WM_APP + 1;
const WM_TOGGLE_MUTE: u32 = WM_APP + 2;
const WM_OVERLAY_POSITIONING: u32 = WM_APP + 3;
const WM_MUTE: u32 = WM_APP + 4;
const WM_UNMUTE: u32 = WM_APP + 5;
const WM_OPEN_SETTINGS: u32 = WM_APP + 6;
const WM_UPDATE_NOW: u32 = WM_APP + 7;
const WM_EXIT_ALL: u32 = WM_APP + 8;
const WM_WHATS_NEW: u32 = WM_APP + 9;
const WM_AUDIO_MUTE_STATE_CHANGED: u32 = WM_APP + 10;
const WM_AUDIO_ENDPOINT_CHANGED: u32 = WM_APP + 11;
const WM_MUTE_FAILED: u32 = WM_APP + 12;
const WM_OVERLAY_ACTION: u32 = WM_APP + 13;
/// `config.json` was saved by another silence! process; reload it now.
const WM_CONFIG_CHANGED: u32 = WM_APP + 14;
/// Default console render endpoint volume or mute changed.
/// `wparam`: `(generation & 0x3FFF_FFFF) << 2`, bit 0 = muted, bit 1 = own change.
/// `lparam`: master volume as `f32` bits (`0.0..=1.0`).
const WM_OUTPUT_VOLUME_CHANGED: u32 = WM_APP + 15;
/// Default console render device changed, or the render endpoint must be rebound.
const WM_OUTPUT_DEVICE_CHANGED: u32 = WM_APP + 16;
/// Mouse messages for the output-volume icon. `lparam` is the raw mouse
/// message, same as `WM_TRAY` (the mic icon does not use `NOTIFYICON_VERSION_4`).
const WM_VOLUME_TRAY: u32 = WM_APP + 17;
const ID_TRAY: u32 = 1;
const ID_VOLUME_TRAY: u32 = 2;
const ID_STATE_TIMER: usize = 10;
const ID_OVERLAY_HIDE_TIMER: usize = 11;
const ID_OVERLAY_DRAG_TIMER: usize = 12;
const ID_TRAY_CLICK_TIMER: usize = 13;
const ID_TRAY_ADD_RETRY_TIMER: usize = 14;
const ID_MUTE_FAILURE_TIMER: usize = 15;
const ID_VOLUME_TRAY_CLICK_TIMER: usize = 16;
const ID_VOLUME_TRAY_ADD_RETRY_TIMER: usize = 17;
const ID_MENU_TOGGLE: usize = 1001;
const ID_MENU_SETTINGS: usize = 1002;
const ID_MENU_EXIT: usize = 1003;
const ID_MENU_TITLE: usize = 1004;
const ID_MENU_INSTALL_UPDATE: usize = 1005;
const ID_MENU_INPUT_DEVICE_BASE: usize = 2000;
const ID_MENU_OUTPUT_DEVICE_BASE: usize = 3000;
const ID_MENU_MIC_APP_BASE: usize = 4000;
const ID_MENU_VOLUME_TITLE: usize = 5100;
const ID_MENU_VOLUME_MUTE: usize = 5101;
const ID_MENU_VOLUME_MIXER: usize = 5102;
const ID_MENU_VOLUME_SOUND: usize = 5103;
const ID_MENU_VOLUME_SETTINGS: usize = 5104;
const ID_MENU_VOLUME_EXIT: usize = 5105;
const MENU_POS_DEFAULT_OUTPUT: u32 = 2;
const MENU_POS_DEFAULT_INPUT: u32 = 3;
const SETTINGS_WINDOW_TITLE: &str = "silence!";
const APP_USER_MODEL_ID: &str = "Silence.SilenceV2";
const APP_PROTOCOL: &str = "silence";
const ARG_POST_INSTALL: &str = "--post-install";
const MAIN_INSTANCE_MUTEX: PCWSTR = w!("SilenceV2BackgroundApp");
const DWMWA_MICA_EFFECT: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(1029);
const TRAY_DOUBLE_CLICK_DELAY_MS: u32 = 500;
const TRAY_ADD_RETRY_MS: u32 = 2_000;
const MIC_IN_USE_REFRESH_MS: u64 = 1_500;
const GILRS_ACTIVE_POLL_MS: u64 = 16;
const GAMEPAD_INACTIVE_POLL_MS: u64 = 1_000;
const XINPUT_CONNECTED_POLL_MS: u64 = 16;
const XINPUT_IDLE_POLL_MS: u64 = 1_000;
const ICON_RESOURCE_VERSION: u32 = 0x0003_0000;
const STARTUP_RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const STARTUP_RUN_VALUE: &str = "silence!";
const XINPUT_TRIGGER_PRESS_THRESHOLD: u8 = 160;
const XINPUT_TRIGGER_RELEASE_THRESHOLD: u8 = 96;
const MAX_GAMEPAD_COMBO_INPUTS: usize = 2;

pub(crate) const HOTKEY_TARGET_ALL_MICROPHONES: &str = "__all_microphones__";
pub(crate) const OVERLAY_DISPLAY_PRIMARY: &str = "Primary";
/// Volume OSD follows the monitor under the cursor.
pub(crate) const VOLUME_OSD_DISPLAY_CURSOR: &str = "cursor";
pub(crate) const AUDIO_DEVICE_NAME_PRETTY: &str = "PrettyName";
pub(crate) const AUDIO_DEVICE_NAME_SYSTEM: &str = "SystemName";
pub(crate) const AUDIO_DEVICE_NAME_BOTH: &str = "Both";
pub(crate) const INDICATOR_DEVICE_AUTO: &str = "Auto";
pub(crate) const INDICATOR_DEVICE_DEFAULT: &str = "Default";

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
