//! Volume slider popup that opens above the volume tray icon.
//!
//! It is a second window on the volume OSD's GPUI thread. The tray thread
//! asks for it through [`volume_popup_show`] and keeps it current with
//! [`volume_popup_update`]; the popup asks the tray thread to change the
//! volume by posting window messages, so all Core Audio writes stay on the
//! tray thread.

use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
};
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromRect},
    UI::{
        HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
        WindowsAndMessaging::*,
    },
};

/// Card width in logical pixels, without the shadow gutter. The height
/// depends on how many apps are playing (see the GPUI view).
pub(crate) const CARD_W: f32 = 360.0;
/// Transparent margin around the card for its shadow.
pub(crate) const GUTTER: f32 = 18.0;
/// Space between the card and the taskbar.
const TASKBAR_GAP: f32 = 6.0;

#[derive(Clone, Debug, PartialEq)]
pub struct VolumePopupState {
    pub percent: u8,
    pub muted: bool,
    pub device_name: String,
}

pub(crate) enum PopupCommand {
    Show {
        anchor: RECT,
        state: VolumePopupState,
    },
    Update(VolumePopupState),
    Close,
    /// Create the hidden window ahead of the first click.
    Prewarm,
}

static OPEN: AtomicBool = AtomicBool::new(false);
static CLOSED_AT_MS: AtomicU64 = AtomicU64::new(0);
static CLOSED_BY_PRESS: AtomicBool = AtomicBool::new(false);
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static ANCHOR: Lazy<Mutex<RECT>> = Lazy::new(|| Mutex::new(RECT::default()));

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn volume_popup_open() -> bool {
    OPEN.load(Ordering::Acquire)
}

/// Pressing the tray icon while the popup is open deactivates (and closes)
/// the popup before the click arrives. Call this on button down: it notes
/// whether this press is the one that closed it, so the click that follows
/// does not reopen it. Tied to the press, not to a time window, so the very
/// next click opens the popup again.
pub fn volume_popup_note_press() {
    // The popup's thread may or may not have seen the deactivation yet.
    let closed = volume_popup_open()
        || now_ms().saturating_sub(CLOSED_AT_MS.load(Ordering::Acquire)) < 150;
    CLOSED_BY_PRESS.store(closed, Ordering::Release);
}

/// Whether the current press already closed the popup. Reads once.
pub fn volume_popup_just_closed() -> bool {
    CLOSED_BY_PRESS.swap(false, Ordering::AcqRel)
}

pub(crate) fn mark_open(open: bool) {
    OPEN.store(open, Ordering::Release);
    if !open {
        CLOSED_AT_MS.store(now_ms(), Ordering::Release);
    }
}

pub fn volume_popup_show(anchor: RECT, state: VolumePopupState) {
    *ANCHOR.lock().unwrap() = anchor;
    crate::volume_osd::send_popup(PopupCommand::Show { anchor, state });
}

pub fn volume_popup_update(state: VolumePopupState) {
    if volume_popup_open() {
        crate::volume_osd::send_popup(PopupCommand::Update(state));
    }
}

pub fn volume_popup_close() {
    if volume_popup_open() {
        crate::volume_osd::send_popup(PopupCommand::Close);
    }
}

/// Strip the frame, keep it out of the taskbar and the alt-tab list, and
/// subclass it so the client area covers the whole window.
pub(crate) fn attach(hwnd: HWND) {
    unsafe {
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let decorations =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0;
        SetWindowLongW(
            hwnd,
            GWL_STYLE,
            (style & !(decorations as i32)) | WS_POPUP.0 as i32,
        );
        let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            ex_style | (WS_EX_TOOLWINDOW | WS_EX_TOPMOST).0 as i32,
        );
        crate::volume_osd::apply_frame(hwnd);
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, popup_wnd_proc as *const () as _);
        ORIGINAL_WNDPROC.store(previous as isize, Ordering::Release);
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

/// The anchor the popup was last opened from.
pub(crate) fn last_anchor() -> RECT {
    *ANCHOR.lock().unwrap()
}

/// Monitor rectangle and the card's own rectangle (physical pixels, screen
/// coordinates, without the shadow gutter) for a card `card_h` tall.
pub(crate) fn card_screen_rect(anchor: RECT, card_h: f32) -> (RECT, RECT) {
    let (x, y, width, height) = placement(anchor, card_h);
    let monitor = unsafe { MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = unsafe { GetMonitorInfoW(monitor, &mut info) };
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let gutter = (GUTTER * dpi_x.max(96) as f32 / 96.0).round() as i32;
    (
        info.rcMonitor,
        RECT {
            left: x + gutter,
            top: y + gutter,
            right: x + width - gutter,
            bottom: y + height - gutter,
        },
    )
}

/// Window rectangle (physical pixels) for the popup next to `anchor`, on the
/// anchor's monitor, sitting on the taskbar edge the icon lives on.
pub(crate) fn placement(anchor: RECT, card_h: f32) -> (i32, i32, i32, i32) {
    let monitor = unsafe { MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    let _ = unsafe { GetMonitorInfoW(monitor, &mut info) };
    let work = info.rcWork;
    let mut dpi_x = 96u32;
    let mut dpi_y = 96u32;
    let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    let scale = dpi_x.max(96) as f32 / 96.0;
    let width = ((CARD_W + GUTTER * 2.0) * scale).round() as i32;
    let height = ((card_h + GUTTER * 2.0) * scale).round() as i32;
    let gutter = (GUTTER * scale).round() as i32;
    let gap = (TASKBAR_GAP * scale).round() as i32;
    let center_x = (anchor.left + anchor.right) / 2;
    let x = (center_x - width / 2).clamp(work.left - gutter, work.right - width + gutter);
    let y = if anchor.top >= work.bottom - 1 {
        // Bottom taskbar: card bottom sits `gap` above the work area.
        work.bottom - gap - height + gutter
    } else if anchor.bottom <= work.top + 1 {
        work.top + gap - gutter
    } else {
        // Side taskbar or unknown: center on the icon vertically.
        ((anchor.top + anchor.bottom) / 2 - height / 2)
            .clamp(work.top - gutter, work.bottom - height + gutter)
    };
    (x, y, width, height)
}

/// Fallback anchor: a small square under the cursor.
pub fn cursor_anchor() -> RECT {
    let mut point = POINT::default();
    let _ = unsafe { GetCursorPos(&mut point) };
    RECT {
        left: point.x - 8,
        top: point.y - 8,
        right: point.x + 8,
        bottom: point.y + 8,
    }
}

unsafe extern "system" fn popup_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCALCSIZE => LRESULT(0),
        _ => {
            let previous = ORIGINAL_WNDPROC.load(Ordering::Acquire);
            if previous == 0 {
                return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
            }
            unsafe {
                CallWindowProcW(
                    Some(std::mem::transmute::<
                        isize,
                        unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
                    >(previous)),
                    hwnd,
                    msg,
                    wparam,
                    lparam,
                )
            }
        }
    }
}
