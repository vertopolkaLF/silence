//! Hit-testing and notch accumulation for mouse-wheel output volume.
//!
//! The low-level mouse hook may call [`decode_wheel`] and [`WheelTargets::hit`]
//! only. Everything else talks to the shell or to the window manager and has
//! to run on the tray UI thread (the thread that owns the hook and the message
//! loop). Do not call [`WheelTargets::refresh`] or [`fullscreen_blocks_wheel`]
//! from `mouse_proc`: `Shell_NotifyIconGetRect` and
//! `SHQueryUserNotificationState` are synchronous cross-process calls.
//!
//! # Wiring
//!
//! Refresh while the selected zone is not [`WheelZone::Off`]:
//! - a UI timer every [`REFRESH_INTERVAL`] (1s). The existing 250ms state
//!   timer is too chatty for `EnumWindows`; divide it, or use a dedicated timer.
//! - `WM_DISPLAYCHANGE`
//! - `WM_DPICHANGED`, if the hidden window receives it
//! - the registered `TaskbarCreated` message (`TASKBAR_CREATED_MESSAGE`,
//!   already handled in `main_wnd_proc` — refresh after the icon is re-added)
//! - after the volume tray icon is added or moved (`NIM_ADD` / `NIM_MODIFY`)
//!
//! Do not refresh from the hook when the cursor enters the taskbar. An
//! auto-hide bar can therefore be up to one refresh interval late.
//!
//! Suggested hook body (same thread, no shell calls):
//!
//! ```text
//! note Instant::now() for should_reinstall_hook (mouse AND keyboard hooks)
//! if let Some((pt, delta, injected)) = decode_wheel(message, lparam) {
//!     if !injected && targets.hit(pt, zone) {
//!         PostMessage the point and delta to the hidden window
//!         return LRESULT(1) so the window under the cursor does not also scroll
//!     }
//! }
//! CallNextHookEx
//! ```
//!
//! On that posted message: if `fullscreen_blocks_wheel(pt)` then ignore,
//! otherwise `accumulator.feed(delta)` and apply that many volume notches.
//!
//! Coordinates are physical screen pixels. The process is already per-monitor
//! DPI aware (`SetProcessDpiAwarenessContext` V2); keep it that way or the
//! hook point and these rects will disagree.
//!
//! # What this machine actually uses (Windows 11 24H2, build 26100.8328)
//!
//! `Shell_TrayWnd` is still the taskbar. Its child `TrayNotifyWnd` is still
//! the clock + notification icons (no `SysPager` / `ToolbarWindow32` /
//! `TrayClockWClass` children on this build). `Windows.UI.Composition.DesktopWindowContentBridge`
//! covers the whole bar and `Windows.UI.Input.InputSite.WindowClass` had an
//! empty rect, so neither is the notification area. The overflow flyout is
//! `TopLevelWindowForOverflowXamlIsland` ("System tray overflow window."),
//! not the Win10 `NotifyIconOverflowWindow`. While closed it stays alive,
//! uncloaked, with a real on-screen rect, and `IsWindowVisible` is false —
//! visibility is the open signal, not the rect and not the cloak flag.
//! `Shell_NotifyIconGetRect` for `SilenceV2Hidden` uID 1 returned `S_OK` and
//! a rect inside `TrayNotifyWnd`.
//!
//! # Hook health
//!
//! Windows deletes a `WH_*_LL` hook without notice when the callback exceeds
//! `LowLevelHooksTimeout` (`HKCU\Control Panel\Desktop`, milliseconds; 300 if
//! the value is missing). [`should_reinstall_hook`] is the check for the UI
//! timer: pass the newest callback time from every low-level hook you
//! installed. Keyboard input updates `GetLastInputInfo` too, so a mouse-only
//! timestamp looks like a dead hook while the user is typing.

#![allow(dead_code)]

use std::mem::size_of;
use std::time::{Duration, Instant};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, S_FALSE},
        Graphics::{
            Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
            Gdi::{
                GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
            },
        },
        System::SystemInformation::GetTickCount,
        UI::{
            Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO},
            Shell::{
                NOTIFYICONIDENTIFIER, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN,
                SHQueryUserNotificationState,
            },
            WindowsAndMessaging::{
                EnumWindows, FindWindowExW, GetClassNameW, GetForegroundWindow, GetWindowRect,
                IsIconic, IsWindowVisible, IsZoomed, LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED,
                MSLLHOOKSTRUCT, WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WNDENUMPROC,
            },
        },
    },
    core::{GUID, HRESULT, PCWSTR, w},
};

#[link(name = "shell32")]
unsafe extern "system" {
    /// Raw call so `S_FALSE` (icon hidden in the overflow) stays visible.
    /// The `windows` 0.58 wrapper maps every success HRESULT, including
    /// `S_FALSE`, to `Ok(RECT)`.
    fn Shell_NotifyIconGetRect(
        identifier: *const NOTIFYICONIDENTIFIER,
        icon_location: *mut RECT,
    ) -> HRESULT;
}

/// UI-thread refresh period while the zone is not [`WheelZone::Off`].
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// One Windows wheel notch. High-resolution mice report smaller deltas.
pub const WHEEL_NOTCH: i32 = 120;

/// Drop a partial notch after the wheel has been still this long.
pub const WHEEL_IDLE_RESET: Duration = Duration::from_millis(300);

/// How long the hook may be silent, past the user's last input, before the
/// UI timer should install it again. See [`should_reinstall_hook`].
pub const HOOK_REINSTALL_GRACE: Duration = Duration::from_secs(3);

/// A window this many pixels smaller than the monitor still counts as covering it.
const FULLSCREEN_SLOP: i32 = 8;

/// Where a wheel event is allowed to change the output volume.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WheelZone {
    #[default]
    Off,
    /// The silence! (or dedicated volume) notify icon, including inside an open overflow flyout.
    TrayIcon,
    /// `TrayNotifyWnd` on each taskbar, plus the overflow flyout while it is open.
    NotificationArea,
    /// `Shell_TrayWnd` and every `Shell_SecondaryTrayWnd`. Not the flyout.
    Taskbar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScreenRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

impl ScreenRect {
    fn from_raw(rect: RECT) -> Option<Self> {
        let value = Self {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        };
        (!value.is_empty()).then_some(value)
    }

    fn is_empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }

    /// Half-open, matching a Win32 `RECT`.
    fn contains(self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }

    fn center(self) -> (i32, i32) {
        (
            self.left + (self.right - self.left) / 2,
            self.top + (self.bottom - self.top) / 2,
        )
    }
}

/// Cached taskbar / notification / icon rectangles.
///
/// [`Self::hit`] only reads this cache. The hook must not refresh it.
#[derive(Clone, Debug, Default)]
pub struct WheelTargets {
    taskbars: Vec<ScreenRect>,
    notify_areas: Vec<ScreenRect>,
    /// Overflow flyout rects that are actually on screen right now.
    overflow_open: Vec<ScreenRect>,
    /// Flyout rects even while the window is hidden. Win11 keeps a real rect
    /// for `TopLevelWindowForOverflowXamlIsland` after the flyout closes.
    overflow_bounds: Vec<ScreenRect>,
    icon: Option<ScreenRect>,
    icon_hidden_in_overflow: bool,
}

impl WheelTargets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuild the cache. UI thread only.
    ///
    /// `icon_hwnd` / `icon_uid` identify the notify icon (`SilenceV2Hidden`,
    /// uID 1 today; the volume icon will pass its own uID). A null hwnd skips
    /// the icon query.
    pub fn refresh(&mut self, icon_hwnd: HWND, icon_uid: u32) {
        let mut next = Self::default();
        next.scan_shell_windows();
        next.read_icon(icon_hwnd, icon_uid);
        *self = next;
    }

    /// `true` when `pt` is inside `zone`. Rect tests only.
    pub fn hit(&self, pt: POINT, zone: WheelZone) -> bool {
        match zone {
            WheelZone::Off => false,
            WheelZone::TrayIcon => self.icon.is_some_and(|rect| rect.contains(pt.x, pt.y)),
            WheelZone::NotificationArea => self
                .notify_areas
                .iter()
                .chain(self.overflow_open.iter())
                .any(|rect| rect.contains(pt.x, pt.y)),
            WheelZone::Taskbar => self.taskbars.iter().any(|rect| rect.contains(pt.x, pt.y)),
        }
    }

    /// The icon is not on the taskbar notification area.
    ///
    /// True both when the flyout is closed (`S_FALSE` or an empty rect) and
    /// when it is open and the icon rect lies inside it. The settings UI can
    /// warn that "tray icon" will miss until the icon is dragged out, or until
    /// the flyout happens to be open. A failed lookup (icon not created yet)
    /// stays `false`.
    pub fn icon_hidden_in_overflow(&self) -> bool {
        self.icon_hidden_in_overflow
    }

    fn scan_shell_windows(&mut self) {
        let mut hwnds = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(push_hwnd),
                LPARAM(&mut hwnds as *mut Vec<HWND> as isize),
            );
        }
        for hwnd in hwnds {
            let class = class_name(hwnd);
            if is_taskbar_class(&class) {
                self.push_taskbar(hwnd);
            } else if is_overflow_class(&class) {
                self.push_overflow(hwnd);
            }
        }
    }

    fn push_taskbar(&mut self, hwnd: HWND) {
        if !is_visible(hwnd) {
            return;
        }
        if let Some(rect) = window_rect(hwnd) {
            self.taskbars.push(rect);
        }
        let Some(notify) = notify_child(hwnd) else {
            return;
        };
        if is_visible(notify) {
            if let Some(rect) = window_rect(notify) {
                self.notify_areas.push(rect);
            }
        }
    }

    fn push_overflow(&mut self, hwnd: HWND) {
        let Some(rect) = window_rect(hwnd) else {
            return;
        };
        // Closed Win11 flyout: visible=false, cloaked=0, rect still on screen.
        self.overflow_bounds.push(rect);
        if is_visible(hwnd) && !window_cloaked(hwnd) {
            self.overflow_open.push(rect);
        }
    }

    fn read_icon(&mut self, hwnd: HWND, uid: u32) {
        if hwnd.0.is_null() {
            return;
        }
        match notify_icon_rect(hwnd, uid) {
            IconQuery::Missing => {}
            IconQuery::Hidden => self.icon_hidden_in_overflow = true,
            IconQuery::Rect(rect) => {
                let (icon, hidden) = place_icon(
                    &self.taskbars,
                    &self.overflow_bounds,
                    &self.overflow_open,
                    rect,
                );
                self.icon = icon;
                self.icon_hidden_in_overflow = hidden;
            }
        }
    }
}

/// Whole-notch accumulator. Partial movement is kept until the next notch,
/// a direction change, or [`WHEEL_IDLE_RESET`] of silence.
#[derive(Clone, Debug, Default)]
pub struct WheelAccumulator {
    remainder: i32,
    last_event: Option<Instant>,
}

impl WheelAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Notches to apply now. Positive is wheel-up. Leftover stays in the accumulator.
    pub fn feed(&mut self, delta: i16) -> i32 {
        self.feed_at(delta, Instant::now())
    }

    /// [`Self::feed`] at a chosen time, so tests and a posted message can share one clock.
    pub fn feed_at(&mut self, delta: i16, now: Instant) -> i32 {
        if delta == 0 {
            return 0;
        }
        let idle = self
            .last_event
            .map(|then| now.saturating_duration_since(then))
            .unwrap_or(Duration::ZERO);
        let direction_changed = self.remainder != 0 && (self.remainder > 0) != (delta > 0);
        if direction_changed || idle >= WHEEL_IDLE_RESET {
            self.remainder = 0;
        }
        self.remainder = self.remainder.saturating_add(i32::from(delta));
        self.last_event = Some(now);
        let steps = self.remainder / WHEEL_NOTCH;
        self.remainder %= WHEEL_NOTCH;
        steps
    }
}

/// `WM_MOUSEWHEEL` from an `MSLLHOOKSTRUCT`. `WM_MOUSEHWHEEL` and every other
/// message are `None`.
///
/// The message is required: the hook struct's `mouseData` high word is the
/// delta for both wheel axes, so `lparam` alone cannot tell them apart.
///
/// `injected` is set for `LLMHF_INJECTED` and `LLMHF_LOWER_IL_INJECTED`.
/// Ignore those if applying a notch would synthesize another wheel.
pub fn decode_wheel(message: u32, lparam: LPARAM) -> Option<(POINT, i16, bool)> {
    // Same `mouseData` slot as the horizontal wheel; that message is not volume.
    if message == WM_MOUSEHWHEEL || message != WM_MOUSEWHEEL || lparam.0 == 0 {
        return None;
    }
    let hook = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    let delta = (hook.mouseData >> 16) as i16;
    let injected = hook.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0;
    Some((hook.pt, delta, injected))
}

/// The wheel should not change the app volume.
///
/// Blocks when `SHQueryUserNotificationState` reports exclusive Direct3D
/// fullscreen or presentation mode, and when the foreground window covers the
/// monitor under `pt`. Maximized windows (`IsZoomed`), cloaked windows, and
/// the desktop / taskbar / overflow shell windows do not count as covering.
///
/// Not hook-safe. Call it on the UI thread, on the posted wheel message.
pub fn fullscreen_blocks_wheel(pt: POINT) -> bool {
    if exclusive_fullscreen_or_presentation() {
        return true;
    }
    foreground_covers_monitor(pt)
}

/// `true` when an installed low-level hook has probably been dropped.
///
/// `last_event` is the last time any installed `WH_*_LL` callback ran (mouse
/// and keyboard — see the module docs). Call from the UI timer, then
/// `UnhookWindowsHookEx` + `SetWindowsHookExW` on that same thread.
///
/// Idle machines do not reinstall: the hook is only treated as dead when the
/// user has produced input more recently than the last callback, by more than
/// [`HOOK_REINSTALL_GRACE`].
pub fn should_reinstall_hook(last_event: Instant) -> bool {
    let Some(idle) = user_idle_duration() else {
        return false;
    };
    hook_needs_reinstall(last_event.elapsed(), idle)
}

fn hook_needs_reinstall(silent_for: Duration, user_idle: Duration) -> bool {
    silent_for > user_idle.saturating_add(HOOK_REINSTALL_GRACE)
}

enum IconQuery {
    Missing,
    Hidden,
    Rect(ScreenRect),
}

fn notify_icon_rect(hwnd: HWND, uid: u32) -> IconQuery {
    let identifier = NOTIFYICONIDENTIFIER {
        cbSize: size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: uid,
        guidItem: GUID::zeroed(),
    };
    let mut raw = RECT::default();
    let hr = unsafe { Shell_NotifyIconGetRect(&identifier, &mut raw) };
    if hr.is_err() {
        return IconQuery::Missing;
    }
    if hr == S_FALSE {
        return IconQuery::Hidden;
    }
    match ScreenRect::from_raw(raw) {
        Some(rect) => IconQuery::Rect(rect),
        None => IconQuery::Hidden,
    }
}

/// `(rect used by hit, hidden in the overflow)`.
///
/// A center that sits on a taskbar wins over the flyout. A center inside the
/// flyout is a hit only while that flyout is open; the hidden flag stays set
/// so the settings hint does not flicker with the popup.
fn place_icon(
    taskbars: &[ScreenRect],
    overflow_bounds: &[ScreenRect],
    overflow_open: &[ScreenRect],
    rect: ScreenRect,
) -> (Option<ScreenRect>, bool) {
    let (x, y) = rect.center();
    if contains_any(taskbars, x, y) {
        return (Some(rect), false);
    }
    if contains_any(overflow_bounds, x, y) {
        let visible = contains_any(overflow_open, x, y);
        return (visible.then_some(rect), true);
    }
    (Some(rect), false)
}

fn contains_any(rects: &[ScreenRect], x: i32, y: i32) -> bool {
    rects.iter().any(|rect| rect.contains(x, y))
}

fn is_taskbar_class(class: &str) -> bool {
    matches!(class, "Shell_TrayWnd" | "Shell_SecondaryTrayWnd")
}

fn is_overflow_class(class: &str) -> bool {
    matches!(
        class,
        "TopLevelWindowForOverflowXamlIsland" | "NotifyIconOverflowWindow"
    )
}

fn is_shell_or_desktop(class: &str) -> bool {
    matches!(
        class,
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            | "NotifyIconOverflowWindow"
            | "TopLevelWindowForOverflowXamlIsland"
            | "XamlExplorerHostIslandWindow"
    )
}

fn notify_child(parent: HWND) -> Option<HWND> {
    unsafe { FindWindowExW(parent, HWND::default(), w!("TrayNotifyWnd"), PCWSTR::null()).ok() }
}

fn exclusive_fullscreen_or_presentation() -> bool {
    let Ok(state) = (unsafe { SHQueryUserNotificationState() }) else {
        return false;
    };
    state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE
}

fn foreground_covers_monitor(pt: POINT) -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.0.is_null()
        || !is_visible(hwnd)
        || is_iconic(hwnd)
        || is_zoomed(hwnd)
        || window_cloaked(hwnd)
    {
        return false;
    }
    if is_shell_or_desktop(&class_name(hwnd)) {
        return false;
    }
    let (Some(window), Some(monitor)) = (window_rect(hwnd), monitor_rect(pt)) else {
        return false;
    };
    covers_monitor(window, monitor, FULLSCREEN_SLOP)
}

fn covers_monitor(window: ScreenRect, monitor: ScreenRect, slop: i32) -> bool {
    window.left <= monitor.left.saturating_add(slop)
        && window.top <= monitor.top.saturating_add(slop)
        && window.right >= monitor.right.saturating_sub(slop)
        && window.bottom >= monitor.bottom.saturating_sub(slop)
}

fn monitor_rect(pt: POINT) -> Option<ScreenRect> {
    let monitor: HMONITOR = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    if monitor.is_invalid() {
        return None;
    }
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info).as_bool() } {
        return None;
    }
    ScreenRect::from_raw(info.rcMonitor)
}

fn user_idle_duration() -> Option<Duration> {
    let mut info = LASTINPUTINFO {
        cbSize: size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info).as_bool() } {
        return None;
    }
    let now = unsafe { GetTickCount() };
    Some(Duration::from_millis(u64::from(
        now.wrapping_sub(info.dwTime),
    )))
}

fn window_rect(hwnd: HWND) -> Option<ScreenRect> {
    let mut raw = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut raw).ok()? };
    ScreenRect::from_raw(raw)
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) };
    if len <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buf[..len as usize])
}

fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd).as_bool() }
}

fn is_iconic(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd).as_bool() }
}

fn is_zoomed(hwnd: HWND) -> bool {
    unsafe { IsZoomed(hwnd).as_bool() }
}

fn window_cloaked(hwnd: HWND) -> bool {
    let mut value = 0u32;
    let ok = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut value as *mut u32 as *mut _,
            size_of::<u32>() as u32,
        )
        .is_ok()
    };
    ok && value != 0
}

unsafe extern "system" fn push_hwnd(
    hwnd: HWND,
    lparam: LPARAM,
) -> windows::Win32::Foundation::BOOL {
    let hwnds = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    hwnds.push(hwnd);
    windows::Win32::Foundation::BOOL(1)
}

const _: WNDENUMPROC = Some(push_hwnd);

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(left: i32, top: i32, right: i32, bottom: i32) -> ScreenRect {
        ScreenRect {
            left,
            top,
            right,
            bottom,
        }
    }

    fn pt(x: i32, y: i32) -> POINT {
        POINT { x, y }
    }

    fn at(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    /// Layout captured on this machine: 2560x1440, taskbar along the bottom,
    /// silence! icon inside `TrayNotifyWnd`, overflow flyout closed.
    fn observed_desktop() -> WheelTargets {
        let mut targets = WheelTargets::default();
        targets.taskbars.push(rect(0, 1387, 2560, 1440));
        targets.notify_areas.push(rect(1884, 1387, 2560, 1440));
        targets.overflow_bounds.push(rect(1664, 1194, 2207, 1387));
        let icon = rect(1953, 1387, 1988, 1440);
        let (placed, hidden) = place_icon(
            &targets.taskbars,
            &targets.overflow_bounds,
            &targets.overflow_open,
            icon,
        );
        targets.icon = placed;
        targets.icon_hidden_in_overflow = hidden;
        targets
    }

    #[test]
    fn observed_icon_hits_every_zone_except_off() {
        let targets = observed_desktop();
        let icon = pt(1970, 1410);
        assert!(!targets.icon_hidden_in_overflow());
        assert!(targets.hit(icon, WheelZone::TrayIcon));
        assert!(targets.hit(icon, WheelZone::NotificationArea));
        assert!(targets.hit(icon, WheelZone::Taskbar));
        assert!(!targets.hit(icon, WheelZone::Off));
    }

    #[test]
    fn zones_are_nested_and_edges_are_exclusive() {
        let targets = observed_desktop();
        let notify_only = pt(1900, 1410);
        assert!(!targets.hit(notify_only, WheelZone::TrayIcon));
        assert!(targets.hit(notify_only, WheelZone::NotificationArea));
        assert!(targets.hit(notify_only, WheelZone::Taskbar));

        let buttons = pt(400, 1410);
        assert!(!targets.hit(buttons, WheelZone::TrayIcon));
        assert!(!targets.hit(buttons, WheelZone::NotificationArea));
        assert!(targets.hit(buttons, WheelZone::Taskbar));

        assert!(!targets.hit(pt(10, 10), WheelZone::Taskbar));
        assert!(!targets.hit(pt(2560, 1410), WheelZone::Taskbar));
        assert!(!targets.hit(pt(1970, 1440), WheelZone::TrayIcon));
        assert!(targets.hit(pt(0, 1387), WheelZone::Taskbar));
    }

    #[test]
    fn closed_overflow_rect_is_not_a_hit() {
        let targets = observed_desktop();
        let stale = pt(1900, 1300);
        assert!(!targets.hit(stale, WheelZone::NotificationArea));
        assert!(!targets.hit(stale, WheelZone::Taskbar));
        assert!(!targets.hit(stale, WheelZone::TrayIcon));
    }

    #[test]
    fn open_overflow_hits_notification_area_and_a_hidden_icon() {
        let flyout = rect(1664, 1194, 2207, 1387);
        let icon = rect(1700, 1220, 1736, 1256);
        let (placed, hidden) = place_icon(&[], &[flyout], &[flyout], icon);
        let targets = WheelTargets {
            overflow_open: vec![flyout],
            overflow_bounds: vec![flyout],
            icon: placed,
            icon_hidden_in_overflow: hidden,
            ..WheelTargets::default()
        };
        assert!(hidden);
        assert!(targets.icon_hidden_in_overflow());
        assert!(targets.hit(pt(1710, 1230), WheelZone::TrayIcon));
        assert!(targets.hit(pt(1800, 1250), WheelZone::NotificationArea));
        assert!(!targets.hit(pt(1800, 1250), WheelZone::Taskbar));
    }

    #[test]
    fn closed_overflow_keeps_the_hint_and_drops_the_icon_hit() {
        let flyout = rect(1664, 1194, 2207, 1387);
        let icon = rect(1700, 1220, 1736, 1256);
        let (placed, hidden) = place_icon(&[], &[flyout], &[], icon);
        assert_eq!(placed, None);
        assert!(hidden);
        let targets = WheelTargets {
            overflow_bounds: vec![flyout],
            icon: placed,
            icon_hidden_in_overflow: hidden,
            ..WheelTargets::default()
        };
        assert!(!targets.hit(pt(1710, 1230), WheelZone::TrayIcon));
    }

    #[test]
    fn taskbar_wins_when_the_flyout_rect_also_contains_the_icon() {
        let taskbar = rect(0, 1000, 500, 1040);
        let flyout = rect(0, 1000, 500, 1200);
        let icon = rect(10, 1000, 40, 1040);
        let (placed, hidden) = place_icon(&[taskbar], &[flyout], &[flyout], icon);
        assert_eq!(placed, Some(icon));
        assert!(!hidden);
    }

    #[test]
    fn empty_cache_hits_nothing() {
        let targets = WheelTargets::new();
        assert!(!targets.icon_hidden_in_overflow());
        for zone in [
            WheelZone::Off,
            WheelZone::TrayIcon,
            WheelZone::NotificationArea,
            WheelZone::Taskbar,
        ] {
            assert!(!targets.hit(pt(1, 1), zone));
        }
    }

    #[test]
    fn accumulator_emits_whole_notches_and_keeps_the_remainder() {
        let mut wheel = WheelAccumulator::new();
        let t0 = at(0);
        assert_eq!(wheel.feed_at(119, t0), 0);
        assert_eq!(wheel.remainder, 119);
        assert_eq!(wheel.feed_at(1, t0 + Duration::from_millis(10)), 1);
        assert_eq!(wheel.remainder, 0);
        assert_eq!(wheel.feed_at(240, t0 + Duration::from_millis(20)), 2);
        assert_eq!(wheel.feed_at(-120, t0 + Duration::from_millis(30)), -1);
        assert_eq!(wheel.feed_at(-250, t0 + Duration::from_millis(40)), -2);
        assert_eq!(wheel.remainder, -10);
    }

    #[test]
    fn accumulator_resets_on_direction_change_and_after_idle() {
        let mut wheel = WheelAccumulator::new();
        let t0 = at(1_000);
        assert_eq!(wheel.feed_at(100, t0), 0);
        assert_eq!(wheel.feed_at(-120, t0 + Duration::from_millis(10)), -1);
        assert_eq!(wheel.remainder, 0);

        assert_eq!(wheel.feed_at(100, t0 + Duration::from_millis(20)), 0);
        assert_eq!(wheel.feed_at(0, t0 + Duration::from_millis(30)), 0);
        assert_eq!(wheel.remainder, 100);
        assert_eq!(wheel.feed_at(100, t0 + Duration::from_millis(20 + 300)), 0);
        assert_eq!(wheel.remainder, 100);
        assert_eq!(
            wheel.feed_at(20, t0 + Duration::from_millis(20 + 300 + 10)),
            1
        );
    }

    #[test]
    fn high_resolution_deltas_sum_to_one_notch() {
        let mut wheel = WheelAccumulator::new();
        let t0 = at(5_000);
        assert_eq!(wheel.feed_at(40, t0), 0);
        assert_eq!(wheel.feed_at(40, t0 + Duration::from_millis(5)), 0);
        assert_eq!(wheel.feed_at(40, t0 + Duration::from_millis(10)), 1);
    }

    #[test]
    fn decode_wheel_reads_vertical_delta_and_ignores_horizontal() {
        let hook = MSLLHOOKSTRUCT {
            pt: pt(1953, 1400),
            mouseData: (u16::from_ne_bytes((-120i16).to_ne_bytes()) as u32) << 16,
            flags: LLMHF_INJECTED,
            time: 1,
            dwExtraInfo: 0,
        };
        let lparam = LPARAM(&hook as *const MSLLHOOKSTRUCT as isize);
        assert_eq!(decode_wheel(WM_MOUSEHWHEEL, lparam), None);
        assert_eq!(decode_wheel(WM_MOUSEWHEEL, LPARAM(0)), None);
        assert_eq!(
            decode_wheel(WM_MOUSEWHEEL, lparam),
            Some((pt(1953, 1400), -120, true))
        );

        let mut clean = hook;
        clean.flags = LLMHF_LOWER_IL_INJECTED;
        clean.mouseData = (40u32) << 16;
        let lparam = LPARAM(&clean as *const MSLLHOOKSTRUCT as isize);
        assert_eq!(
            decode_wheel(WM_MOUSEWHEEL, lparam),
            Some((pt(1953, 1400), 40, true))
        );

        clean.flags = 0;
        let lparam = LPARAM(&clean as *const MSLLHOOKSTRUCT as isize);
        assert_eq!(
            decode_wheel(WM_MOUSEWHEEL, lparam),
            Some((pt(1953, 1400), 40, false))
        );
    }

    #[test]
    fn hook_reinstall_only_when_input_happened_without_a_callback() {
        let grace = HOOK_REINSTALL_GRACE;
        assert!(!hook_needs_reinstall(
            Duration::from_secs(2),
            Duration::from_secs(2)
        ));
        assert!(!hook_needs_reinstall(grace, Duration::ZERO));
        assert!(hook_needs_reinstall(
            grace + Duration::from_millis(1),
            Duration::ZERO
        ));
        assert!(!hook_needs_reinstall(
            Duration::from_secs(30),
            Duration::from_secs(40)
        ));
    }

    #[test]
    fn fullscreen_cover_ignores_a_maximized_work_area() {
        let monitor = rect(0, 0, 2560, 1440);
        assert!(covers_monitor(monitor, monitor, FULLSCREEN_SLOP));
        assert!(covers_monitor(
            rect(-2, -2, 2561, 1441),
            monitor,
            FULLSCREEN_SLOP
        ));
        assert!(!covers_monitor(
            rect(0, 0, 2560, 1387),
            monitor,
            FULLSCREEN_SLOP
        ));
        assert!(is_shell_or_desktop("Progman"));
        assert!(is_shell_or_desktop("TopLevelWindowForOverflowXamlIsland"));
        assert!(!is_shell_or_desktop("Chrome_WidgetWin_1"));
    }
}
