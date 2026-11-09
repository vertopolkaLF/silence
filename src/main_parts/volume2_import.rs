//! Read Volume² (irzyxa) install state and settings. This module does not write silence! config.
//!
//! Action indexes are `TAction.Tag` values from Volume² 26.1.0.471 (`FmMain`), not the order
//! of the hotkey dropdown. Checked against that build:
//! - 1 is the Volume² popup slider (`Standard regulator`)
//! - 5 is Volume Up, 7 is Open Volume Control
//! - next/previous default device are 29 and 57
//! - 35 is Set Default Audio Device, 39 is the audio device manager
//!
//! `ScrollObject` follows `CBScrollObject_Items` in `Languages\English.lng` for that build:
//! 0 icon in the notification area, 1 taskbar, 2 tray, 3 desktop, 4 window caption,
//! 5 anywhere on the screen, 6 OSD window.
//!
//! Hotkey names are the English tokens Volume² writes (`F15`, `Ctrl + Alt + Up`, `Win_Up`).
//! `stop_volume2` is the only function that terminates a process. Tests must not call it,
//! and they must not call `disable_volume2_autostart` either.

#![allow(dead_code)]

use std::{
    collections::{BTreeMap, HashSet},
    env, fs, io,
    mem::size_of,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::Command,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ACCESS_DENIED, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, HANDLE, WIN32_ERROR},
        Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation},
        System::{
            Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS},
            Registry::{
                HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY, RRF_RT_REG_DWORD, RRF_RT_REG_EXPAND_SZ, RRF_RT_REG_SZ,
                RegCloseKey, RegDeleteKeyValueW, RegEnumKeyExW, RegEnumValueW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
            },
            Threading::{OpenProcess, OpenProcessToken, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW, TerminateProcess},
        },
    },
    core::{PCWSTR, PWSTR},
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const BACKUP_KEY: &str = r"Software\silence!\Volume2AutostartBackup";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_KEY_WOW64: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume2Detection {
    pub installed_path: Option<PathBuf>,
    pub running: bool,
    /// OpenProcess failed, or the process token is elevated. Volume² started with
    /// `asInvoker`, so a failure here usually means it was launched elevated.
    pub running_elevated_or_unknown: bool,
    pub running_pids: Vec<u32>,
    /// Preferred autostart location: scheduled task, then HKLM Run, then HKCU Run, then a Startup file.
    pub autostart: Option<Volume2Autostart>,
    pub autostart_locations: Vec<Volume2Autostart>,
    pub config_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RegistryHive {
    CurrentUser,
    LocalMachine,
}

impl RegistryHive {
    fn hkey(self) -> HKEY {
        match self {
            Self::CurrentUser => HKEY_CURRENT_USER,
            Self::LocalMachine => HKEY_LOCAL_MACHINE,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::CurrentUser => "HKCU",
            Self::LocalMachine => "HKLM",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Volume2Autostart {
    RunKey {
        hive: RegistryHive,
        value_name: String,
        command: String,
    },
    ScheduledTask {
        task_name: String,
        command: String,
    },
    StartupFile {
        path: PathBuf,
    },
}

impl Volume2Autostart {
    fn rank(&self) -> u8 {
        match self {
            Self::ScheduledTask { .. } => 0,
            Self::RunKey { hive: RegistryHive::LocalMachine, .. } => 1,
            Self::RunKey { hive: RegistryHive::CurrentUser, .. } => 2,
            Self::StartupFile { .. } => 3,
        }
    }

    fn sort_key(&self) -> String {
        match self {
            Self::RunKey { hive, value_name, command } => format!("run:{}:{value_name}:{command}", hive.label()),
            Self::ScheduledTask { task_name, command } => format!("task:{task_name}:{command}"),
            Self::StartupFile { path } => format!("startup:{}", path.display()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisabledVolume2Autostart {
    /// The Run value was copied to `Software\silence!\Volume2AutostartBackup` and then deleted.
    MovedRunValue {
        hive: RegistryHive,
        value_name: String,
        backup_key: String,
        backup_value: String,
        command: String,
    },
    DisabledScheduledTask {
        task_name: String,
    },
    RenamedStartupFile {
        from: PathBuf,
        to: PathBuf,
    },
}

#[derive(Debug)]
pub enum Volume2ImportError {
    AdminRequired { message: String },
    NotFound { message: String },
    Failed { message: String },
}

impl std::fmt::Display for Volume2ImportError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AdminRequired { message } | Self::NotFound { message } | Self::Failed { message } => formatter.write_str(message),
        }
    }
}

impl std::error::Error for Volume2ImportError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportedWheelZone {
    /// `CBScrollObject_Items[0]`, "Icon in notification area".
    NotificationAreaIcon,
    /// `CBScrollObject_Items[1]`.
    Taskbar,
    /// `CBScrollObject_Items[2]`, "Tray".
    Tray,
    Desktop,
    WindowCaption,
    AnywhereOnScreen,
    OsdWindow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportedSkin {
    Rings,
    Digits,
    Speaker,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImportedOsdPosition {
    pub x_percent: f64,
    pub y_percent: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ImportedModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    pub right_ctrl: bool,
    pub right_alt: bool,
    pub right_shift: bool,
    pub right_win: bool,
}

/// One Volume² `TAction.Tag`. Percent variants stay `None` when the ini has no parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportedAction {
    None,
    VolumePopup,
    OpenAudioProperties,
    MuteToggle,
    OpenSettings,
    VolumeUp,
    VolumeDown,
    OpenMixer,
    SetVolumePercent(Option<u8>),
    EjectOptical,
    BrightnessUp,
    BrightnessDown,
    AltVolumeUp,
    AltVolumeDown,
    AltMuteToggle,
    BalanceLeft,
    BalanceRight,
    EjectUsb,
    BrightnessReset,
    BatteryPercent,
    NextDefaultDevice,
    VolumeChannelUp,
    VolumeChannelDown,
    NextChannel,
    SystemVolumePopup,
    SetDefaultAudioDevice,
    RunApplication,
    MuteAllExceptActiveApp,
    UnmuteAllApps,
    AudioDeviceManager,
    SetBrightnessPercent(Option<u8>),
    GotoBack,
    Reboot,
    PowerOff,
    TurnOffMonitor,
    BalanceCenter,
    StandBy,
    AppVolumeUp,
    AppVolumeDown,
    AppMuteToggle,
    ResetAppVolumes,
    MediaKeys,
    SetBalancePercent(Option<u8>),
    VolumeOn,
    VolumeOff,
    AltVolumeOn,
    AltVolumeOff,
    PreviousDefaultDevice,
    RunTask,
    TurnOnMonitor,
    MuteAllMicrophones,
    ToggleAudioEnhancements,
    ScreenEdgeToggle,
}

pub type ImportedClickAction = ImportedAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Volume2ActionInfo {
    pub index: u32,
    pub id: &'static str,
    pub caption: &'static str,
    pub action: ImportedAction,
}

const VOLUME2_ACTIONS: &[Volume2ActionInfo] = &[
    Volume2ActionInfo { index: 0, id: "None", caption: "No", action: ImportedAction::None },
    Volume2ActionInfo { index: 1, id: "Standard", caption: "Standard regulator", action: ImportedAction::VolumePopup },
    Volume2ActionInfo { index: 2, id: "SettingsAudio", caption: "Adjust Audio Properties", action: ImportedAction::OpenAudioProperties },
    Volume2ActionInfo { index: 3, id: "SoundOff", caption: "Mute On/Off", action: ImportedAction::MuteToggle },
    Volume2ActionInfo { index: 4, id: "Settings", caption: "Program options", action: ImportedAction::OpenSettings },
    Volume2ActionInfo { index: 5, id: "VolumeUp", caption: "Volume Up", action: ImportedAction::VolumeUp },
    Volume2ActionInfo { index: 6, id: "VolumeDown", caption: "Volume Down", action: ImportedAction::VolumeDown },
    Volume2ActionInfo { index: 7, id: "OpenVolumeControl", caption: "Open Volume Control", action: ImportedAction::OpenMixer },
    Volume2ActionInfo { index: 8, id: "Volume10", caption: "Volume 10%", action: ImportedAction::SetVolumePercent(Some(10)) },
    Volume2ActionInfo { index: 9, id: "Volume20", caption: "Volume 20%", action: ImportedAction::SetVolumePercent(Some(20)) },
    Volume2ActionInfo { index: 10, id: "Volume30", caption: "Volume 30%", action: ImportedAction::SetVolumePercent(Some(30)) },
    Volume2ActionInfo { index: 11, id: "Volume40", caption: "Volume 40%", action: ImportedAction::SetVolumePercent(Some(40)) },
    Volume2ActionInfo { index: 12, id: "Volume50", caption: "Volume 50%", action: ImportedAction::SetVolumePercent(Some(50)) },
    Volume2ActionInfo { index: 13, id: "Volume60", caption: "Volume 60%", action: ImportedAction::SetVolumePercent(Some(60)) },
    Volume2ActionInfo { index: 14, id: "Volume70", caption: "Volume 70%", action: ImportedAction::SetVolumePercent(Some(70)) },
    Volume2ActionInfo { index: 15, id: "Volume80", caption: "Volume 80%", action: ImportedAction::SetVolumePercent(Some(80)) },
    Volume2ActionInfo { index: 16, id: "Volume90", caption: "Volume 90%", action: ImportedAction::SetVolumePercent(Some(90)) },
    Volume2ActionInfo { index: 17, id: "Volume100", caption: "Volume 100%", action: ImportedAction::SetVolumePercent(Some(100)) },
    Volume2ActionInfo { index: 18, id: "EjectCDRom", caption: "Eject CD/DVD-ROM", action: ImportedAction::EjectOptical },
    Volume2ActionInfo { index: 19, id: "BrightnessUp", caption: "Brightness +", action: ImportedAction::BrightnessUp },
    Volume2ActionInfo { index: 20, id: "BrightnessDown", caption: "Brightness -", action: ImportedAction::BrightnessDown },
    Volume2ActionInfo { index: 21, id: "AltVolumeUp", caption: "Volume Up (alternate device)", action: ImportedAction::AltVolumeUp },
    Volume2ActionInfo { index: 22, id: "AltVolumeDown", caption: "Volume Down (alternate device)", action: ImportedAction::AltVolumeDown },
    Volume2ActionInfo { index: 23, id: "AltSoundOff", caption: "Mute On/Off (alternate device)", action: ImportedAction::AltMuteToggle },
    Volume2ActionInfo { index: 24, id: "BalanceLeft", caption: "Balance Left", action: ImportedAction::BalanceLeft },
    Volume2ActionInfo { index: 25, id: "BalanceRight", caption: "Balance Right", action: ImportedAction::BalanceRight },
    Volume2ActionInfo { index: 26, id: "SafelyRemoveDevice", caption: "Extract USB device", action: ImportedAction::EjectUsb },
    Volume2ActionInfo { index: 27, id: "BrightnessReset", caption: "Brightness *(Reset)", action: ImportedAction::BrightnessReset },
    Volume2ActionInfo { index: 28, id: "BatteryState", caption: "Battery life percent", action: ImportedAction::BatteryPercent },
    Volume2ActionInfo { index: 29, id: "SetNextDefaultDevice", caption: "Set Next Default Device", action: ImportedAction::NextDefaultDevice },
    Volume2ActionInfo { index: 30, id: "VolumeChannelUp", caption: "Volume channel +", action: ImportedAction::VolumeChannelUp },
    Volume2ActionInfo { index: 31, id: "VolumeChannelDown", caption: "Volume channel -", action: ImportedAction::VolumeChannelDown },
    Volume2ActionInfo { index: 32, id: "SetChannelActive", caption: "Set active next channel", action: ImportedAction::NextChannel },
    Volume2ActionInfo { index: 33, id: "ShowWindowsStandard", caption: "System regulator", action: ImportedAction::SystemVolumePopup },
    Volume2ActionInfo { index: 34, id: "SetVolumePercent", caption: "Set volume percent", action: ImportedAction::SetVolumePercent(None) },
    Volume2ActionInfo { index: 35, id: "SetDefaultAudioDevice", caption: "Set Default Audio Device", action: ImportedAction::SetDefaultAudioDevice },
    Volume2ActionInfo { index: 36, id: "RunApplication", caption: "Run Application", action: ImportedAction::RunApplication },
    Volume2ActionInfo { index: 37, id: "MuteAllExceptActiveApp", caption: "Mute All Except Active Application", action: ImportedAction::MuteAllExceptActiveApp },
    Volume2ActionInfo { index: 38, id: "UnMuteAllApps", caption: "UnMute All Applications", action: ImportedAction::UnmuteAllApps },
    Volume2ActionInfo { index: 39, id: "ShowSetDefaultDeviceWindow", caption: "Audio device manager", action: ImportedAction::AudioDeviceManager },
    Volume2ActionInfo { index: 40, id: "BrightnessPercent", caption: "Set brightness percent", action: ImportedAction::SetBrightnessPercent(None) },
    Volume2ActionInfo { index: 41, id: "GotoBack", caption: "Goto Back", action: ImportedAction::GotoBack },
    Volume2ActionInfo { index: 42, id: "RebootPC", caption: "Reboot PC", action: ImportedAction::Reboot },
    Volume2ActionInfo { index: 43, id: "ShutdownPC", caption: "Power Off", action: ImportedAction::PowerOff },
    Volume2ActionInfo { index: 44, id: "TurnOffMonitor", caption: "Turn Off Monitor", action: ImportedAction::TurnOffMonitor },
    Volume2ActionInfo { index: 45, id: "BalanceReset", caption: "Set Balance 50/50", action: ImportedAction::BalanceCenter },
    Volume2ActionInfo { index: 46, id: "StandByPC", caption: "Stand By", action: ImportedAction::StandBy },
    Volume2ActionInfo { index: 47, id: "ActiveAppVolumeUp", caption: "Volume Up (application)", action: ImportedAction::AppVolumeUp },
    Volume2ActionInfo { index: 48, id: "ActiveAppVolumeDown", caption: "Volume Down (application)", action: ImportedAction::AppVolumeDown },
    Volume2ActionInfo { index: 49, id: "ActiveAppMute", caption: "Mute On/Off (application)", action: ImportedAction::AppMuteToggle },
    Volume2ActionInfo { index: 50, id: "ResetAllAppsVolume", caption: "Reset All Apps Volume to Master", action: ImportedAction::ResetAppVolumes },
    Volume2ActionInfo { index: 51, id: "MediaKeys", caption: "Media Keys", action: ImportedAction::MediaKeys },
    Volume2ActionInfo { index: 52, id: "SetBalancePercent", caption: "Set Balance Percent", action: ImportedAction::SetBalancePercent(None) },
    Volume2ActionInfo { index: 53, id: "VolumeOn", caption: "Sound On", action: ImportedAction::VolumeOn },
    Volume2ActionInfo { index: 54, id: "VolumeOff", caption: "Sound Off", action: ImportedAction::VolumeOff },
    Volume2ActionInfo { index: 55, id: "AltVolumeOn", caption: "Sound On (alternate device)", action: ImportedAction::AltVolumeOn },
    Volume2ActionInfo { index: 56, id: "AltVolumeOff", caption: "Sound Off (alternate device)", action: ImportedAction::AltVolumeOff },
    Volume2ActionInfo { index: 57, id: "SetPrevDefaultDevice", caption: "Set Prev Default Device", action: ImportedAction::PreviousDefaultDevice },
    Volume2ActionInfo { index: 58, id: "ExecuteTask", caption: "Run Task", action: ImportedAction::RunTask },
    Volume2ActionInfo { index: 59, id: "TurnOnMonitor", caption: "Turn On Monitor", action: ImportedAction::TurnOnMonitor },
    Volume2ActionInfo { index: 60, id: "MuteAllMicrophones", caption: "Mute All Microphones", action: ImportedAction::MuteAllMicrophones },
    Volume2ActionInfo { index: 61, id: "AudioEnhancementsToggle", caption: "Audio Enhancements On/Off", action: ImportedAction::ToggleAudioEnhancements },
    Volume2ActionInfo { index: 62, id: "ScreenEdgeOnOff", caption: "Screen Edge On/Off", action: ImportedAction::ScreenEdgeToggle },
];

pub fn volume2_action_table() -> &'static [Volume2ActionInfo] {
    VOLUME2_ACTIONS
}

fn action_info(index: u32) -> Option<&'static Volume2ActionInfo> {
    VOLUME2_ACTIONS.iter().find(|info| info.index == index)
}

fn import_hotkey_action(action: ImportedAction) -> bool {
    !matches!(
        action,
        ImportedAction::None
            | ImportedAction::OpenSettings
            | ImportedAction::EjectOptical
            | ImportedAction::BrightnessUp
            | ImportedAction::BrightnessDown
            | ImportedAction::EjectUsb
            | ImportedAction::BrightnessReset
            | ImportedAction::BatteryPercent
            | ImportedAction::SetBrightnessPercent(_)
            | ImportedAction::GotoBack
            | ImportedAction::Reboot
            | ImportedAction::PowerOff
            | ImportedAction::TurnOffMonitor
            | ImportedAction::StandBy
            | ImportedAction::MediaKeys
            | ImportedAction::RunApplication
            | ImportedAction::RunTask
            | ImportedAction::TurnOnMonitor
            | ImportedAction::ScreenEdgeToggle
    )
}

fn apply_numeric_parameter(action: ImportedAction, parameter: Option<&str>) -> ImportedAction {
    let Some(parameter) = parameter.map(str::trim).filter(|value| !value.is_empty()) else {
        return action;
    };
    let Ok(number) = parameter.parse::<u8>() else {
        return action;
    };
    if number > 100 {
        return action;
    }
    match action {
        ImportedAction::SetVolumePercent(None) => ImportedAction::SetVolumePercent(Some(number)),
        ImportedAction::SetBalancePercent(None) => ImportedAction::SetBalancePercent(Some(number)),
        ImportedAction::SetBrightnessPercent(None) => ImportedAction::SetBrightnessPercent(Some(number)),
        other => other,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedHotkey {
    pub vk: u32,
    pub modifiers: ImportedModifiers,
    pub action: ImportedAction,
    pub volume2_index: u32,
    pub parameter: Option<String>,
    /// Key text from `[ShortKeys]`, before `_Parameter`.
    pub combo: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Volume2ImportedSettings {
    pub wheel_enabled: bool,
    pub wheel_zone: Option<ImportedWheelZone>,
    /// `DisableMouseScrollInFullScreen`.
    pub wheel_disabled_in_fullscreen: bool,
    /// `100 / VolSpeed`, integer division. `None` when VolSpeed is missing, 0, or above 100.
    pub step_percent: Option<u8>,
    pub left_click: Option<ImportedClickAction>,
    pub double_click: Option<ImportedClickAction>,
    pub middle_click: Option<ImportedClickAction>,
    /// `UseDisableSoundMiddleButton` in `[MouseActions]`. The middle-click action is kept as written.
    pub middle_button_mutes: bool,
    pub osd_enabled: bool,
    pub osd_duration_ms: Option<u32>,
    pub osd_position: Option<ImportedOsdPosition>,
    pub osd_monitor_index: Option<i32>,
    pub tray_skin: Option<ImportedSkin>,
    /// `[System] NotifyVolume`.
    pub notify_on_external_change: bool,
    pub hotkeys: Vec<ImportedHotkey>,
    pub unknown: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StopVolume2Result {
    pub terminated_pids: Vec<u32>,
    pub failed_pids: Vec<u32>,
    pub may_require_elevation: bool,
    pub not_running: bool,
    pub message: String,
}

pub fn detect_volume2() -> Volume2Detection {
    let processes = volume2_processes();
    let installed_path = find_installed_path(&processes);
    let config_path = find_config_path(installed_path.as_deref());
    let mut autostart_locations = Vec::new();
    autostart_locations.extend(scheduled_tasks());
    autostart_locations.extend(run_key_autostarts());
    autostart_locations.extend(startup_files());
    dedupe_tasks(&mut autostart_locations);
    autostart_locations.sort_by(|left, right| left.rank().cmp(&right.rank()).then_with(|| left.sort_key().cmp(&right.sort_key())));
    let running_elevated_or_unknown = processes.iter().any(|process| process.elevated_or_unknown);
    Volume2Detection {
        installed_path,
        running: !processes.is_empty(),
        running_elevated_or_unknown,
        running_pids: processes.iter().map(|process| process.pid).collect(),
        autostart: autostart_locations.first().cloned(),
        autostart_locations,
        config_path,
    }
}

pub fn parse_volume2_config(text: &str) -> Volume2ImportedSettings {
    let text = text.trim_start_matches('\u{feff}');
    let sections = parse_ini(text);
    let mut unknown = Vec::new();
    let main = section(&sections, "main");
    let mouse = section(&sections, "mouseactions");
    let osd = section(&sections, "osd");
    let skins = section(&sections, "skins");
    let system = section(&sections, "system");
    let short_keys = section(&sections, "shortkeys");

    let wheel_enabled = optional_bool(mouse, "UseMouseScroll", "UseMouseScroll", &mut unknown).unwrap_or(false);
    let wheel_zone = match optional_i32(mouse, "ScrollObject", "ScrollObject", &mut unknown) {
        Some(index) => match wheel_zone_from_index(index) {
            Some(zone) => Some(zone),
            None => {
                unknown.push(format!("unknown ScrollObject {index}"));
                None
            }
        },
        None => None,
    };
    let wheel_disabled_in_fullscreen = optional_bool(mouse, "DisableMouseScrollInFullScreen", "DisableMouseScrollInFullScreen", &mut unknown).unwrap_or(false);
    let step_percent = step_percent_from_vol_speed(main, &mut unknown);
    let left_click = click_action(mouse, "LeftMouseEvent", &mut unknown);
    let double_click = click_action(mouse, "DblLeftMouseEvent", &mut unknown);
    let middle_click = click_action(mouse, "MidleMouseEvent", &mut unknown);
    let middle_button_mutes = optional_bool(mouse, "UseDisableSoundMiddleButton", "UseDisableSoundMiddleButton", &mut unknown).unwrap_or(false);
    let osd_enabled = optional_bool(osd, "UseOSD", "UseOSD", &mut unknown).unwrap_or(false);
    let osd_duration_ms = optional_u32(osd, "OSDShowTime", "OSDShowTime", &mut unknown);
    let osd_position = osd_position(osd, &mut unknown);
    let osd_monitor_index = optional_i32(osd, "MonitorIndex", "MonitorIndex", &mut unknown);
    let tray_skin = skins.and_then(|section| section_value(section, "SkinName")).filter(|name| !name.is_empty()).map(tray_skin_from_name);
    let notify_on_external_change = optional_bool(system, "NotifyVolume", "NotifyVolume", &mut unknown).unwrap_or(false);
    let hotkeys = parse_short_keys(short_keys, &mut unknown);

    Volume2ImportedSettings {
        wheel_enabled,
        wheel_zone,
        wheel_disabled_in_fullscreen,
        step_percent,
        left_click,
        double_click,
        middle_click,
        middle_button_mutes,
        osd_enabled,
        osd_duration_ms,
        osd_position,
        osd_monitor_index,
        tray_skin,
        notify_on_external_change,
        hotkeys,
        unknown,
    }
}

pub fn read_volume2_config(path: impl AsRef<Path>) -> io::Result<Volume2ImportedSettings> {
    let bytes = fs::read(path)?;
    let text = decode_config_bytes(&bytes)?;
    Ok(parse_volume2_config(&text))
}

/// Disables one autostart location. Run keys are copied to
/// `HKCU` or `HKLM\Software\silence!\Volume2AutostartBackup` (same hive as the source)
/// and then deleted. Scheduled tasks are disabled with `schtasks /Change /DISABLE`.
/// Startup files are renamed with a `.disabled-by-silence` suffix.
///
/// Returns [`Volume2ImportError::AdminRequired`] without removing the autostart when the
/// write needs elevation. This function is not called by detection or by tests.
pub fn disable_volume2_autostart(autostart: &Volume2Autostart) -> Result<DisabledVolume2Autostart, Volume2ImportError> {
    match autostart {
        Volume2Autostart::RunKey { hive, value_name, command: _ } => disable_run_value(*hive, value_name),
        Volume2Autostart::ScheduledTask { task_name, command: _ } => disable_scheduled_task(task_name),
        Volume2Autostart::StartupFile { path } => disable_startup_file(path),
    }
}

/// Best-effort `TerminateProcess` for every `Volume2.exe`. If Volume² is elevated, this
/// reports that elevation is required and leaves the process running. Not called by tests.
pub fn stop_volume2() -> StopVolume2Result {
    let processes = volume2_processes();
    if processes.is_empty() {
        return StopVolume2Result {
            terminated_pids: Vec::new(),
            failed_pids: Vec::new(),
            may_require_elevation: false,
            not_running: true,
            message: "Volume² is not running.".to_string(),
        };
    }

    let mut terminated_pids = Vec::new();
    let mut failed_pids = Vec::new();
    let mut may_require_elevation = false;
    for process in &processes {
        match terminate_pid(process.pid) {
            Ok(()) => terminated_pids.push(process.pid),
            Err(TerminateFailure::AccessDenied) => {
                failed_pids.push(process.pid);
                may_require_elevation = true;
            }
            Err(TerminateFailure::Other) => {
                failed_pids.push(process.pid);
                may_require_elevation = true;
            }
        }
    }

    let message = if may_require_elevation {
        format!(
            "Stopping Volume² may require elevation. TerminateProcess failed for {failed_pids:?} and those processes are still running. Terminated {terminated_pids:?}."
        )
    } else {
        format!("Terminated Volume² process(es) {terminated_pids:?}.")
    };
    StopVolume2Result {
        terminated_pids,
        failed_pids,
        may_require_elevation,
        not_running: false,
        message,
    }
}

fn parse_ini(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() >= 2 {
            current = line[1..line.len() - 1].trim().to_ascii_lowercase();
            sections.entry(current.clone()).or_default();
            continue;
        }
        if current.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        sections.entry(current.clone()).or_default().insert(key.trim().to_string(), value.trim().to_string());
    }
    sections
}

fn section<'a>(sections: &'a BTreeMap<String, BTreeMap<String, String>>, name: &str) -> Option<&'a BTreeMap<String, String>> {
    sections.get(name)
}

fn section_value<'a>(section: &'a BTreeMap<String, String>, key: &str) -> Option<&'a str> {
    section.iter().find(|(name, _)| name.eq_ignore_ascii_case(key)).map(|(_, value)| value.as_str())
}

fn optional_bool(section: Option<&BTreeMap<String, String>>, key: &str, label: &str, unknown: &mut Vec<String>) -> Option<bool> {
    let raw = section.and_then(|section| section_value(section, key))?;
    match raw {
        "1" | "true" | "yes" => Some(true),
        "0" | "false" | "no" => Some(false),
        other => {
            unknown.push(format!("unknown {label} value \"{other}\""));
            None
        }
    }
}

fn optional_i32(section: Option<&BTreeMap<String, String>>, key: &str, label: &str, unknown: &mut Vec<String>) -> Option<i32> {
    let raw = section.and_then(|section| section_value(section, key))?;
    match raw.parse::<i32>() {
        Ok(value) => Some(value),
        Err(_) => {
            unknown.push(format!("unknown {label} value \"{raw}\""));
            None
        }
    }
}

fn optional_u32(section: Option<&BTreeMap<String, String>>, key: &str, label: &str, unknown: &mut Vec<String>) -> Option<u32> {
    let raw = section.and_then(|section| section_value(section, key))?;
    match raw.parse::<u32>() {
        Ok(value) => Some(value),
        Err(_) => {
            unknown.push(format!("unknown {label} value \"{raw}\""));
            None
        }
    }
}

fn wheel_zone_from_index(index: i32) -> Option<ImportedWheelZone> {
    Some(match index {
        0 => ImportedWheelZone::NotificationAreaIcon,
        1 => ImportedWheelZone::Taskbar,
        2 => ImportedWheelZone::Tray,
        3 => ImportedWheelZone::Desktop,
        4 => ImportedWheelZone::WindowCaption,
        5 => ImportedWheelZone::AnywhereOnScreen,
        6 => ImportedWheelZone::OsdWindow,
        _ => return None,
    })
}

fn step_percent_from_vol_speed(main: Option<&BTreeMap<String, String>>, unknown: &mut Vec<String>) -> Option<u8> {
    let raw = main.and_then(|section| section_value(section, "VolSpeed"))?;
    let Ok(speed) = raw.parse::<i64>() else {
        unknown.push(format!("unknown VolSpeed value \"{raw}\""));
        return None;
    };
    if !(1..=100).contains(&speed) {
        unknown.push(format!("VolSpeed {speed} is outside 1..=100, so there is no step percent"));
        return None;
    }
    let speed = speed as u32;
    if 100 % speed != 0 {
        unknown.push(format!("VolSpeed {speed} does not divide 100; step percent truncated to {}", 100 / speed));
    }
    Some((100 / speed) as u8)
}

fn click_action(section: Option<&BTreeMap<String, String>>, key: &str, unknown: &mut Vec<String>) -> Option<ImportedClickAction> {
    let raw = section.and_then(|section| section_value(section, key))?;
    let Ok(index) = raw.parse::<u32>() else {
        unknown.push(format!("unknown {key} value \"{raw}\""));
        return None;
    };
    match action_info(index) {
        Some(info) => Some(info.action),
        None => {
            unknown.push(format!("unknown {key} index {index}"));
            None
        }
    }
}

fn osd_position(section: Option<&BTreeMap<String, String>>, unknown: &mut Vec<String>) -> Option<ImportedOsdPosition> {
    let section = section?;
    let left = section_value(section, "OSDPositionLeft");
    let top = section_value(section, "OSDPositionTop");
    if left.is_none() && top.is_none() {
        return None;
    }
    let (Some(left), Some(top)) = (left, top) else {
        unknown.push("OSD position is missing OSDPositionLeft or OSDPositionTop".to_string());
        return None;
    };
    match (parse_percent(left), parse_percent(top)) {
        (Some(x_percent), Some(y_percent)) => Some(ImportedOsdPosition { x_percent, y_percent }),
        _ => {
            unknown.push(format!("unknown OSD position \"{left}\", \"{top}\""));
            None
        }
    }
}

fn parse_percent(raw: &str) -> Option<f64> {
    let normalized = raw.trim().replace(',', ".");
    if normalized.chars().filter(|ch| *ch == '.').count() > 1 {
        return None;
    }
    normalized.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn tray_skin_from_name(name: &str) -> ImportedSkin {
    let lower = name.to_ascii_lowercase();
    if lower.contains("ring") {
        ImportedSkin::Rings
    } else if lower.contains("digit") || lower.contains("default") {
        ImportedSkin::Digits
    } else {
        ImportedSkin::Speaker
    }
}

fn parse_short_keys(section: Option<&BTreeMap<String, String>>, unknown: &mut Vec<String>) -> Vec<ImportedHotkey> {
    let Some(section) = section else {
        return Vec::new();
    };
    let mut parameters = BTreeMap::<String, String>::new();
    let mut bindings = Vec::new();
    for (key, value) in section {
        if let Some(combo) = key.strip_suffix("_Parameter") {
            parameters.insert(combo.to_string(), value.clone());
            continue;
        }
        bindings.push((key.clone(), value.clone()));
    }

    let mut hotkeys = Vec::new();
    for (combo, raw_index) in bindings {
        let parameter = parameters.get(&combo).map(|value| value.trim()).filter(|value| !value.is_empty()).map(ToString::to_string);
        let Ok(index) = raw_index.parse::<u32>() else {
            unknown.push(format!("unknown hotkey \"{combo}\" action \"{raw_index}\""));
            continue;
        };
        let Some(info) = action_info(index) else {
            unknown.push(format!("unknown hotkey \"{combo}\" action index {index}"));
            continue;
        };
        let action = apply_numeric_parameter(info.action, parameter.as_deref());
        if !import_hotkey_action(action) {
            let parameter_note = parameter.as_ref().map(|value| format!(" parameter \"{value}\"")).unwrap_or_default();
            unknown.push(format!("ignored hotkey \"{combo}\" action {} ({index}){parameter_note}", info.caption));
            continue;
        }
        let parsed = match parse_hotkey_combo(&combo) {
            Ok(parsed) => parsed,
            Err(reason) => {
                unknown.push(format!("unknown hotkey \"{combo}\" ({reason})"));
                continue;
            }
        };
        if matches!(action, ImportedAction::SetVolumePercent(None) | ImportedAction::SetBalancePercent(None)) {
            if let Some(value) = &parameter {
                if value.parse::<u8>().ok().filter(|number| *number <= 100).is_none() {
                    unknown.push(format!("hotkey \"{combo}\" parameter \"{value}\" is not a percent from 0 to 100"));
                }
            }
        }
        hotkeys.push(ImportedHotkey {
            vk: parsed.vk,
            modifiers: parsed.modifiers,
            action,
            volume2_index: index,
            parameter,
            combo,
        });
    }
    hotkeys
}

struct ParsedCombo {
    modifiers: ImportedModifiers,
    vk: u32,
}

fn parse_hotkey_combo(raw: &str) -> Result<ParsedCombo, String> {
    let raw = raw.trim().trim_matches('"');
    if raw.is_empty() {
        return Err("empty key name".to_string());
    }
    let parts = split_combo(raw);
    if parts.is_empty() {
        return Err("empty key name".to_string());
    }
    if parts.len() == 1 {
        return match virtual_key(&parts[0]) {
            Some(vk) => Ok(ParsedCombo { modifiers: ImportedModifiers::default(), vk }),
            None => Err(format!("no virtual key for \"{}\"", parts[0])),
        };
    }
    let mut modifiers = ImportedModifiers::default();
    for part in &parts[..parts.len() - 1] {
        if !apply_modifier(&mut modifiers, part) {
            return Err(format!("\"{raw}\" is a chord without a single virtual key"));
        }
    }
    let key = parts.last().map(String::as_str).unwrap_or("");
    match virtual_key(key) {
        Some(vk) => Ok(ParsedCombo { modifiers, vk }),
        None => Err(format!("no virtual key for \"{key}\"")),
    }
}

fn split_combo(raw: &str) -> Vec<String> {
    if virtual_key(raw).is_some() {
        return vec![raw.to_string()];
    }
    let pieces: Vec<String> = if raw.contains(" + ") {
        raw.split(" + ").map(str::trim).filter(|part| !part.is_empty()).map(str::to_string).collect()
    } else if raw.contains('+') {
        raw.split('+').map(str::trim).filter(|part| !part.is_empty()).map(str::to_string).collect()
    } else if raw.contains('_') {
        raw.split('_').map(str::trim).filter(|part| !part.is_empty()).map(str::to_string).collect()
    } else {
        vec![raw.to_string()]
    };
    pieces
}

fn apply_modifier(modifiers: &mut ImportedModifiers, token: &str) -> bool {
    match token.trim().to_ascii_lowercase().as_str() {
        "ctrl" | "lctrl" | "control" | "lcontrol" => {
            modifiers.ctrl = true;
            true
        }
        "rctrl" | "rcontrol" => {
            modifiers.ctrl = true;
            modifiers.right_ctrl = true;
            true
        }
        "alt" | "lalt" => {
            modifiers.alt = true;
            true
        }
        "ralt" => {
            modifiers.alt = true;
            modifiers.right_alt = true;
            true
        }
        "shift" | "lshift" => {
            modifiers.shift = true;
            true
        }
        "rshift" => {
            modifiers.shift = true;
            modifiers.right_shift = true;
            true
        }
        "win" | "lwin" | "windows" => {
            modifiers.win = true;
            true
        }
        "rwin" => {
            modifiers.win = true;
            modifiers.right_win = true;
            true
        }
        _ => false,
    }
}

fn virtual_key(token: &str) -> Option<u32> {
    let token = token.trim();
    if token.chars().count() == 1 {
        let ch = token.chars().next()?;
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_uppercase() as u32);
        }
        if ch.is_ascii_digit() {
            return Some(u32::from(ch as u8));
        }
        return oem_virtual_key(ch);
    }

    let lower = token.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(number) = rest.parse::<u32>() {
            if (1..=24).contains(&number) {
                return Some(0x70 + number - 1);
            }
        }
    }
    if let Some(rest) = lower.strip_prefix("num") {
        if rest.len() == 1 {
            let byte = rest.as_bytes()[0];
            if byte.is_ascii_digit() {
                return Some(0x60 + u32::from(byte - b'0'));
            }
            return match rest {
                "*" => Some(0x6A),
                "+" => Some(0x6B),
                "-" => Some(0x6D),
                "." => Some(0x6E),
                "/" => Some(0x6F),
                _ => None,
            };
        }
    }
    Some(match lower.as_str() {
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "space" | "spacebar" => 0x20,
        "tab" => 0x09,
        "esc" | "escape" => 0x1B,
        "enter" | "return" => 0x0D,
        "back" | "backspace" => 0x08,
        "del" | "delete" => 0x2E,
        "ins" | "insert" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pgup" | "pageup" | "page up" => 0x21,
        "pgdn" | "pagedown" | "page down" => 0x22,
        "pause" => 0x13,
        "capslock" => 0x14,
        "numlock" => 0x90,
        "scrolllock" => 0x91,
        "printscreen" | "prtsc" | "snapshot" => 0x2C,
        "apps" => 0x5D,
        "volumemute" => 0xAD,
        "volumedown" => 0xAE,
        "volumeup" => 0xAF,
        "medianexttrack" => 0xB0,
        "mediaprevtrack" => 0xB1,
        "mediastop" => 0xB2,
        "mediaplaypause" => 0xB3,
        "browserback" => 0xA6,
        "browserforward" => 0xA7,
        "browserrefresh" => 0xA8,
        "browserstop" => 0xA9,
        "browsersearch" => 0xAA,
        "browserfavorites" => 0xAB,
        "browserhome" => 0xAC,
        "launchmail" => 0xB4,
        "launchmediaselect" => 0xB5,
        "launchmycomputer" => 0xB6,
        "launchcalc" => 0xB7,
        "mouseleft" => 0x01,
        "mouseright" => 0x02,
        "mousemiddle" => 0x04,
        "mousex1" => 0x05,
        "mousex2" => 0x06,
        _ => return None,
    })
}

fn oem_virtual_key(ch: char) -> Option<u32> {
    Some(match ch {
        ';' => 0xBA,
        '=' => 0xBB,
        ',' => 0xBC,
        '-' => 0xBD,
        '.' => 0xBE,
        '/' => 0xBF,
        '`' => 0xC0,
        '[' => 0xDB,
        '\\' => 0xDC,
        ']' => 0xDD,
        '\'' => 0xDE,
        _ => return None,
    })
}

fn decode_config_bytes(bytes: &[u8]) -> io::Result<String> {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_le(&bytes[2..]);
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16_be(&bytes[2..]);
    }
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return String::from_utf8(bytes[3..].to_vec()).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error));
    }
    let nul_count = bytes.iter().filter(|byte| **byte == 0).count();
    if bytes.len() >= 4 && bytes.len() % 2 == 0 && nul_count > bytes.len() / 4 {
        return decode_utf16_le(bytes);
    }
    String::from_utf8(bytes.to_vec()).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn decode_utf16_le(bytes: &[u8]) -> io::Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "UTF-16 config has an odd length"));
    }
    let units = bytes.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect::<Vec<_>>();
    Ok(String::from_utf16_lossy(&units))
}

fn decode_utf16_be(bytes: &[u8]) -> io::Result<String> {
    if bytes.len() % 2 != 0 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "UTF-16 config has an odd length"));
    }
    let units = bytes.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect::<Vec<_>>();
    Ok(String::from_utf16_lossy(&units))
}

struct Volume2Process {
    pid: u32,
    image_path: Option<PathBuf>,
    elevated_or_unknown: bool,
}

fn volume2_processes() -> Vec<Volume2Process> {
    let mut processes = Vec::new();
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
            return processes;
        };
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut found = Process32FirstW(snapshot, &mut entry).is_ok();
        while found {
            if process_name(&entry).eq_ignore_ascii_case("Volume2.exe") {
                let pid = entry.th32ProcessID;
                let (image_path, elevated_or_unknown) = inspect_process(pid);
                processes.push(Volume2Process { pid, image_path, elevated_or_unknown });
            }
            found = Process32NextW(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
    processes
}

fn process_name(entry: &PROCESSENTRY32W) -> String {
    let len = entry.szExeFile.iter().position(|unit| *unit == 0).unwrap_or(entry.szExeFile.len());
    String::from_utf16_lossy(&entry.szExeFile[..len])
}

fn inspect_process(pid: u32) -> (Option<PathBuf>, bool) {
    let handle = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(handle) => handle,
        Err(_) => return (None, true),
    };
    let image_path = query_process_path(handle);
    let elevated_or_unknown = match process_is_elevated(handle) {
        Some(elevated) => elevated,
        None => true,
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    (image_path, elevated_or_unknown)
}

fn query_process_path(handle: HANDLE) -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32_768];
    let mut len = buffer.len() as u32;
    let result = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_FORMAT(0), PWSTR(buffer.as_mut_ptr()), &mut len) };
    result.ok()?;
    buffer.truncate(len as usize);
    Some(PathBuf::from(String::from_utf16_lossy(&buffer)))
}

fn process_is_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;
        let result = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut core::ffi::c_void),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );
        let _ = CloseHandle(token);
        result.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

enum TerminateFailure {
    AccessDenied,
    Other,
}

fn terminate_pid(pid: u32) -> Result<(), TerminateFailure> {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, false, pid) }.map_err(|error| {
        if is_access_denied(&error) {
            TerminateFailure::AccessDenied
        } else {
            TerminateFailure::Other
        }
    })?;
    let result = unsafe { TerminateProcess(handle, 0) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    result.map_err(|error| {
        if is_access_denied(&error) {
            TerminateFailure::AccessDenied
        } else {
            TerminateFailure::Other
        }
    })
}

fn is_access_denied(error: &windows_core::Error) -> bool {
    let code = error.code().0 as u32;
    code == ERROR_ACCESS_DENIED.0 || code & 0xFFFF == ERROR_ACCESS_DENIED.0
}

fn find_installed_path(processes: &[Volume2Process]) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    candidates.extend(uninstall_install_paths());
    if let Some(directory) = reg_get_sz(HKEY_LOCAL_MACHINE, r"Software\Volume2", "Directory").found() {
        candidates.push(PathBuf::from(directory));
    }
    if let Some(program_files) = env::var_os("ProgramW6432").or_else(|| env::var_os("ProgramFiles")) {
        candidates.push(PathBuf::from(program_files).join("Volume2"));
    }
    if let Some(program_files_x86) = env::var_os("ProgramFiles(x86)") {
        candidates.push(PathBuf::from(program_files_x86).join("Volume2"));
    }
    candidates.push(PathBuf::from(r"C:\Program Files\Volume2"));
    candidates.push(PathBuf::from(r"C:\Program Files (x86)\Volume2"));
    for process in processes {
        if let Some(path) = &process.image_path {
            candidates.push(path.clone());
        }
    }
    candidates.into_iter().filter_map(|candidate| volume2_exe(&candidate)).find(|path| path.is_file())
}

fn volume2_exe(candidate: &Path) -> Option<PathBuf> {
    if candidate.is_file() && candidate.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.eq_ignore_ascii_case("Volume2.exe")) {
        return Some(candidate.to_path_buf());
    }
    let exe = if candidate.is_dir() {
        candidate.join("Volume2.exe")
    } else {
        candidate.parent()?.join("Volume2.exe")
    };
    Some(exe)
}

fn uninstall_install_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (hive, subkey) in [
        (HKEY_LOCAL_MACHINE, UNINSTALL_KEY),
        (HKEY_LOCAL_MACHINE, UNINSTALL_KEY_WOW64),
        (HKEY_CURRENT_USER, UNINSTALL_KEY),
    ] {
        let Some(key) = open_key(hive, subkey, KEY_READ | KEY_WOW64_64KEY) else {
            continue;
        };
        for name in enum_subkeys(key.0) {
            let child = format!("{subkey}\\{name}");
            let display_name = reg_get_sz(hive, &child, "DisplayName").found().unwrap_or_default();
            let install_location = reg_get_sz(hive, &child, "InstallLocation").found().unwrap_or_default();
            let display_icon = reg_get_sz(hive, &child, "DisplayIcon").found().unwrap_or_default();
            let uninstall_string = reg_get_sz(hive, &child, "UninstallString").found().unwrap_or_default();
            if !is_volume2_uninstall(&display_name, &install_location, &display_icon, &uninstall_string) {
                continue;
            }
            if !install_location.is_empty() {
                paths.push(PathBuf::from(install_location));
            }
            if let Some(icon_path) = display_icon.split(',').next() {
                let icon_path = icon_path.trim().trim_matches('"');
                if !icon_path.is_empty() {
                    paths.push(PathBuf::from(icon_path));
                }
            }
            let uninstall_path = uninstall_string.trim().trim_matches('"');
            if let Some(parent) = Path::new(uninstall_path).parent() {
                paths.push(parent.to_path_buf());
            }
        }
    }
    paths
}

fn is_volume2_uninstall(display_name: &str, install_location: &str, display_icon: &str, uninstall_string: &str) -> bool {
    let name = display_name.to_ascii_lowercase();
    let name_is_volume2 = name.contains("volume2") || name.contains("volume²");
    let evidence = [install_location, display_icon, uninstall_string].join(" ").to_ascii_lowercase();
    let evidence_is_volume2 = evidence.contains("volume2.exe") || evidence.contains("\\volume2\\") || evidence.contains("\\volume2");
    name_is_volume2 || (name.contains("volume") && evidence_is_volume2)
}

fn find_config_path(installed_exe: Option<&Path>) -> Option<PathBuf> {
    let appdata = env::var_os("APPDATA").map(|dir| PathBuf::from(dir).join("Volume2").join("Config.ini"));
    let portable = installed_exe.and_then(|exe| exe.parent().map(|dir| dir.join("Config.ini")));
    let prefer_appdata = reg_get_dword(HKEY_LOCAL_MACHINE, r"Software\Volume2", "SaveOptions") != Some(0);
    let ordered: [Option<PathBuf>; 2] = if prefer_appdata { [appdata, portable] } else { [portable, appdata] };
    ordered.into_iter().flatten().find(|path| path.is_file())
}

fn scheduled_tasks() -> Vec<Volume2Autostart> {
    let mut tasks = scheduled_tasks_from_schtasks_output(&schtasks_list_output());
    tasks.extend(scheduled_tasks_on_disk());
    tasks
}

fn schtasks_list_output() -> String {
    let Ok(output) = Command::new("schtasks").args(["/query", "/fo", "LIST", "/v"]).creation_flags(CREATE_NO_WINDOW).output() else {
        return String::new();
    };
    decode_command_output(&output.stdout)
}

fn scheduled_tasks_from_schtasks_output(text: &str) -> Vec<Volume2Autostart> {
    let mut tasks = Vec::new();
    let mut task_name: Option<String> = None;
    let mut command: Option<String> = None;
    let flush = |task_name: &mut Option<String>, command: &mut Option<String>, tasks: &mut Vec<Volume2Autostart>| {
        if let (Some(name), Some(cmd)) = (task_name.take(), command.take()) {
            if command_is_volume2(&cmd) {
                tasks.push(Volume2Autostart::ScheduledTask { task_name: name, command: cmd });
            }
        } else {
            task_name.take();
            command.take();
        }
    };
    for line in text.lines() {
        if line.trim().is_empty() {
            flush(&mut task_name, &mut command, &mut tasks);
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase().replace(' ', "");
        let value = value.trim();
        if key == "taskname" {
            if task_name.is_some() {
                flush(&mut task_name, &mut command, &mut tasks);
            }
            task_name = Some(value.to_string());
        } else if key == "tasktorun" {
            command = Some(value.to_string());
        }
    }
    flush(&mut task_name, &mut command, &mut tasks);
    tasks
}

fn scheduled_tasks_on_disk() -> Vec<Volume2Autostart> {
    let root = PathBuf::from(r"C:\Windows\System32\Tasks");
    let mut tasks = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(bytes) = fs::read(&path) else {
                continue;
            };
            if bytes.len() > 1_048_576 || !bytes_contain_volume2_exe(&bytes) {
                continue;
            }
            let text = decode_command_output(&bytes);
            let command_node = xml_text(&text, "Command").unwrap_or_else(|| "Volume2.exe".to_string());
            let arguments = xml_text(&text, "Arguments").unwrap_or_default();
            let command = if arguments.is_empty() { command_node } else { format!("{command_node} {arguments}") };
            tasks.push(Volume2Autostart::ScheduledTask {
                task_name: task_name_from_path(&root, &path),
                command,
            });
        }
    }
    tasks
}

fn bytes_contain_volume2_exe(bytes: &[u8]) -> bool {
    let lower = bytes.iter().map(|byte| byte.to_ascii_lowercase()).collect::<Vec<_>>();
    let utf8 = b"volume2.exe";
    if lower.windows(utf8.len()).any(|window| window == utf8) {
        return true;
    }
    let utf16 = "volume2.exe".encode_utf16().flat_map(|unit| unit.to_le_bytes()).collect::<Vec<_>>();
    lower.windows(utf16.len()).any(|window| window == utf16)
}

fn xml_text(text: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let index = text.find(&start)?;
    let rest = &text[index + start.len()..];
    let end_index = rest.find(&end)?;
    Some(rest[..end_index].trim().to_string())
}

fn task_name_from_path(root: &Path, file: &Path) -> String {
    let relative = file.strip_prefix(root).unwrap_or(file);
    let mut name = String::from("\\");
    let parts = relative.components().map(|component| component.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>();
    name.push_str(&parts.join("\\"));
    name
}

fn dedupe_tasks(locations: &mut Vec<Volume2Autostart>) {
    let mut seen = HashSet::new();
    locations.retain(|location| match location {
        Volume2Autostart::ScheduledTask { task_name, .. } => seen.insert(task_name.trim_start_matches('\\').to_ascii_lowercase()),
        _ => true,
    });
}

fn command_is_volume2(command: &str) -> bool {
    command.to_ascii_lowercase().contains("volume2.exe")
}

fn run_key_autostarts() -> Vec<Volume2Autostart> {
    let mut found = Vec::new();
    for hive in [RegistryHive::LocalMachine, RegistryHive::CurrentUser] {
        let Some(key) = open_key(hive.hkey(), RUN_KEY, KEY_READ | KEY_WOW64_64KEY) else {
            continue;
        };
        for value_name in enum_value_names(key.0) {
            let Some(command) = reg_get_sz(hive.hkey(), RUN_KEY, &value_name).found() else {
                continue;
            };
            if command_is_volume2(&command) {
                found.push(Volume2Autostart::RunKey { hive, value_name, command });
            }
        }
    }
    found
}

fn startup_files() -> Vec<Volume2Autostart> {
    let mut files = Vec::new();
    for dir in startup_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_volume2_startup_file(&path) {
                files.push(Volume2Autostart::StartupFile { path });
            }
        }
    }
    files
}

fn startup_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(appdata) = env::var_os("APPDATA") {
        dirs.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup"));
    }
    if let Some(program_data) = env::var_os("ProgramData") {
        dirs.push(PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs\Startup"));
    }
    dirs
}

fn is_volume2_startup_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("volume2pm") {
        return false;
    }
    lower.starts_with("volume2") && (lower.ends_with(".lnk") || lower.ends_with(".exe"))
}

fn disable_run_value(hive: RegistryHive, value_name: &str) -> Result<DisabledVolume2Autostart, Volume2ImportError> {
    let command = match reg_get_sz(hive.hkey(), RUN_KEY, value_name) {
        RegRead::Found(command) => command,
        RegRead::Missing => {
            return Err(Volume2ImportError::NotFound {
                message: format!("Volume² autostart value \"{value_name}\" is not in {}\\{RUN_KEY}. Nothing was changed.", hive.label()),
            });
        }
        RegRead::Error(status) if status == ERROR_ACCESS_DENIED => {
            return Err(admin_required_run(hive, value_name, "reading it"));
        }
        RegRead::Error(status) => {
            return Err(Volume2ImportError::Failed {
                message: format!("Could not read {}\\{RUN_KEY}\\{value_name} (status {}). Nothing was changed.", hive.label(), status.0),
            });
        }
    };

    let backup_value = format!("{}_{value_name}", hive.label());
    match reg_get_sz(hive.hkey(), BACKUP_KEY, &backup_value) {
        RegRead::Found(existing) if existing != command => {
            return Err(Volume2ImportError::Failed {
                message: format!(
                    "Backup value {backup_value} in {}\\{BACKUP_KEY} already holds a different command. Volume² autostart was left unchanged.",
                    hive.label()
                ),
            });
        }
        RegRead::Found(_) | RegRead::Missing => {}
        RegRead::Error(status) if status == ERROR_ACCESS_DENIED => return Err(admin_required_run(hive, value_name, "reading the backup key")),
        RegRead::Error(status) => {
            return Err(Volume2ImportError::Failed {
                message: format!("Could not read the Volume² autostart backup (status {}). Nothing was changed.", status.0),
            });
        }
    }

    let source = format!("{}\\{RUN_KEY}", hive.label());
    if let Err(error) = write_backup(hive, &backup_value, &command, &source) {
        return Err(error);
    }
    let status = reg_delete_value(hive.hkey(), RUN_KEY, value_name);
    if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
        return Ok(DisabledVolume2Autostart::MovedRunValue {
            hive,
            value_name: value_name.to_string(),
            backup_key: BACKUP_KEY.to_string(),
            backup_value,
            command,
        });
    }
    if status == ERROR_ACCESS_DENIED {
        return Err(Volume2ImportError::AdminRequired {
            message: format!(
                "Copied Volume² autostart to {}\\{BACKUP_KEY}\\{backup_value}, but deleting {}\\{RUN_KEY}\\{value_name} needs administrator rights. The Run value is still there, so Volume² will still start.",
                hive.label(),
                hive.label()
            ),
        });
    }
    Err(Volume2ImportError::Failed {
        message: format!(
            "Copied Volume² autostart to the backup key, but deleting the Run value failed (status {}). Volume² will still start.",
            status.0
        ),
    })
}

fn write_backup(hive: RegistryHive, backup_value: &str, command: &str, source: &str) -> Result<(), Volume2ImportError> {
    let command_status = reg_set_sz(hive.hkey(), BACKUP_KEY, backup_value, command);
    if command_status != ERROR_SUCCESS {
        return Err(backup_write_error(hive, command_status));
    }
    let source_value = format!("{backup_value}_Source");
    let source_status = reg_set_sz(hive.hkey(), BACKUP_KEY, &source_value, source);
    if source_status != ERROR_SUCCESS {
        return Err(backup_write_error(hive, source_status));
    }
    Ok(())
}

fn backup_write_error(hive: RegistryHive, status: WIN32_ERROR) -> Volume2ImportError {
    if status == ERROR_ACCESS_DENIED {
        Volume2ImportError::AdminRequired {
            message: format!(
                "Disabling the Volume² Run value needs administrator rights to write {}\\{BACKUP_KEY}. Nothing was removed.",
                hive.label()
            ),
        }
    } else {
        Volume2ImportError::Failed {
            message: format!("Could not write the Volume² autostart backup (status {}). Nothing was removed.", status.0),
        }
    }
}

fn admin_required_run(hive: RegistryHive, value_name: &str, action: &str) -> Volume2ImportError {
    Volume2ImportError::AdminRequired {
        message: format!(
            "Volume² autostart is {}\\{RUN_KEY}\\{value_name}. {action} needs administrator rights. Nothing was changed.",
            hive.label()
        ),
    }
}

fn disable_scheduled_task(task_name: &str) -> Result<DisabledVolume2Autostart, Volume2ImportError> {
    let output = Command::new("schtasks")
        .args(["/Change", "/TN", task_name, "/DISABLE"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| Volume2ImportError::Failed {
            message: format!("Could not run schtasks to disable \"{task_name}\": {error}. The task was not changed."),
        })?;
    if output.status.success() {
        return Ok(DisabledVolume2Autostart::DisabledScheduledTask { task_name: task_name.to_string() });
    }
    let detail = format!("{}{}", decode_command_output(&output.stdout), decode_command_output(&output.stderr));
    let lower = detail.to_ascii_lowercase();
    if lower.contains("denied") || lower.contains("отказ") || lower.contains("0x80070005") {
        return Err(Volume2ImportError::AdminRequired {
            message: format!("Disabling scheduled task \"{task_name}\" needs administrator rights. The task was not changed. {detail}"),
        });
    }
    Err(Volume2ImportError::Failed {
        message: format!("schtasks could not disable \"{task_name}\". The task was not changed. {detail}"),
    })
}

fn disable_startup_file(path: &Path) -> Result<DisabledVolume2Autostart, Volume2ImportError> {
    if !path.exists() {
        return Err(Volume2ImportError::NotFound {
            message: format!("Startup file {} is already gone. Nothing was changed.", path.display()),
        });
    }
    let mut destination = path.as_os_str().to_os_string();
    destination.push(".disabled-by-silence");
    let destination = PathBuf::from(destination);
    if destination.exists() {
        return Err(Volume2ImportError::Failed {
            message: format!("Refusing to rename {} because {} already exists. Nothing was changed.", path.display(), destination.display()),
        });
    }
    match fs::rename(path, &destination) {
        Ok(()) => Ok(DisabledVolume2Autostart::RenamedStartupFile { from: path.to_path_buf(), to: destination }),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => Err(Volume2ImportError::AdminRequired {
            message: format!("Renaming startup file {} needs administrator rights. Nothing was changed.", path.display()),
        }),
        Err(error) => Err(Volume2ImportError::Failed {
            message: format!("Could not rename {}: {error}. Nothing was changed.", path.display()),
        }),
    }
}

enum RegRead {
    Found(String),
    Missing,
    Error(WIN32_ERROR),
}

impl RegRead {
    fn found(self) -> Option<String> {
        match self {
            Self::Found(value) => Some(value),
            _ => None,
        }
    }
}

fn reg_get_sz(hive: HKEY, subkey: &str, value: &str) -> RegRead {
    let subkey = wide(subkey);
    let value = wide(value);
    let mut size = 0u32;
    let status = unsafe { RegGetValueW(hive, PCWSTR(subkey.as_ptr()), PCWSTR(value.as_ptr()), RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ, None, None, Some(&mut size)) };
    if status == ERROR_FILE_NOT_FOUND {
        return RegRead::Missing;
    }
    if status != ERROR_SUCCESS || size < 2 {
        if status == ERROR_SUCCESS {
            return RegRead::Found(String::new());
        }
        return RegRead::Error(status);
    }
    let mut buffer = vec![0u16; (size as usize / 2) + 1];
    let mut size = (buffer.len() * 2) as u32;
    let status = unsafe {
        RegGetValueW(
            hive,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_SZ | RRF_RT_REG_EXPAND_SZ,
            None,
            Some(buffer.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return RegRead::Missing;
    }
    if status != ERROR_SUCCESS {
        return RegRead::Error(status);
    }
    let len = (size as usize / 2).saturating_sub(1);
    RegRead::Found(String::from_utf16_lossy(&buffer[..len.min(buffer.len())]))
}

fn reg_get_dword(hive: HKEY, subkey: &str, value: &str) -> Option<u32> {
    let subkey = wide(subkey);
    let value = wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    let status = unsafe {
        RegGetValueW(
            hive,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    if status == ERROR_SUCCESS { Some(data) } else { None }
}

fn reg_set_sz(hive: HKEY, subkey: &str, value_name: &str, data: &str) -> WIN32_ERROR {
    let subkey = wide(subkey);
    let value_name = wide(value_name);
    let data = wide(data);
    unsafe {
        RegSetKeyValueW(
            hive,
            PCWSTR(subkey.as_ptr()),
            PCWSTR(value_name.as_ptr()),
            1,
            Some(data.as_ptr() as *const core::ffi::c_void),
            (data.len() * size_of::<u16>()) as u32,
        )
    }
}

fn reg_delete_value(hive: HKEY, subkey: &str, value_name: &str) -> WIN32_ERROR {
    let subkey = wide(subkey);
    let value_name = wide(value_name);
    unsafe { RegDeleteKeyValueW(hive, PCWSTR(subkey.as_ptr()), PCWSTR(value_name.as_ptr())) }
}

struct OpenKey(HKEY);

impl Drop for OpenKey {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = RegCloseKey(self.0);
            }
        }
    }
}

fn open_key(hive: HKEY, subkey: &str, access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Option<OpenKey> {
    let subkey = wide(subkey);
    let mut key = HKEY::default();
    let status = unsafe { RegOpenKeyExW(hive, PCWSTR(subkey.as_ptr()), 0, access, &mut key) };
    if status == ERROR_SUCCESS { Some(OpenKey(key)) } else { None }
}

fn enum_subkeys(key: HKEY) -> Vec<String> {
    let mut names = Vec::new();
    let mut index = 0u32;
    loop {
        let mut name = vec![0u16; 256];
        let mut name_len = name.len() as u32;
        let status = unsafe { RegEnumKeyExW(key, index, PWSTR(name.as_mut_ptr()), &mut name_len, None, PWSTR::null(), None, None) };
        if status == ERROR_NO_MORE_ITEMS || status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&name[..name_len as usize]));
        index += 1;
    }
    names
}

fn enum_value_names(key: HKEY) -> Vec<String> {
    let mut names = Vec::new();
    let mut index = 0u32;
    loop {
        let mut name = vec![0u16; 1024];
        let mut name_len = name.len() as u32;
        let status = unsafe { RegEnumValueW(key, index, PWSTR(name.as_mut_ptr()), &mut name_len, None, None, None, None) };
        if status == ERROR_NO_MORE_ITEMS || status != ERROR_SUCCESS {
            break;
        }
        names.push(String::from_utf16_lossy(&name[..name_len as usize]));
        index += 1;
    }
    names
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn decode_command_output(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return decode_utf16_le(&bytes[2..]).unwrap_or_default();
    }
    if bytes.len() >= 4 && bytes.len() % 2 == 0 && bytes[1] == 0 && bytes[3] == 0 {
        return decode_utf16_le(bytes).unwrap_or_default();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/main_parts/testdata/volume2_config.ini")
    }

    #[test]
    fn action_table_matches_volume2_26_tags() {
        let indexes = volume2_action_table().iter().map(|info| info.index).collect::<Vec<_>>();
        assert_eq!(indexes, (0..=62).collect::<Vec<_>>());
        assert_eq!(action_info(1).unwrap().action, ImportedAction::VolumePopup);
        assert_eq!(action_info(5).unwrap().action, ImportedAction::VolumeUp);
        assert_eq!(action_info(7).unwrap().action, ImportedAction::OpenMixer);
        assert_eq!(action_info(29).unwrap().action, ImportedAction::NextDefaultDevice);
        assert_eq!(action_info(35).unwrap().action, ImportedAction::SetDefaultAudioDevice);
        assert_eq!(action_info(36).unwrap().action, ImportedAction::RunApplication);
        assert_eq!(action_info(39).unwrap().action, ImportedAction::AudioDeviceManager);
        assert_eq!(action_info(49).unwrap().action, ImportedAction::AppMuteToggle);
        assert_eq!(action_info(57).unwrap().action, ImportedAction::PreviousDefaultDevice);
        assert!(action_info(63).is_none());
    }

    #[test]
    fn reads_sanitized_utf16_fixture_from_this_machine() {
        let bytes = fs::read(fixture_path()).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xFE]);
        let text = decode_config_bytes(&bytes).unwrap();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.to_ascii_lowercase();
            if key.contains("token") || key.contains("password") || key.contains("secret") {
                assert!(value.is_empty(), "{line}");
            }
        }

        let settings = parse_volume2_config(&text);
        assert!(!settings.wheel_enabled);
        assert_eq!(settings.wheel_zone, Some(ImportedWheelZone::Tray));
        assert!(settings.wheel_disabled_in_fullscreen);
        assert_eq!(settings.step_percent, Some(2));
        assert_eq!(settings.left_click, Some(ImportedAction::VolumePopup));
        assert_eq!(settings.double_click, Some(ImportedAction::VolumeUp));
        assert_eq!(settings.middle_click, Some(ImportedAction::OpenMixer));
        assert!(settings.middle_button_mutes);
        assert!(settings.osd_enabled);
        assert_eq!(settings.osd_duration_ms, Some(400));
        let position = settings.osd_position.expect("osd position");
        assert_eq!(position.x_percent, 50.0);
        assert!((position.y_percent - 92.037037037037).abs() < 1e-6);
        assert_eq!(settings.osd_monitor_index, Some(1));
        assert_eq!(settings.tray_skin, Some(ImportedSkin::Rings));
        assert!(settings.notify_on_external_change);
        assert!(settings.unknown.is_empty(), "{:?}", settings.unknown);
        assert_eq!(settings.hotkeys.len(), 1);
        let hotkey = &settings.hotkeys[0];
        assert_eq!(hotkey.combo, "F15");
        assert_eq!(hotkey.vk, 0x7E);
        assert_eq!(hotkey.modifiers, ImportedModifiers::default());
        assert_eq!(hotkey.action, ImportedAction::AppMuteToggle);
        assert_eq!(hotkey.volume2_index, 49);
        assert_eq!(hotkey.parameter, None);

        let reread = read_volume2_config(fixture_path()).unwrap();
        assert_eq!(reread.step_percent, Some(2));
        assert_eq!(reread.hotkeys.len(), 1);
    }

    #[test]
    fn parses_edge_cases_without_guessing() {
        let settings = parse_volume2_config(
            r#"
            [Main]
            VolSpeed=0
            [MouseActions]
            UseMouseScroll=1
            ScrollObject=1
            LeftMouseEvent=99
            DblLeftMouseEvent=0
            MidleMouseEvent=3
            UseDisableSoundMiddleButton=0
            DisableMouseScrollInFullScreen=0
            [OSD]
            UseOSD=0
            OSDShowTime=1500
            OSDPositionLeft=10,5
            OSDPositionTop=20.25
            MonitorIndex=-1
            [Skins]
            SkinName=Volume2 Default
            [System]
            NotifyVolume=0
            [ShortKeys]
            Win + Up=19
            Ctrl_Shift_Q=29
            RCtrl + F15=57
            VolumeMute=3
            MouseWheelUp=5
            MouseX1 + MouseMiddle=6
            F13=63
            Num+=8
            F15_Parameter=
            F14=34
            F14_Parameter=40
            "#,
        );
        assert!(settings.wheel_enabled);
        assert_eq!(settings.wheel_zone, Some(ImportedWheelZone::Taskbar));
        assert!(!settings.wheel_disabled_in_fullscreen);
        assert_eq!(settings.step_percent, None);
        assert_eq!(settings.left_click, None);
        assert_eq!(settings.double_click, Some(ImportedAction::None));
        assert_eq!(settings.middle_click, Some(ImportedAction::MuteToggle));
        assert!(!settings.middle_button_mutes);
        assert!(!settings.osd_enabled);
        assert_eq!(settings.osd_duration_ms, Some(1500));
        let position = settings.osd_position.unwrap();
        assert!((position.x_percent - 10.5).abs() < 1e-9);
        assert!((position.y_percent - 20.25).abs() < 1e-9);
        assert_eq!(settings.osd_monitor_index, Some(-1));
        assert_eq!(settings.tray_skin, Some(ImportedSkin::Digits));
        assert!(!settings.notify_on_external_change);

        let by_combo = settings.hotkeys.iter().map(|hotkey| (hotkey.combo.as_str(), hotkey)).collect::<BTreeMap<_, _>>();
        let next_device = by_combo["Ctrl_Shift_Q"];
        assert_eq!(next_device.vk, u32::from(b'Q'));
        assert!(next_device.modifiers.ctrl && next_device.modifiers.shift);
        assert!(!next_device.modifiers.alt && !next_device.modifiers.win);
        assert_eq!(next_device.action, ImportedAction::NextDefaultDevice);
        let previous = by_combo["RCtrl + F15"];
        assert!(previous.modifiers.ctrl && previous.modifiers.right_ctrl);
        assert_eq!(previous.vk, 0x7E);
        assert_eq!(previous.action, ImportedAction::PreviousDefaultDevice);
        let mute = by_combo["VolumeMute"];
        assert_eq!(mute.vk, 0xAD);
        assert_eq!(mute.action, ImportedAction::MuteToggle);
        let percent = by_combo["Num+"];
        assert_eq!(percent.vk, 0x6B);
        assert_eq!(percent.action, ImportedAction::SetVolumePercent(Some(10)));
        let set_percent = by_combo["F14"];
        assert_eq!(set_percent.action, ImportedAction::SetVolumePercent(Some(40)));
        assert_eq!(set_percent.parameter.as_deref(), Some("40"));
        assert_eq!(set_percent.volume2_index, 34);

        let unknown = settings.unknown.join("\n");
        assert!(unknown.contains("VolSpeed 0"), "{unknown}");
        assert!(unknown.contains("unknown LeftMouseEvent index 99"), "{unknown}");
        assert!(unknown.contains("ignored hotkey \"Win + Up\" action Brightness + (19)"), "{unknown}");
        assert!(unknown.contains("MouseWheelUp"), "{unknown}");
        assert!(unknown.contains("MouseX1 + MouseMiddle"), "{unknown}");
        assert!(unknown.contains("action index 63"), "{unknown}");
        assert!(!settings.hotkeys.iter().any(|hotkey| hotkey.combo == "Win + Up"));
        assert!(!settings.hotkeys.iter().any(|hotkey| hotkey.combo == "F15_Parameter"));
    }

    #[test]
    fn maps_skins_and_odd_step_counts() {
        let rings = parse_volume2_config("[Skins]\nSkinName=Rings Blue 2\n");
        assert_eq!(rings.tray_skin, Some(ImportedSkin::Rings));
        let digits = parse_volume2_config("[Skins]\nSkinName=My Digit Skin\n");
        assert_eq!(digits.tray_skin, Some(ImportedSkin::Digits));
        let speaker = parse_volume2_config("[Skins]\nSkinName=No icon\n");
        assert_eq!(speaker.tray_skin, Some(ImportedSkin::Speaker));
        let steps = parse_volume2_config("[Main]\nVolSpeed=3\n");
        assert_eq!(steps.step_percent, Some(33));
        assert!(steps.unknown.iter().any(|item| item.contains("truncated")));
        let too_many = parse_volume2_config("[Main]\nVolSpeed=101\n");
        assert_eq!(too_many.step_percent, None);
        let zone = parse_volume2_config("[MouseActions]\nScrollObject=0\n");
        assert_eq!(zone.wheel_zone, Some(ImportedWheelZone::NotificationAreaIcon));
        let osd = parse_volume2_config("[MouseActions]\nScrollObject=6\n");
        assert_eq!(osd.wheel_zone, Some(ImportedWheelZone::OsdWindow));
        let missing = parse_volume2_config("[MouseActions]\nScrollObject=8\n");
        assert_eq!(missing.wheel_zone, None);
        assert!(missing.unknown.iter().any(|item| item.contains("ScrollObject 8")));
    }

    #[test]
    fn decodes_utf8_and_utf16_configs() {
        let utf8 = parse_volume2_config("[Main]\nVolSpeed=25\n");
        assert_eq!(utf8.step_percent, Some(4));
        let ini = "[Main]\r\nVolSpeed=25\r\n";
        let mut bytes = vec![0xFF, 0xFE];
        for unit in ini.encode_utf16() {
            bytes.extend(unit.to_le_bytes());
        }
        let text = decode_config_bytes(&bytes).unwrap();
        assert_eq!(parse_volume2_config(&text).step_percent, Some(4));
        let error = read_volume2_config(Path::new(r"C:\missing\volume2-config-does-not-exist.ini")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn finds_a_volume2_scheduled_task_and_ignores_other_tasks() {
        let text = "\n\
TaskName:                             \\Activation-Renewal\n\
Task To Run:                          %ProgramFiles%\\Activation-Renewal\\Activation_task.cmd Task\n\
\n\
TaskName:                             \\Volume2\n\
Status:                               Ready\n\
Task To Run:                          \"C:\\Program Files\\Volume2\\Volume2.exe\" /autorun\n\
\n\
TaskName:                             \\Volume Controller\n\
Task To Run:                          C:\\Program Files\\Elgato\\Volume Controller\\watcher.exe\n";
        let tasks = scheduled_tasks_from_schtasks_output(text);
        assert_eq!(tasks.len(), 1);
        match &tasks[0] {
            Volume2Autostart::ScheduledTask { task_name, command } => {
                assert_eq!(task_name, "\\Volume2");
                assert!(command_is_volume2(command));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    /// Live read of this machine. Does not disable autostart and does not stop Volume².
    #[test]
    #[ignore]
    fn detect_volume2_on_this_machine() {
        let detection = detect_volume2();
        eprintln!("{detection:#?}");
    }
}
