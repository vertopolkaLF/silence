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

thread_local! {
    static AUDIO_NOTIFICATION_REGISTRATION: RefCell<Option<AudioNotificationRegistration>> =
        const { RefCell::new(None) };
}
static SETTINGS_GAMEPAD_HELD: Lazy<Mutex<HashSet<GamepadInput>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));
static SETTINGS_MOUSE_HELD: Lazy<Mutex<Vec<u32>>> = Lazy::new(|| Mutex::new(Vec::new()));
static SETTINGS_MOUSE_PRESSED_SHORTCUT: Lazy<Mutex<Option<Shortcut>>> =
    Lazy::new(|| Mutex::new(None));
static TRAY_DEVICE_COMMANDS: Lazy<Mutex<HashMap<usize, TrayDeviceCommand>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static NORMALIZED_AUDIO_DEVICE_NAMES: Lazy<Mutex<HashSet<(String, String)>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));
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
    MicApp(u32),
}

#[derive(Clone, Debug)]
struct MicUsingApp {
    pid: u32,
    name: String,
}
