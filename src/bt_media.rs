//! Bluetooth and external HID media/volume event interception plus Windows
//! native volume OSD suppression.
//!
//! Keyboard volume keys are swallowed in `WH_KEYBOARD_LL` before Explorer sees
//! them. Bluetooth headsets bypass `WH_KEYBOARD_LL` completely:
//! - AVRCP Absolute Volume updates the Core Audio endpoint directly, which
//!   triggers both silence!'s `IAudioEndpointVolumeCallback` and Explorer's
//!   native Hardware Confirmator OSD (`XamlExplorerHostIslandWindow` on Win11
//!   22H2+, `NativeHWNDHost` + `DirectUIHWND` on Win10 / Win11 21H2).
//! - AVRCP relative / HID Consumer Control headsets deliver events via HID
//!   Raw Input (`WM_INPUT`, Usage Page `0x0C`) and `HSHELL_APPCOMMAND`
//!   (`SHELLHOOK`, `FAPPCOMMAND_OEM`).
//!
//! This module:
//! 1. Listens to `HSHELL_APPCOMMAND` (`RegisterShellHookWindow`) and background
//!    HID Consumer Control Raw Input (`RIDEV_INPUTSINK`) so Bluetooth headset
//!    play/pause/next/previous/stop and volume buttons reach silence!.
//! 2. Temporarily suppresses Windows' native volume OSD during volume changes
//!    (using `SetWinEventHook` + async off-screen minimize) and restores it
//!    once quiet so laptop brightness OSDs (which share the same host window on
//!    Windows 11) keep working.

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    ffi::c_void,
    mem::size_of,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Devices::HumanInterfaceDevice::{
            HIDP_STATUS_SUCCESS, HidP_GetUsages, HidP_Input, PHIDP_PREPARSED_DATA,
        },
        Foundation::{BOOL, CloseHandle, HANDLE, HLOCAL, HWND, LPARAM, LocalFree, RECT, WPARAM},
        System::{
            LibraryLoader::{GetModuleHandleW, GetProcAddress},
            Threading::{OpenThread, THREAD_QUERY_LIMITED_INFORMATION},
        },
        UI::{
            Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
            Input::{
                GetRawInputData, GetRawInputDeviceInfoW, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE,
                RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIDI_PREPARSEDDATA,
                RIM_TYPEHID, RegisterRawInputDevices,
            },
            WindowsAndMessaging::{
                DeregisterShellHookWindow, FindWindowExW, GWL_EXSTYLE, GetClassNameW,
                GetShellWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
                HSHELL_APPCOMMAND, IsIconic, IsWindow, IsWindowVisible, KillTimer,
                RegisterShellHookWindow, SW_HIDE, SW_SHOWMINNOACTIVE, SW_SHOWNOACTIVATE,
                SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetTimer,
                SetWindowLongW, SetWindowPos, ShowWindowAsync, WS_EX_LAYERED, WS_EX_TRANSPARENT,
            },
        },
    },
    core::{HRESULT, PCWSTR, PWSTR, s, w},
};

use crate::media_session::MediaKey;

const APPCOMMAND_VOLUME_MUTE: u16 = 8;
const APPCOMMAND_VOLUME_DOWN: u16 = 9;
const APPCOMMAND_VOLUME_UP: u16 = 10;
const APPCOMMAND_MEDIA_NEXTTRACK: u16 = 11;
const APPCOMMAND_MEDIA_PREVIOUSTRACK: u16 = 12;
const APPCOMMAND_MEDIA_STOP: u16 = 13;
const APPCOMMAND_MEDIA_PLAY_PAUSE: u16 = 14;
const APPCOMMAND_MEDIA_PLAY: u16 = 46;
const APPCOMMAND_MEDIA_PAUSE: u16 = 47;

const FAPPCOMMAND_MASK: u16 = 0xF000;
const FAPPCOMMAND_KEY: u16 = 0x0000;
const FAPPCOMMAND_OEM: u16 = 0x1000;

const HID_USAGE_PAGE_CONSUMER: u16 = 0x000C;
const HID_USAGE_CONSUMER_CONTROL: u16 = 0x0001;

const CONSUMER_USAGE_PLAY: u16 = 0x00B0;
const CONSUMER_USAGE_PAUSE: u16 = 0x00B1;
const CONSUMER_USAGE_SCAN_NEXT: u16 = 0x00B5;
const CONSUMER_USAGE_SCAN_PREVIOUS: u16 = 0x00B6;
const CONSUMER_USAGE_STOP: u16 = 0x00B7;
const CONSUMER_USAGE_PLAY_PAUSE: u16 = 0x00CD;
const CONSUMER_USAGE_MUTE: u16 = 0x00E2;
const CONSUMER_USAGE_VOLUME_INC: u16 = 0x00E9;
const CONSUMER_USAGE_VOLUME_DEC: u16 = 0x00EA;

const EVENT_OBJECT_CREATE: u32 = 0x8000;
const EVENT_OBJECT_DESTROY: u32 = 0x8001;
const EVENT_OBJECT_SHOW: u32 = 0x8002;
const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
const OBJID_WINDOW: i32 = 0;
const CHILDID_SELF: i32 = 0;

const WIN11_22H2_BUILD: u32 = 22_620;
const ZBID_DESKTOP: u32 = 1;
const ZBID_ABOVELOCK_UX: u32 = 18;
const OFF_SCREEN_COORD: i32 = -32_000;
const OFF_SCREEN_THRESHOLD: i32 = -20_000;
const MAX_OSD_WIDTH: i32 = 900;
const MAX_OSD_HEIGHT: i32 = 350;
const REHIDE_THROTTLE: Duration = Duration::from_millis(15);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BtMediaEvent {
    Volume,
    Transport(MediaKey),
}

struct NativeOsdState {
    owner_hwnd: HWND,
    windows_build: u32,
    win_event_hook: HWINEVENTHOOK,
    cached_osd: Option<HWND>,
    hidden: bool,
    orig_ex_style: i32,
    orig_pos: (i32, i32),
    suppress_until: Option<Instant>,
    last_hide_at: Option<Instant>,
    in_self_op: bool,
    /// Per-device HID preparsed descriptor data (`hDevice` -> bytes).
    hid_preparsed: HashMap<isize, Vec<u8>>,
    /// Consumer usages currently held per HID device, for rising-edge detection.
    hid_pressed: HashMap<isize, HashSet<u16>>,
}

impl Default for NativeOsdState {
    fn default() -> Self {
        Self {
            owner_hwnd: HWND::default(),
            windows_build: 0,
            win_event_hook: HWINEVENTHOOK::default(),
            cached_osd: None,
            hidden: false,
            orig_ex_style: 0,
            orig_pos: (0, 0),
            suppress_until: None,
            last_hide_at: None,
            in_self_op: false,
            hid_preparsed: HashMap::new(),
            hid_pressed: HashMap::new(),
        }
    }
}

thread_local! {
    static STATE: RefCell<NativeOsdState> = RefCell::new(NativeOsdState::default());
}

/// Register shell hook, background Consumer Control Raw Input, and Explorer's
/// WinEvent hook on the tray hidden window. Tray UI thread only.
pub fn install(hwnd: HWND, windows_build: u32) {
    if hwnd.0.is_null() {
        return;
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.owner_hwnd = hwnd;
        state.windows_build = windows_build;
    });

    unsafe {
        if !RegisterShellHookWindow(hwnd).as_bool() {
            eprintln!("bt_media: RegisterShellHookWindow failed");
        }
    }
    register_consumer_raw_input(hwnd, true);
    install_win_event_hook();
}

/// Explorer restarted (`TaskbarCreated`): re-register the shell hook and bind
/// the WinEvent hook to the new `explorer.exe` process.
pub fn on_taskbar_created(hwnd: HWND) {
    if hwnd.0.is_null() {
        return;
    }
    unsafe {
        let _ = DeregisterShellHookWindow(hwnd);
        let _ = RegisterShellHookWindow(hwnd);
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.cached_osd = None;
        state.hidden = false;
        state.suppress_until = None;
    });
    install_win_event_hook();
}

/// Restore the native OSD and remove all hooks on tray exit.
pub fn shutdown(hwnd: HWND) {
    restore_native_osd(hwnd);
    unhook_win_event();
    if !hwnd.0.is_null() {
        register_consumer_raw_input(hwnd, false);
        unsafe {
            let _ = DeregisterShellHookWindow(hwnd);
        }
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.hid_preparsed.clear();
        state.hid_pressed.clear();
        state.cached_osd = None;
        state.owner_hwnd = HWND::default();
    });
}

/// Handle a `SHELLHOOK` message from `main_wnd_proc`.
pub fn handle_shell_hook(wparam: WPARAM, lparam: LPARAM) -> Option<BtMediaEvent> {
    if wparam.0 != HSHELL_APPCOMMAND as usize {
        return None;
    }
    decode_shell_appcommand(lparam.0)
}

/// Decode `HSHELL_APPCOMMAND`'s `lParam` into a volume or media transport event.
pub fn decode_shell_appcommand(lparam: isize) -> Option<BtMediaEvent> {
    let hi = ((lparam as usize) >> 16) as u16;
    let cmd = hi & !FAPPCOMMAND_MASK;
    let device = hi & FAPPCOMMAND_MASK;
    // Ignore mouse side-buttons (`FAPPCOMMAND_MOUSE`); accept headset/BT/HID
    // (`FAPPCOMMAND_OEM`) and keyboard (`FAPPCOMMAND_KEY`).
    if device != FAPPCOMMAND_OEM && device != FAPPCOMMAND_KEY {
        return None;
    }
    match cmd {
        APPCOMMAND_VOLUME_MUTE | APPCOMMAND_VOLUME_DOWN | APPCOMMAND_VOLUME_UP => {
            Some(BtMediaEvent::Volume)
        }
        APPCOMMAND_MEDIA_PLAY_PAUSE => Some(BtMediaEvent::Transport(MediaKey::PlayPause)),
        APPCOMMAND_MEDIA_PLAY => Some(BtMediaEvent::Transport(MediaKey::Play)),
        APPCOMMAND_MEDIA_PAUSE => Some(BtMediaEvent::Transport(MediaKey::Pause)),
        APPCOMMAND_MEDIA_NEXTTRACK => Some(BtMediaEvent::Transport(MediaKey::Next)),
        APPCOMMAND_MEDIA_PREVIOUSTRACK => Some(BtMediaEvent::Transport(MediaKey::Previous)),
        APPCOMMAND_MEDIA_STOP => Some(BtMediaEvent::Transport(MediaKey::Stop)),
        _ => None,
    }
}

/// Decode a HID Consumer Page (`0x0C`) usage ID into a volume or media event.
pub fn decode_consumer_usage(usage: u16) -> Option<BtMediaEvent> {
    match usage {
        CONSUMER_USAGE_VOLUME_INC | CONSUMER_USAGE_VOLUME_DEC | CONSUMER_USAGE_MUTE => {
            Some(BtMediaEvent::Volume)
        }
        CONSUMER_USAGE_PLAY_PAUSE => Some(BtMediaEvent::Transport(MediaKey::PlayPause)),
        CONSUMER_USAGE_PLAY => Some(BtMediaEvent::Transport(MediaKey::Play)),
        CONSUMER_USAGE_PAUSE => Some(BtMediaEvent::Transport(MediaKey::Pause)),
        CONSUMER_USAGE_SCAN_NEXT => Some(BtMediaEvent::Transport(MediaKey::Next)),
        CONSUMER_USAGE_SCAN_PREVIOUS => Some(BtMediaEvent::Transport(MediaKey::Previous)),
        CONSUMER_USAGE_STOP => Some(BtMediaEvent::Transport(MediaKey::Stop)),
        _ => None,
    }
}

/// Handle `WM_INPUT` for background HID Consumer Control devices (BT/USB headsets).
/// Returns only newly pressed (rising-edge) events.
pub fn handle_raw_input(lparam: LPARAM) -> Vec<BtMediaEvent> {
    let raw_handle = HRAWINPUT(lparam.0 as *mut c_void);
    let mut size = 0u32;
    let header_size = size_of::<RAWINPUTHEADER>() as u32;
    let query = unsafe { GetRawInputData(raw_handle, RID_INPUT, None, &mut size, header_size) };
    if query == u32::MAX || size < header_size {
        return Vec::new();
    }
    let mut buffer = vec![0u8; size as usize];
    let read = unsafe {
        GetRawInputData(
            raw_handle,
            RID_INPUT,
            Some(buffer.as_mut_ptr() as *mut c_void),
            &mut size,
            header_size,
        )
    };
    if read == u32::MAX || read < header_size {
        return Vec::new();
    }
    let (device_handle, device_key, report_size, report_count, raw_data_offset) = {
        let raw = unsafe { &*(buffer.as_ptr() as *const RAWINPUT) };
        if raw.header.dwType != RIM_TYPEHID.0 {
            return Vec::new();
        }
        let device_handle = raw.header.hDevice;
        let device_key = device_handle.0 as isize;
        let hid = unsafe { &raw.data.hid };
        let report_size = hid.dwSizeHid as usize;
        let report_count = hid.dwCount as usize;
        let base = buffer.as_ptr() as usize;
        let data_ptr = std::ptr::addr_of!(hid.bRawData) as usize;
        let raw_data_offset = data_ptr.saturating_sub(base);
        (
            device_handle,
            device_key,
            report_size,
            report_count,
            raw_data_offset,
        )
    };
    if report_size == 0 || report_count == 0 {
        return Vec::new();
    }
    let needed = raw_data_offset.saturating_add(report_size.saturating_mul(report_count));
    if needed > read as usize {
        return Vec::new();
    }
    let reports = &mut buffer[raw_data_offset..needed];

    let mut events = Vec::new();
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if !state.hid_preparsed.contains_key(&device_key) {
            let Some(preparsed) = load_hid_preparsed_data(device_handle) else {
                return;
            };
            state.hid_preparsed.insert(device_key, preparsed);
        }
        let preparsed_ptr = state
            .hid_preparsed
            .get(&device_key)
            .map(|data| PHIDP_PREPARSED_DATA(data.as_ptr() as isize));
        let Some(preparsed) = preparsed_ptr else {
            return;
        };

        let mut usages = [0u16; 16];
        for report in reports.chunks_exact_mut(report_size) {
            let mut usage_count = usages.len() as u32;
            let status = unsafe {
                HidP_GetUsages(
                    HidP_Input,
                    HID_USAGE_PAGE_CONSUMER,
                    0,
                    usages.as_mut_ptr(),
                    &mut usage_count,
                    preparsed,
                    report,
                )
            };
            if status != HIDP_STATUS_SUCCESS {
                continue;
            }
            let current: HashSet<u16> = usages[..usage_count as usize].iter().copied().collect();
            let prev = state.hid_pressed.entry(device_key).or_default();
            for &usage in &current {
                if !prev.contains(&usage)
                    && let Some(event) = decode_consumer_usage(usage)
                {
                    events.push(event);
                }
            }
            if current.is_empty() {
                state.hid_pressed.remove(&device_key);
            } else {
                *prev = current;
            }
        }
    });
    events
}

/// Suppress Windows' native volume OSD for the duration of a volume burst,
/// scheduling an automatic restore so brightness OSDs keep working.
pub fn suppress_native_osd(owner_hwnd: HWND, windows_build: u32) {
    if owner_hwnd.0.is_null() {
        return;
    }
    let now = Instant::now();
    let until = now + Duration::from_millis(u64::from(crate::NATIVE_OSD_RESTORE_DELAY_MS));

    let found = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.owner_hwnd = owner_hwnd;
        state.windows_build = windows_build;
        state.suppress_until = Some(until);
        if state.win_event_hook.is_invalid() {
            drop(state);
            install_win_event_hook();
            state = cell.borrow_mut();
        }
        try_hide_native_osd_locked(&mut state, now)
    });

    unsafe {
        let _ = SetTimer(
            owner_hwnd,
            crate::ID_NATIVE_OSD_RESTORE_TIMER,
            crate::NATIVE_OSD_RESTORE_DELAY_MS,
            None,
        );
        if !found {
            let _ = SetTimer(
                owner_hwnd,
                crate::ID_NATIVE_OSD_RETRY_TIMER,
                crate::NATIVE_OSD_RETRY_DELAY_MS,
                None,
            );
        } else {
            let _ = KillTimer(owner_hwnd, crate::ID_NATIVE_OSD_RETRY_TIMER);
        }
    }
}

/// One-shot retry if Explorer hadn't created its OSD host window yet when the
/// first Bluetooth volume change arrived.
pub fn on_retry_timer(owner_hwnd: HWND) {
    unsafe {
        let _ = KillTimer(owner_hwnd, crate::ID_NATIVE_OSD_RETRY_TIMER);
    }
    let now = Instant::now();
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        if state.suppress_until.is_some_and(|until| until > now) {
            let _ = try_hide_native_osd_locked(&mut state, now);
        }
    });
}

/// Restore the native OSD window cleanly (off-screen un-minimize + hide +
/// restore original style and coordinates) so nothing flashes and brightness
/// OSDs work normally.
pub fn restore_native_osd(owner_hwnd: HWND) {
    if !owner_hwnd.0.is_null() {
        unsafe {
            let _ = KillTimer(owner_hwnd, crate::ID_NATIVE_OSD_RESTORE_TIMER);
            let _ = KillTimer(owner_hwnd, crate::ID_NATIVE_OSD_RETRY_TIMER);
        }
    }
    STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        state.suppress_until = None;
        if !state.hidden {
            return;
        }
        state.hidden = false;
        let Some(hwnd) = state.cached_osd else {
            return;
        };
        if !unsafe { IsWindow(hwnd).as_bool() } {
            state.cached_osd = None;
            return;
        }
        let orig_ex_style = state.orig_ex_style;
        let (orig_x, orig_y) = state.orig_pos;
        state.in_self_op = true;
        unsafe {
            // Un-minimize while still off-screen, hide immediately, then
            // restore style and original position while hidden.
            let _ = ShowWindowAsync(hwnd, SW_SHOWNOACTIVATE);
            let _ = ShowWindowAsync(hwnd, SW_HIDE);
            let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, orig_ex_style);
            let _ = SetWindowPos(
                hwnd,
                None,
                orig_x,
                orig_y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
            );
        }
        state.in_self_op = false;
    });
}

fn try_hide_native_osd_locked(state: &mut NativeOsdState, now: Instant) -> bool {
    if let Some(hwnd) = state.cached_osd
        && !unsafe { IsWindow(hwnd).as_bool() }
    {
        state.cached_osd = None;
        state.hidden = false;
    }

    let hwnd = match state.cached_osd {
        Some(hwnd) => hwnd,
        None => match find_native_osd_window(state.windows_build) {
            Some(hwnd) => {
                state.cached_osd = Some(hwnd);
                hwnd
            }
            None => return false,
        },
    };

    hide_hwnd_locked(state, hwnd, now);
    true
}

fn hide_hwnd_locked(state: &mut NativeOsdState, hwnd: HWND, now: Instant) {
    if state.in_self_op {
        return;
    }
    if state.hidden
        && state
            .last_hide_at
            .is_some_and(|last| now.duration_since(last) < REHIDE_THROTTLE)
    {
        return;
    }
    if !state.hidden {
        let ex_style = unsafe { GetWindowLongW(hwnd, GWL_EXSTYLE) };
        let mut rect = RECT::default();
        let pos = if unsafe { GetWindowRect(hwnd, &mut rect).is_ok() }
            && rect.left > OFF_SCREEN_THRESHOLD
            && rect.top > OFF_SCREEN_THRESHOLD
        {
            (rect.left, rect.top)
        } else {
            (0, 0)
        };
        state.orig_ex_style = ex_style;
        state.orig_pos = pos;
    }

    let layered_transparent = (WS_EX_LAYERED | WS_EX_TRANSPARENT).0 as i32;
    let target_style = state.orig_ex_style | layered_transparent;
    state.in_self_op = true;
    unsafe {
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, target_style);
        let _ = SetWindowPos(
            hwnd,
            None,
            OFF_SCREEN_COORD,
            OFF_SCREEN_COORD,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS,
        );
        let _ = ShowWindowAsync(hwnd, SW_SHOWMINNOACTIVE);
    }
    state.in_self_op = false;
    state.hidden = true;
    state.last_hide_at = Some(now);
}

fn register_consumer_raw_input(hwnd: HWND, enable: bool) {
    let device = RAWINPUTDEVICE {
        usUsagePage: HID_USAGE_PAGE_CONSUMER,
        usUsage: HID_USAGE_CONSUMER_CONTROL,
        dwFlags: if enable { RIDEV_INPUTSINK } else { RIDEV_REMOVE },
        hwndTarget: if enable { hwnd } else { HWND::default() },
    };
    unsafe {
        if let Err(err) = RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) {
            eprintln!("bt_media: RegisterRawInputDevices({enable}) failed: {err:?}");
        }
    }
}

fn load_hid_preparsed_data(device: HANDLE) -> Option<Vec<u8>> {
    let mut size = 0u32;
    let query = unsafe { GetRawInputDeviceInfoW(device, RIDI_PREPARSEDDATA, None, &mut size) };
    if query == u32::MAX || size == 0 {
        return None;
    }
    let mut data = vec![0u8; size as usize];
    let read = unsafe {
        GetRawInputDeviceInfoW(
            device,
            RIDI_PREPARSEDDATA,
            Some(data.as_mut_ptr() as *mut c_void),
            &mut size,
        )
    };
    if read == u32::MAX {
        return None;
    }
    Some(data)
}

fn install_win_event_hook() {
    unhook_win_event();
    let Some(shell_pid) = shell_explorer_pid() else {
        return;
    };
    let hook = unsafe {
        SetWinEventHook(
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_LOCATIONCHANGE,
            None,
            Some(win_event_proc),
            shell_pid,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        )
    };
    if hook.is_invalid() {
        eprintln!("bt_media: SetWinEventHook failed for explorer pid {shell_pid}");
        return;
    }
    STATE.with(|cell| {
        cell.borrow_mut().win_event_hook = hook;
    });
}

fn unhook_win_event() {
    let hook = STATE.with(|cell| {
        let mut state = cell.borrow_mut();
        std::mem::take(&mut state.win_event_hook)
    });
    if !hook.is_invalid() {
        unsafe {
            let _ = UnhookWinEvent(hook);
        }
    }
}

unsafe extern "system" fn win_event_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if id_object != OBJID_WINDOW || id_child != CHILDID_SELF || hwnd.0.is_null() {
        return;
    }
    if !matches!(
        event,
        EVENT_OBJECT_CREATE
            | EVENT_OBJECT_DESTROY
            | EVENT_OBJECT_SHOW
            | EVENT_OBJECT_LOCATIONCHANGE
    ) {
        return;
    }

    STATE.with(|cell| {
        let Ok(mut state) = cell.try_borrow_mut() else {
            return;
        };
        if state.in_self_op {
            return;
        }
        if event == EVENT_OBJECT_DESTROY {
            if state.cached_osd == Some(hwnd) {
                state.cached_osd = None;
                state.hidden = false;
            }
            return;
        }

        let is_cached = state.cached_osd == Some(hwnd);
        if !is_cached {
            if !is_native_osd_window(hwnd, state.windows_build) {
                return;
            }
            state.cached_osd = Some(hwnd);
        }

        let now = Instant::now();
        let suppressing = state.suppress_until.is_some_and(|until| until > now);
        if !suppressing {
            return;
        }

        // Avoid a feedback loop from our own async hide: only re-hide when
        // Explorer un-minimizes the window or moves it back onto a monitor.
        let needs_hide = !state.hidden
            || !unsafe { IsIconic(hwnd).as_bool() }
            || window_is_on_screen(hwnd);
        if needs_hide {
            hide_hwnd_locked(&mut state, hwnd, now);
        }
    });
}

fn shell_explorer_pid() -> Option<u32> {
    let shell = unsafe { GetShellWindow() };
    if shell.0.is_null() {
        return None;
    }
    let mut pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(shell, Some(&mut pid));
    }
    (pid != 0).then_some(pid)
}

fn window_pid_and_tid(hwnd: HWND) -> (u32, u32) {
    let mut pid = 0u32;
    let tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid, tid)
}

fn window_class_name(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 128];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    if len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn window_is_on_screen(hwnd: HWND) -> bool {
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
        return false;
    }
    rect.left > OFF_SCREEN_THRESHOLD && rect.top > OFF_SCREEN_THRESHOLD
}

fn is_osd_sized_rect(rect: &RECT) -> bool {
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    width > 0 && height > 0 && width <= MAX_OSD_WIDTH && height <= MAX_OSD_HEIGHT
}

fn is_known_non_osd_thread_description(desc: &str) -> bool {
    let lower = desc.to_ascii_lowercase();
    lower == "multitaskingview" || lower.contains("snap") || lower.contains("taskview")
}

fn find_native_osd_window(windows_build: u32) -> Option<HWND> {
    let shell_pid = shell_explorer_pid()?;
    if windows_build >= WIN11_22H2_BUILD {
        find_win11_osd_window(shell_pid)
    } else {
        find_win10_osd_window(shell_pid)
    }
}

fn find_win11_osd_window(shell_pid: u32) -> Option<HWND> {
    let mut cursor = HWND::default();
    let mut best: Option<(u8, HWND)> = None;
    loop {
        let Ok(hwnd) = (unsafe {
            FindWindowExW(
                HWND::default(),
                cursor,
                w!("XamlExplorerHostIslandWindow"),
                PCWSTR::null(),
            )
        }) else {
            break;
        };
        if hwnd.0.is_null() {
            break;
        }
        cursor = hwnd;

        let Some(score) = score_win11_osd_candidate(hwnd, shell_pid) else {
            continue;
        };
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, hwnd));
        }
    }
    best.map(|(_, hwnd)| hwnd)
}

fn find_win10_osd_window(shell_pid: u32) -> Option<HWND> {
    let mut cursor = HWND::default();
    let mut found = None;
    let mut count = 0usize;
    loop {
        let Ok(hwnd) =
            (unsafe { FindWindowExW(HWND::default(), cursor, w!("NativeHWNDHost"), w!("")) })
        else {
            break;
        };
        if hwnd.0.is_null() {
            break;
        }
        cursor = hwnd;
        if !is_win10_osd_candidate(hwnd, shell_pid) {
            continue;
        }
        found = Some(hwnd);
        count += 1;
    }
    if count == 1 { found } else { None }
}

fn is_native_osd_window(hwnd: HWND, windows_build: u32) -> bool {
    let Some(shell_pid) = shell_explorer_pid() else {
        return false;
    };
    let Some(class) = window_class_name(hwnd) else {
        return false;
    };
    if windows_build >= WIN11_22H2_BUILD {
        class == "XamlExplorerHostIslandWindow"
            && score_win11_osd_candidate(hwnd, shell_pid).is_some()
    } else {
        class == "NativeHWNDHost" && is_win10_osd_candidate(hwnd, shell_pid)
    }
}

fn score_win11_osd_candidate(hwnd: HWND, shell_pid: u32) -> Option<u8> {
    let (pid, tid) = window_pid_and_tid(hwnd);
    if pid != shell_pid {
        return None;
    }
    // Must host a DesktopWindowXamlSource bridge and InputSite child.
    let bridge = unsafe {
        FindWindowExW(
            hwnd,
            HWND::default(),
            w!("Windows.UI.Composition.DesktopWindowContentBridge"),
            w!("DesktopWindowXamlSource"),
        )
    }
    .ok()?;
    if bridge.0.is_null() {
        return None;
    }
    let input_site = unsafe {
        FindWindowExW(
            bridge,
            HWND::default(),
            w!("Windows.UI.Input.InputSite.WindowClass"),
            PCWSTR::null(),
        )
    }
    .ok()?;
    if input_site.0.is_null() {
        return None;
    }

    // Never touch Task View or Snap Layouts flyouts.
    if let Some(desc) = thread_description(tid)
        && is_known_non_osd_thread_description(&desc)
    {
        return None;
    }

    // Reject desktop-band windows; the Hardware Confirmator lives in a higher shell band.
    let band = window_band(hwnd);
    if band == Some(ZBID_DESKTOP) {
        return None;
    }

    // Must be OSD-sized (not full-screen Task View and not a 0x0 dormant island).
    let mut rect = RECT::default();
    if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() || !is_osd_sized_rect(&rect) {
        return None;
    }

    let mut score = 1u8;
    if band == Some(ZBID_ABOVELOCK_UX) {
        score += 4;
    }
    if unsafe { IsWindowVisible(hwnd).as_bool() } {
        score += 2;
    }
    let mut input_rect = RECT::default();
    if unsafe { GetWindowRect(input_site, &mut input_rect) }.is_ok()
        && !(input_rect.left == 0
            && input_rect.top == 0
            && input_rect.right == 0
            && input_rect.bottom == 0)
    {
        score += 1;
    }
    Some(score)
}

fn is_win10_osd_candidate(hwnd: HWND, shell_pid: u32) -> bool {
    let (pid, _) = window_pid_and_tid(hwnd);
    if pid != shell_pid {
        return false;
    }
    let Ok(dui) =
        (unsafe { FindWindowExW(hwnd, HWND::default(), w!("DirectUIHWND"), PCWSTR::null()) })
    else {
        return false;
    };
    !dui.0.is_null()
}

fn window_band(hwnd: HWND) -> Option<u32> {
    type GetWindowBandFn = unsafe extern "system" fn(HWND, *mut u32) -> BOOL;
    let module = unsafe { GetModuleHandleW(w!("user32.dll")) }.ok()?;
    let proc = unsafe { GetProcAddress(module, s!("GetWindowBand")) }?;
    let func: GetWindowBandFn = unsafe { std::mem::transmute(proc) };
    let mut band = 0u32;
    unsafe { func(hwnd, &mut band).as_bool().then_some(band) }
}

fn thread_description(tid: u32) -> Option<String> {
    type GetThreadDescriptionFn = unsafe extern "system" fn(HANDLE, *mut PWSTR) -> HRESULT;
    if tid == 0 {
        return None;
    }
    let module = unsafe { GetModuleHandleW(w!("kernelbase.dll")) }.ok()?;
    let proc = unsafe { GetProcAddress(module, s!("GetThreadDescription")) }?;
    let func: GetThreadDescriptionFn = unsafe { std::mem::transmute(proc) };
    let thread = unsafe { OpenThread(THREAD_QUERY_LIMITED_INFORMATION, false, tid) }.ok()?;
    let mut raw = PWSTR::null();
    let hr = unsafe { func(thread, &mut raw) };
    unsafe {
        let _ = CloseHandle(thread);
    }
    if hr.is_err() || raw.0.is_null() {
        return None;
    }
    let text = unsafe {
        let mut len = 0usize;
        while *raw.0.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(raw.0, len);
        let value = String::from_utf16_lossy(slice);
        let _ = LocalFree(HLOCAL(raw.0 as *mut c_void));
        value
    };
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_appcommand(cmd: u16, device: u16) -> isize {
        ((usize::from(cmd | device)) << 16) as isize
    }

    #[test]
    fn decodes_bluetooth_oem_and_keyboard_appcommands() {
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_VOLUME_UP, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Volume)
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_VOLUME_DOWN, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Volume)
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_VOLUME_MUTE, FAPPCOMMAND_KEY)),
            Some(BtMediaEvent::Volume)
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_MEDIA_PLAY_PAUSE, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Transport(MediaKey::PlayPause))
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_MEDIA_PLAY, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Transport(MediaKey::Play))
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_MEDIA_PAUSE, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Transport(MediaKey::Pause))
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_MEDIA_NEXTTRACK, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Transport(MediaKey::Next))
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(
                APPCOMMAND_MEDIA_PREVIOUSTRACK,
                FAPPCOMMAND_OEM
            )),
            Some(BtMediaEvent::Transport(MediaKey::Previous))
        );
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_MEDIA_STOP, FAPPCOMMAND_OEM)),
            Some(BtMediaEvent::Transport(MediaKey::Stop))
        );
    }

    #[test]
    fn ignores_mouse_appcommands_and_unrelated_commands() {
        const FAPPCOMMAND_MOUSE: u16 = 0x8000;
        assert_eq!(
            decode_shell_appcommand(pack_appcommand(APPCOMMAND_VOLUME_UP, FAPPCOMMAND_MOUSE)),
            None
        );
        assert_eq!(decode_shell_appcommand(pack_appcommand(1, FAPPCOMMAND_OEM)), None);
    }

    #[test]
    fn decodes_hid_consumer_usages() {
        assert_eq!(decode_consumer_usage(CONSUMER_USAGE_VOLUME_INC), Some(BtMediaEvent::Volume));
        assert_eq!(decode_consumer_usage(CONSUMER_USAGE_VOLUME_DEC), Some(BtMediaEvent::Volume));
        assert_eq!(decode_consumer_usage(CONSUMER_USAGE_MUTE), Some(BtMediaEvent::Volume));
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_PLAY_PAUSE),
            Some(BtMediaEvent::Transport(MediaKey::PlayPause))
        );
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_PLAY),
            Some(BtMediaEvent::Transport(MediaKey::Play))
        );
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_PAUSE),
            Some(BtMediaEvent::Transport(MediaKey::Pause))
        );
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_SCAN_NEXT),
            Some(BtMediaEvent::Transport(MediaKey::Next))
        );
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_SCAN_PREVIOUS),
            Some(BtMediaEvent::Transport(MediaKey::Previous))
        );
        assert_eq!(
            decode_consumer_usage(CONSUMER_USAGE_STOP),
            Some(BtMediaEvent::Transport(MediaKey::Stop))
        );
        assert_eq!(decode_consumer_usage(0x0040), None);
    }

    #[test]
    fn rejects_known_non_osd_flyouts_and_fullscreen_rects() {
        assert!(is_known_non_osd_thread_description("MultitaskingView"));
        assert!(is_known_non_osd_thread_description("SnapLayoutFlyout"));
        assert!(is_known_non_osd_thread_description("TaskViewHost"));
        assert!(!is_known_non_osd_thread_description("HardwareConfirmator"));
        assert!(!is_known_non_osd_thread_description(""));

        assert!(is_osd_sized_rect(&RECT {
            left: 800,
            top: 900,
            right: 1040,
            bottom: 960
        }));
        assert!(!is_osd_sized_rect(&RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080
        }));
        assert!(!is_osd_sized_rect(&RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0
        }));
    }
}
