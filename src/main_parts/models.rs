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
    SetVolume,
    IncreaseVolume,
    DecreaseVolume,
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
        Self::SetDefaultOutputDevice,
        Self::ToggleDefaultOutputDevice,
        Self::SetDefaultInputDevice,
        Self::ToggleDefaultInputDevice,
        Self::SetVolume,
        Self::IncreaseVolume,
        Self::DecreaseVolume,
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
            Self::SetDefaultInputDevice => "Set input device",
            Self::SetDefaultOutputDevice => "Set output device",
            Self::ToggleDefaultInputDevice => "Toggle input device",
            Self::ToggleDefaultOutputDevice => "Toggle output device",
            Self::SetVolume => "Set volume",
            Self::IncreaseVolume => "Increase volume",
            Self::DecreaseVolume => "Decrease volume",
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
            Self::SetVolume => "SetVolume",
            Self::IncreaseVolume => "IncreaseVolume",
            Self::DecreaseVolume => "DecreaseVolume",
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
            "SetVolume" => Self::SetVolume,
            "IncreaseVolume" => Self::IncreaseVolume,
            "DecreaseVolume" => Self::DecreaseVolume,
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
                | Self::SetVolume
                | Self::IncreaseVolume
                | Self::DecreaseVolume
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
    pub target_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_2_name: Option<String>,
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
            target_name: None,
            target_2: None,
            target_2_name: None,
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
    #[serde(default)]
    pub disable_auto_updates: bool,
    #[serde(default)]
    pub ungroup_tray_devices: bool,
    #[serde(default = "default_audio_device_name_display")]
    pub audio_device_name_display: String,
    #[serde(default = "default_enable_mica")]
    pub enable_mica: bool,
}

impl Default for AdvancedSettings {
    fn default() -> Self {
        Self {
            disable_tray_double_click_settings: false,
            disable_auto_updates: false,
            ungroup_tray_devices: false,
            audio_device_name_display: default_audio_device_name_display(),
            enable_mica: default_enable_mica(),
        }
    }
}

fn default_audio_device_name_display() -> String {
    AUDIO_DEVICE_NAME_PRETTY.to_string()
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
    mic_in_use: bool,
    available_update: Option<updater::UpdateInfo>,
}

struct AudioNotificationRegistration {
    enumerator: IMMDeviceEnumerator,
    endpoint_callback: IMMNotificationClient,
    volume_callback: IAudioEndpointVolumeCallback,
    volume: Option<IAudioEndpointVolume>,
    device_id: Option<String>,
}

impl AudioNotificationRegistration {
    fn new(hwnd: HWND) -> Result<Self> {
        let enumerator = audio_device_enumerator()?;
        let endpoint_callback: IMMNotificationClient =
            windows_core::ComObject::new(AudioDeviceNotificationSink { hwnd }).into_interface();
        let volume_callback: IAudioEndpointVolumeCallback =
            windows_core::ComObject::new(AudioEndpointVolumeSink { hwnd }).into_interface();
        unsafe {
            enumerator
                .RegisterEndpointNotificationCallback(&endpoint_callback)
                .context("register audio endpoint notification callback")?;
        }

        let mut registration = Self {
            enumerator,
            endpoint_callback,
            volume_callback,
            volume: None,
            device_id: None,
        };

        if let Err(err) = registration.rebind_default_capture_volume() {
            registration.unregister_endpoint_callback();
            return Err(err);
        }

        Ok(registration)
    }

    fn rebind_default_capture_volume(&mut self) -> Result<()> {
        let device = unsafe { capture_device(&self.enumerator, None)? };
        let device_id = unsafe { endpoint_device_id(&device)? };
        if self.device_id.as_deref() == Some(device_id.as_str()) {
            return Ok(());
        }

        let volume: IAudioEndpointVolume = unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .context("activate endpoint volume for mute notifications")?
        };
        unsafe {
            volume
                .RegisterControlChangeNotify(&self.volume_callback)
                .context("register endpoint mute notification callback")?;
        }

        if let Some(previous_volume) = self.volume.replace(volume) {
            unsafe {
                if let Err(err) =
                    previous_volume.UnregisterControlChangeNotify(&self.volume_callback)
                {
                    eprintln!("failed to unregister stale endpoint mute callback: {err:?}");
                }
            }
        }

        self.device_id = Some(device_id);
        Ok(())
    }

    fn shutdown(mut self) {
        self.unregister_volume_callback();
        self.unregister_endpoint_callback();
    }

    fn unregister_volume_callback(&mut self) {
        if let Some(volume) = self.volume.take() {
            unsafe {
                if let Err(err) = volume.UnregisterControlChangeNotify(&self.volume_callback) {
                    eprintln!("failed to unregister endpoint mute callback: {err:?}");
                }
            }
        }
        self.device_id = None;
    }

    fn unregister_endpoint_callback(&self) {
        unsafe {
            if let Err(err) = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.endpoint_callback)
            {
                eprintln!("failed to unregister audio endpoint notification callback: {err:?}");
            }
        }
    }
}

#[windows_core::implement(IAudioEndpointVolumeCallback)]
struct AudioEndpointVolumeSink {
    hwnd: HWND,
}

impl windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolumeCallback_Impl
    for AudioEndpointVolumeSink_Impl
{
    fn OnNotify(&self, _pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        post_audio_window_message(self.hwnd, WM_AUDIO_MUTE_STATE_CHANGED);
        Ok(())
    }
}

#[windows_core::implement(IMMNotificationClient)]
struct AudioDeviceNotificationSink {
    hwnd: HWND,
}

impl AudioDeviceNotificationSink_Impl {
    fn post_rebind(&self) {
        post_audio_window_message(self.hwnd, WM_AUDIO_ENDPOINT_CHANGED);
    }
}

impl windows::Win32::Media::Audio::IMMNotificationClient_Impl for AudioDeviceNotificationSink_Impl {
    fn OnDeviceStateChanged(
        &self,
        _pwstrdeviceid: &PCWSTR,
        _dwnewstate: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        self.post_rebind();
        Ok(())
    }

    fn OnDeviceAdded(&self, _pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        self.post_rebind();
        Ok(())
    }

    fn OnDeviceRemoved(&self, _pwstrdeviceid: &PCWSTR) -> windows::core::Result<()> {
        self.post_rebind();
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        _role: ERole,
        _pwstrdefaultdeviceid: &PCWSTR,
    ) -> windows::core::Result<()> {
        if flow == eCapture {
            self.post_rebind();
        }
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _pwstrdeviceid: &PCWSTR,
        _key: &windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
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
    #[serde(default = "default_overlay_display")]
    pub display: String,
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
    #[serde(default = "default_overlay_muted_label")]
    pub muted_label: String,
    #[serde(default = "default_overlay_unmuted_label")]
    pub unmuted_label: String,
    #[serde(default = "default_overlay_text_font")]
    pub text_font: String,
    #[serde(default = "default_overlay_text_font_weight")]
    pub text_font_weight: u16,
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
    #[serde(default = "default_overlay_behaviour")]
    pub behaviour: String,
    #[serde(default = "default_overlay_single_click")]
    pub single_click: OverlayActionBinding,
    #[serde(default)]
    pub double_click: OverlayActionBinding,
    #[serde(default)]
    pub middle_click: OverlayActionBinding,
    #[serde(default)]
    pub right_click: OverlayActionBinding,
    #[serde(default)]
    pub wheel_up: OverlayActionBinding,
    #[serde(default)]
    pub wheel_down: OverlayActionBinding,
    #[serde(default, skip_serializing)]
    pub single_click_action: Option<HotkeyAction>,
    #[serde(default, skip_serializing)]
    pub double_click_action: Option<HotkeyAction>,
    #[serde(default, skip_serializing)]
    pub middle_click_action: Option<HotkeyAction>,
    #[serde(default, skip_serializing)]
    pub right_click_action: Option<HotkeyAction>,
    #[serde(default, skip_serializing)]
    pub wheel_up_action: Option<HotkeyAction>,
    #[serde(default, skip_serializing)]
    pub wheel_down_action: Option<HotkeyAction>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct OverlayActionBinding {
    #[serde(default)]
    pub action: Option<HotkeyAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_2: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlayDisplay {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub primary: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SystemFont {
    pub family: String,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            enabled: default_overlay_enabled(),
            visibility: default_overlay_visibility(),
            display: default_overlay_display(),
            position_x: default_overlay_position_x(),
            position_y: default_overlay_position_y(),
            duration_secs: default_overlay_duration_secs(),
            scale: default_overlay_scale(),
            show_text: false,
            muted_label: default_overlay_muted_label(),
            unmuted_label: default_overlay_unmuted_label(),
            text_font: default_overlay_text_font(),
            text_font_weight: default_overlay_text_font_weight(),
            variant: default_overlay_variant(),
            icon_pair: crate::overlay_icons::default_overlay_icon_pair(),
            icon_style: default_overlay_icon_style(),
            background_style: default_overlay_background_style(),
            background_opacity: default_overlay_background_opacity(),
            content_opacity: default_overlay_content_opacity(),
            border_radius: default_overlay_border_radius(),
            show_border: default_overlay_show_border(),
            behaviour: default_overlay_behaviour(),
            single_click: default_overlay_single_click(),
            double_click: OverlayActionBinding::default(),
            middle_click: OverlayActionBinding::default(),
            right_click: OverlayActionBinding::default(),
            wheel_up: OverlayActionBinding::default(),
            wheel_down: OverlayActionBinding::default(),
            single_click_action: None,
            double_click_action: None,
            middle_click_action: None,
            right_click_action: None,
            wheel_up_action: None,
            wheel_down_action: None,
        }
    }
}

fn default_overlay_enabled() -> bool {
    true
}

fn default_overlay_visibility() -> String {
    "WhenMuted".to_string()
}

fn default_overlay_display() -> String {
    OVERLAY_DISPLAY_PRIMARY.to_string()
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

fn default_overlay_muted_label() -> String {
    "Microphone muted".to_string()
}

fn default_overlay_unmuted_label() -> String {
    "Microphone on".to_string()
}

fn default_overlay_text_font() -> String {
    "Segoe UI".to_string()
}

fn default_overlay_text_font_weight() -> u16 {
    700
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

fn default_overlay_behaviour() -> String {
    "PassThrough".to_string()
}

fn default_overlay_single_click() -> OverlayActionBinding {
    OverlayActionBinding {
        action: Some(HotkeyAction::ToggleMute),
        target: None,
        target_2: None,
    }
}

pub fn system_fonts() -> Vec<SystemFont> {
    let mut families = Vec::<String>::new();
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if !hdc.0.is_null() {
            let mut logfont = LOGFONTW {
                lfCharSet: DEFAULT_CHARSET,
                ..Default::default()
            };
            let _ = EnumFontFamiliesExW(
                hdc,
                &mut logfont,
                Some(collect_system_font),
                LPARAM(&mut families as *mut _ as isize),
                0,
            );
            let _ = DeleteDC(hdc);
        }
    }

    families.sort_by_key(|family| family.to_ascii_lowercase());
    families.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    if families.is_empty() {
        families = vec![
            "Segoe UI".to_string(),
            "Arial".to_string(),
            "Calibri".to_string(),
            "Tahoma".to_string(),
            "Verdana".to_string(),
        ];
    }

    families
        .into_iter()
        .map(|family| SystemFont { family })
        .collect()
}

unsafe extern "system" fn collect_system_font(
    logfont: *const LOGFONTW,
    _text_metric: *const TEXTMETRICW,
    _font_type: u32,
    lparam: LPARAM,
) -> i32 {
    if logfont.is_null() || lparam.0 == 0 {
        return 1;
    }

    let families = unsafe { &mut *(lparam.0 as *mut Vec<String>) };
    let face = unsafe { wide_buf_to_string(&(*logfont).lfFaceName) };
    if face.is_empty() || face.starts_with('@') {
        return 1;
    }

    families.push(face);
    1
}

fn wide_buf_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|ch| *ch == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).trim().to_string()
}

pub fn overlay_displays() -> Vec<OverlayDisplay> {
    let mut monitors = Vec::<MonitorSnapshot>::new();
    let mut friendly_names = display_config_target_names();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(collect_monitor_snapshot),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }

    if monitors.is_empty() {
        return vec![OverlayDisplay {
            id: OVERLAY_DISPLAY_PRIMARY.to_string(),
            label: "Primary display".to_string(),
            detail: "Windows primary monitor".to_string(),
            primary: true,
        }];
    }

    monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let display_number = index + 1;
            let fallback_label = if monitor.primary {
                format!("Display {display_number} (primary)")
            } else {
                format!("Display {display_number}")
            };
            let friendly_name = friendly_names.pop_front().filter(|name| !name.is_empty());
            let label = match (friendly_name, monitor.primary) {
                (Some(name), true) => format!("{name} – Display {display_number} (primary)"),
                (Some(name), false) => format!("{name} – Display {display_number}"),
                (None, _) => fallback_label,
            };
            let width = monitor.rect.right - monitor.rect.left;
            let height = monitor.rect.bottom - monitor.rect.top;
            OverlayDisplay {
                id: if monitor.primary {
                    OVERLAY_DISPLAY_PRIMARY.to_string()
                } else {
                    monitor.device_name
                },
                label,
                detail: format!("{width} x {height}"),
                primary: monitor.primary,
            }
        })
        .collect()
}

#[derive(Clone)]
struct MonitorSnapshot {
    rect: RECT,
    device_name: String,
    primary: bool,
}

unsafe extern "system" fn collect_monitor_snapshot(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorSnapshot>) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        monitors.push(MonitorSnapshot {
            rect: info.rcMonitor,
            device_name: format!("Monitor{}", monitors.len() + 1),
            primary: (info.dwFlags & 1) != 0,
        });
    }
    true.into()
}

fn display_config_target_names() -> VecDeque<String> {
    let mut path_count = 0;
    let mut mode_count = 0;
    let flags = QDC_ONLY_ACTIVE_PATHS;

    let status = unsafe { GetDisplayConfigBufferSizes(flags, &mut path_count, &mut mode_count) };
    if status != ERROR_SUCCESS || path_count == 0 {
        return VecDeque::new();
    }

    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    let status = unsafe {
        QueryDisplayConfig(
            flags,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return VecDeque::new();
    }

    paths.truncate(path_count as usize);
    paths
        .iter()
        .filter_map(|path| display_config_target_name(path))
        .collect()
}

fn display_config_target_name(path: &DISPLAYCONFIG_PATH_INFO) -> Option<String> {
    let mut target_name = DISPLAYCONFIG_TARGET_DEVICE_NAME::default();
    target_name.header = DISPLAYCONFIG_DEVICE_INFO_HEADER {
        r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
        size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
        adapterId: path.targetInfo.adapterId,
        id: path.targetInfo.id,
    };

    let status = unsafe {
        DisplayConfigGetDeviceInfo(&mut target_name.header as *mut DISPLAYCONFIG_DEVICE_INFO_HEADER)
    };
    if status != 0 {
        return None;
    }

    let name = wide_slice_to_string(&target_name.monitorFriendlyDeviceName);
    if name.is_empty() { None } else { Some(name) }
}

fn wide_slice_to_string(value: &[u16]) -> String {
    let len = value
        .iter()
        .position(|item| *item == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..len]).trim().to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TrayIconConfig {
    #[serde(default = "default_tray_icon_variant")]
    pub variant: String,
    #[serde(default = "crate::overlay_icons::default_overlay_icon_pair")]
    pub icon_pair: String,
    #[serde(default = "default_tray_icon_status_style")]
    pub status_style: String,
    #[serde(default = "default_tray_icon_show_mic_in_use")]
    pub show_mic_in_use: bool,
}

impl Default for TrayIconConfig {
    fn default() -> Self {
        Self {
            variant: default_tray_icon_variant(),
            icon_pair: crate::overlay_icons::default_overlay_icon_pair(),
            status_style: default_tray_icon_status_style(),
            show_mic_in_use: default_tray_icon_show_mic_in_use(),
        }
    }
}

fn default_tray_icon_variant() -> String {
    "StatusMic".to_string()
}

fn default_tray_icon_status_style() -> String {
    "Colored".to_string()
}

fn default_tray_icon_show_mic_in_use() -> bool {
    true
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
    pub system_name: String,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub system_name: String,
    pub is_default: bool,
}

impl MicDevice {
    pub fn display_name(&self, mode: &str) -> String {
        display_audio_device_name(&self.name, &self.system_name, mode)
    }
}

impl AudioDevice {
    pub fn display_name(&self, mode: &str) -> String {
        display_audio_device_name(&self.name, &self.system_name, mode)
    }
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
            mic_in_use: false,
            available_update: None,
        }
    }
}

unsafe impl Send for AppState {}

