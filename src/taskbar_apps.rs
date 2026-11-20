//! Mouse wheel over an app's taskbar button changes that app's volume.
//!
//! The low-level mouse hook calls [`hover`] and [`wheel`] only. Both read
//! caches and post to the worker; neither blocks.
//!
//! The worker reads the Windows 11 taskbar buttons with UI Automation, and
//! only while the pointer is over a taskbar, so explorer is never polled in the
//! background. Each button names either one window (`Window: 0x<hwnd>`) or an
//! app (`Appid: <AppUserModelID or exe path>`). Buttons are kept with the
//! session keys (lowercase exe paths, see [`crate::volume_sessions`]) of the
//! apps that are playing sound right now. Chrome, Discord and other apps that
//! play from a child process share the exe path, so they resolve too.
//!
//! Only buttons with a live session take the wheel. Over any other button it
//! falls through to the normal wheel zones, or to the taskbar itself.

use std::{
    path::PathBuf,
    sync::{
        OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    time::{Duration, Instant},
};

use once_cell::sync::Lazy;
use windows::{
    Win32::{
        Foundation::{BOOL, CloseHandle, HWND, LPARAM, POINT, RECT},
        Storage::Packaging::Appx::GetApplicationUserModelId,
        System::{
            Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, StructuredStorage::PropVariantToBSTR,
            },
            Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
        Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MonitorFromPoint},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            Accessibility::{
                CUIAutomation, IUIAutomation, IUIAutomationCondition, TreeScope_Descendants,
                UIA_ClassNamePropertyId,
            },
            Shell::{
                KF_FLAG_DEFAULT, PropertiesSystem::{IPropertyStore, PROPERTYKEY, SHGetPropertyStoreForWindow},
                SHGetKnownFolderPath,
            },
            WindowsAndMessaging::{
                EnumChildWindows, EnumWindows, FindWindowExW, GetWindowThreadProcessId,
                IsWindowVisible,
            },
        },
    },
    core::{GUID, PWSTR, VARIANT, w},
};

use crate::{volume_sessions, wheel_volume::WheelAccumulator};

/// UIA class of a Windows 11 taskbar app button.
const BUTTON_CLASS: &str = "Taskbar.TaskListButtonAutomationPeer";

/// A hover rescans the buttons at most this often.
const SCAN_INTERVAL: Duration = Duration::from_millis(700);

/// Taskbar rects (auto-hide, a new monitor) are re-read this often.
const TASKBAR_INTERVAL: Duration = Duration::from_secs(2);

/// One window preview over a taskbar button, in logical pixels, with the
/// room the card keeps from it. Measured on Windows 11 24H2 at 125%.
const PREVIEW_WIDTH: f32 = 220.0;
const PREVIEW_HEIGHT: f32 = 230.0;
const PREVIEW_MARGIN: f32 = 12.0;

/// `PKEY_AppUserModel_ID`; its own module would need another windows feature.
const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
    pid: 5,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl Rect {
    fn from_raw(rect: RECT) -> Option<Self> {
        (rect.right > rect.left && rect.bottom > rect.top).then_some(Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
    }

    fn contains(self, pt: POINT) -> bool {
        pt.x >= self.left && pt.x < self.right && pt.y >= self.top && pt.y < self.bottom
    }
}

struct Button {
    rect: Rect,
    /// Session keys of the apps behind the button that are playing now.
    keys: Vec<String>,
    /// Window previews shown side by side over the button on hover.
    previews: u32,
}

enum Msg {
    /// The pointer is over a taskbar: re-read the buttons.
    Scan,
    /// Re-read the taskbar rects now (just enabled).
    Taskbars,
    Wheel { pt: POINT, delta: i16 },
}

static ENABLED: AtomicBool = AtomicBool::new(false);
static STEP: AtomicU8 = AtomicU8::new(2);
static SCAN_QUEUED: AtomicBool = AtomicBool::new(false);
static LAST_SCAN_MS: AtomicU64 = AtomicU64::new(0);
static CLOCK: Lazy<Instant> = Lazy::new(Instant::now);
static TASKBARS: Lazy<RwLock<Vec<Rect>>> = Lazy::new(|| RwLock::new(Vec::new()));
static BUTTONS: Lazy<RwLock<Vec<Button>>> = Lazy::new(|| RwLock::new(Vec::new()));
static WORKER: OnceLock<Sender<Msg>> = OnceLock::new();

fn worker() -> &'static Sender<Msg> {
    WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("taskbar-apps".into())
            .spawn(move || run(receiver));
        sender
    })
}

fn now_ms() -> u64 {
    CLOCK.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

/// Turn the feature on or off and set the notch size. Any thread.
pub fn configure(enabled: bool, step_percent: u8) {
    STEP.store(step_percent.clamp(1, 100), Ordering::Relaxed);
    let was = ENABLED.swap(enabled, Ordering::Relaxed);
    if enabled && !was {
        let _ = worker().send(Msg::Taskbars);
    } else if !enabled && was {
        if let Ok(mut buttons) = BUTTONS.write() {
            buttons.clear();
        }
    }
}

/// Mouse moved. Hook thread: cache reads and a channel send only.
pub fn hover(pt: POINT) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let Ok(taskbars) = TASKBARS.try_read() else {
        return;
    };
    if !taskbars.iter().any(|rect| rect.contains(pt)) {
        return;
    }
    drop(taskbars);
    if now_ms().saturating_sub(LAST_SCAN_MS.load(Ordering::Relaxed))
        < SCAN_INTERVAL.as_millis() as u64
    {
        return;
    }
    if SCAN_QUEUED.swap(true, Ordering::Relaxed) {
        return;
    }
    if worker().send(Msg::Scan).is_err() {
        SCAN_QUEUED.store(false, Ordering::Relaxed);
    }
}

/// `true` when the wheel at `pt` is over a button of an app playing sound;
/// the worker then changes that app's volume. Hook thread, never blocks.
pub fn wheel(pt: POINT, delta: i16) -> bool {
    if !ENABLED.load(Ordering::Relaxed) {
        return false;
    }
    let Ok(buttons) = BUTTONS.try_read() else {
        return false;
    };
    let hit = buttons
        .iter()
        .any(|button| !button.keys.is_empty() && button.rect.contains(pt));
    drop(buttons);
    hit && worker().send(Msg::Wheel { pt, delta }).is_ok()
}

fn run(receiver: mpsc::Receiver<Msg>) {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let automation: Option<IUIAutomation> =
        unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok();
    let condition = automation.as_ref().and_then(|automation| unsafe {
        automation
            .CreatePropertyCondition(UIA_ClassNamePropertyId, &VARIANT::from(BUTTON_CLASS))
            .ok()
    });
    let mut accumulator = WheelAccumulator::new();
    let mut last_keys: Vec<String> = Vec::new();
    loop {
        match receiver.recv_timeout(TASKBAR_INTERVAL) {
            Ok(Msg::Scan) => {
                if ENABLED.load(Ordering::Relaxed)
                    && let (Some(automation), Some(condition)) = (&automation, &condition)
                {
                    scan(automation, condition);
                }
                LAST_SCAN_MS.store(now_ms(), Ordering::Relaxed);
                SCAN_QUEUED.store(false, Ordering::Relaxed);
            }
            Ok(Msg::Taskbars) | Err(RecvTimeoutError::Timeout) => {
                if ENABLED.load(Ordering::Relaxed) {
                    refresh_taskbars();
                }
            }
            Ok(Msg::Wheel { pt, delta }) => {
                let (keys, preview) = BUTTONS
                    .read()
                    .ok()
                    .and_then(|buttons| {
                        buttons
                            .iter()
                            .find(|button| button.rect.contains(pt))
                            .map(|button| (button.keys.clone(), preview_zone(button)))
                    })
                    .unwrap_or_default();
                if keys.is_empty() {
                    continue;
                }
                if keys != last_keys {
                    accumulator = WheelAccumulator::new();
                    last_keys = keys.clone();
                }
                let steps = accumulator.feed(delta);
                if steps == 0 {
                    continue;
                }
                let step = STEP.load(Ordering::Relaxed);
                if let Some(app) = volume_sessions::step_app_volume(&keys, steps, step) {
                    crate::show_app_volume_osd(app, preview);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn taskbar_windows() -> Vec<HWND> {
    let mut out = Vec::new();
    unsafe {
        if let Ok(primary) = FindWindowExW(None, None, w!("Shell_TrayWnd"), None)
            && !primary.0.is_null()
        {
            out.push(primary);
        }
        let mut after = HWND::default();
        while let Ok(next) = FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), None) {
            if next.0.is_null() {
                break;
            }
            out.push(next);
            after = next;
        }
    }
    out
}

fn refresh_taskbars() {
    let rects: Vec<Rect> = taskbar_windows()
        .into_iter()
        .filter(|hwnd| unsafe { IsWindowVisible(*hwnd) }.as_bool())
        .filter_map(|hwnd| {
            let mut rect = RECT::default();
            unsafe { windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut rect) }
                .ok()?;
            Rect::from_raw(rect)
        })
        .collect();
    if let Ok(mut taskbars) = TASKBARS.write() {
        *taskbars = rects;
    }
}

/// Lazily read once per scan: only `Appid:` buttons need the window list.
struct Windows(Option<Vec<(HWND, u32)>>);

impl Windows {
    fn get(&mut self) -> &[(HWND, u32)] {
        self.0.get_or_insert_with(top_level_windows)
    }
}

fn scan(automation: &IUIAutomation, condition: &IUIAutomationCondition) {
    let live: Vec<(String, String)> = volume_sessions::app_volumes()
        .into_iter()
        .filter(|app| !app.system)
        .map(|app| (app.key, app.name))
        .collect();
    let mut windows = Windows(None);
    let mut buttons = Vec::new();
    for taskbar in taskbar_windows() {
        let Ok(root) = (unsafe { automation.ElementFromHandle(taskbar) }) else {
            continue;
        };
        let Ok(found) = (unsafe { root.FindAll(TreeScope_Descendants, condition) }) else {
            continue;
        };
        let count = unsafe { found.Length() }.unwrap_or(0);
        for index in 0..count {
            let Ok(element) = (unsafe { found.GetElement(index) }) else {
                continue;
            };
            let Some(rect) = unsafe { element.CurrentBoundingRectangle() }
                .ok()
                .and_then(Rect::from_raw)
            else {
                continue;
            };
            let id = unsafe { element.CurrentAutomationId() }
                .map(|id| id.to_string())
                .unwrap_or_default();
            let name = unsafe { element.CurrentName() }
                .map(|name| name.to_string())
                .unwrap_or_default();
            let keys = if live.is_empty() {
                Vec::new()
            } else {
                button_keys(&id, &name, &live, &mut windows)
            };
            let previews = running_windows(&name).clamp(1, 8);
            buttons.push(Button {
                rect,
                keys,
                previews,
            });
        }
    }
    if let Ok(mut cache) = BUTTONS.write() {
        *cache = buttons;
    }
}

/// Session keys of the playing apps behind one button.
fn button_keys(
    id: &str,
    name: &str,
    live: &[(String, String)],
    windows: &mut Windows,
) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    let by_id = if let Some(hex) = id.strip_prefix("Window: ") {
        let raw = hex.trim().trim_start_matches("0x").trim_start_matches("0X");
        if let Ok(value) = isize::from_str_radix(raw, 16) {
            paths.extend(window_exe(HWND(value as *mut _)));
        }
        true
    } else if let Some(app_id) = id.strip_prefix("Appid: ") {
        let app_id = app_id.trim();
        if app_id.contains('\\') {
            paths.extend(expand_known_folder(app_id));
        } else {
            for &(hwnd, pid) in windows.get() {
                let matches = window_app_id(hwnd)
                    .or_else(|| package_app_id(pid))
                    .is_some_and(|window_id| window_id.eq_ignore_ascii_case(app_id));
                if matches {
                    paths.extend(window_exe(hwnd));
                }
            }
        }
        false
    } else {
        return Vec::new();
    };
    let mut keys: Vec<String> = live
        .iter()
        .filter(|(key, _)| paths.iter().any(|path| path == key))
        .map(|(key, _)| key.clone())
        .collect();
    // An app that sets its ID per process (Electron apps such as Discord)
    // cannot be read from outside. Its grouped or pinned button still carries
    // the app's name: "Discord - 1 running window", "Discord pinned".
    if keys.is_empty() && !by_id {
        let label = button_app_name(name).to_lowercase();
        keys.extend(
            live.iter()
                .filter(|(_, app)| app.to_lowercase() == label)
                .map(|(key, _)| key.clone()),
        );
    }
    keys.dedup();
    keys
}

/// "Discord - 3 running windows" -> 3; a single window's button says 1.
fn running_windows(name: &str) -> u32 {
    let Some(at) = name.rfind(" - ") else {
        return 1;
    };
    let tail = &name[at + 3..];
    if !tail.contains("running window") {
        return 1;
    }
    tail.split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or(1)
}

/// Where Windows draws the hover previews for `button`, with a margin:
/// centered over it, just above the taskbar, one preview per window.
/// Explorer draws them in its own composition layer, not as a window, so
/// there is no rectangle to ask for.
fn preview_zone(button: &Button) -> Option<RECT> {
    let rect = button.rect;
    let center = POINT {
        x: rect.left + (rect.right - rect.left) / 2,
        y: rect.top + (rect.bottom - rect.top) / 2,
    };
    let scale = monitor_scale(center);
    let half = (PREVIEW_WIDTH * button.previews as f32 / 2.0 + PREVIEW_MARGIN) * scale;
    let height = (PREVIEW_HEIGHT + PREVIEW_MARGIN) * scale;
    Some(RECT {
        left: center.x - half.round() as i32,
        top: rect.top - height.round() as i32,
        right: center.x + half.round() as i32,
        bottom: rect.bottom,
    })
}

fn monitor_scale(pt: POINT) -> f32 {
    let monitor = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.is_err() {
        return 1.0;
    }
    dpi_x.max(96) as f32 / 96.0
}

fn button_app_name(name: &str) -> &str {
    let name = name.trim();
    if let Some(stripped) = name.strip_suffix(" pinned") {
        return stripped.trim();
    }
    match name.rfind(" - ") {
        Some(at) if name[at + 3..].contains("running window") => name[..at].trim(),
        _ => name,
    }
}

/// `{FOLDERID}\rest\app.exe` (or a plain path) as a lowercase exe path.
fn expand_known_folder(app_id: &str) -> Option<String> {
    let path = if let Some(rest) = app_id.strip_prefix('{') {
        let (guid, tail) = rest.split_once('}')?;
        let guid = GUID::from_u128(u128::from_str_radix(&guid.replace('-', ""), 16).ok()?);
        let folder = unsafe { SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None) }.ok()?;
        let base = unsafe { folder.to_string() }.unwrap_or_default();
        unsafe { CoTaskMemFree(Some(folder.0 as _)) };
        if base.is_empty() {
            return None;
        }
        format!("{base}{tail}")
    } else {
        app_id.to_string()
    };
    Some(path.to_lowercase())
}

/// Exe of the process that owns `hwnd`. A UWP frame's app lives in a child
/// window of another process.
fn window_exe(hwnd: HWND) -> Option<String> {
    let pid = window_pid(hwnd)?;
    let path = crate::process_image_path(pid)?;
    let is_frame = path
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("ApplicationFrameHost.exe"));
    let path = if is_frame {
        child_of_other_process(hwnd, pid).and_then(crate::process_image_path)?
    } else {
        path
    };
    Some(path_key(&path))
}

fn path_key(path: &PathBuf) -> String {
    path.to_string_lossy().to_lowercase()
}

fn window_pid(hwnd: HWND) -> Option<u32> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

fn child_of_other_process(parent: HWND, parent_pid: u32) -> Option<u32> {
    let mut found: (u32, Option<u32>) = (parent_pid, None);
    unsafe extern "system" fn visit(child: HWND, lparam: LPARAM) -> BOOL {
        let found = unsafe { &mut *(lparam.0 as *mut (u32, Option<u32>)) };
        if let Some(pid) = window_pid(child)
            && pid != found.0
        {
            found.1 = Some(pid);
            return false.into();
        }
        true.into()
    }
    unsafe {
        let _ = EnumChildWindows(parent, Some(visit), LPARAM(&mut found as *mut _ as isize));
    }
    found.1
}

fn top_level_windows() -> Vec<(HWND, u32)> {
    let mut out: Vec<(HWND, u32)> = Vec::new();
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let out = unsafe { &mut *(lparam.0 as *mut Vec<(HWND, u32)>) };
        if unsafe { IsWindowVisible(hwnd) }.as_bool()
            && let Some(pid) = window_pid(hwnd)
        {
            out.push((hwnd, pid));
        }
        true.into()
    }
    unsafe {
        let _ = EnumWindows(Some(visit), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

/// The window's own AppUserModelID (Chrome profiles, most shell-aware apps).
fn window_app_id(hwnd: HWND) -> Option<String> {
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let value = store.GetValue(&PKEY_APP_USER_MODEL_ID).ok()?;
        let text = PropVariantToBSTR(&value).ok()?.to_string();
        (!text.is_empty()).then_some(text)
    }
}

/// A packaged app's AppUserModelID ("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App").
fn package_app_id(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 256];
        let mut len = buffer.len() as u32;
        let result = GetApplicationUserModelId(process, &mut len, PWSTR(buffer.as_mut_ptr()));
        let _ = CloseHandle(process);
        if result.0 != 0 {
            return None;
        }
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        let text = String::from_utf16_lossy(&buffer[..end]);
        (!text.is_empty()).then_some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{button_app_name, running_windows};

    #[test]
    fn window_count_from_button_label() {
        assert_eq!(running_windows("Discord - 3 running windows"), 3);
        assert_eq!(running_windows("Reddit - Google Chrome - 1 running window"), 1);
        assert_eq!(running_windows("Discord pinned"), 1);
    }

    #[test]
    fn app_name_from_button_label() {
        assert_eq!(button_app_name("Discord pinned"), "Discord");
        assert_eq!(button_app_name("Discord - 1 running window"), "Discord");
        assert_eq!(button_app_name("Telegram - 3 running windows"), "Telegram");
        assert_eq!(button_app_name("Spotify"), "Spotify");
    }
}
