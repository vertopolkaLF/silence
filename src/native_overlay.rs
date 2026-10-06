//! Windows overlay behavior. All pixels and frame timing belong to GPUI.
use crate::gpui_overlay::{Command, Snapshot};
use anyhow::{Context as _, Result};
use once_cell::sync::Lazy;
use std::{
    mem::size_of,
    sync::{
        Mutex,
        atomic::{AtomicIsize, Ordering},
    },
};
use windows::Win32::{
    Foundation::{BOOL, COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::{
        Dwm::{
            DWM_SYSTEMBACKDROP_TYPE, DWM_WINDOW_CORNER_PREFERENCE, DWMNCRP_DISABLED, DWMSBT_NONE,
            DWMSBT_TRANSIENTWINDOW, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR,
            DWMWA_NCRENDERING_POLICY, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
            DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND, DWMWCP_ROUND,
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
        },
        Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
    },
    UI::{
        Controls::MARGINS,
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetDoubleClickTime, VK_LBUTTON},
        WindowsAndMessaging::*,
    },
};

const WM_GPUI_SURFACE: u32 = WM_APP + 50;
const WM_GPUI_PRESENT: u32 = WM_APP + 51;
const WM_GPUI_POLICY: u32 = WM_APP + 52;
const ID_SINGLE_CLICK_TIMER: usize = 32;
const ID_TOPMOST_TIMER: usize = 33;
const TOPMOST_INTERVAL_MS: u32 = 1_000;
static OVERLAY: Lazy<Mutex<Option<NativeOverlay>>> = Lazy::new(|| Mutex::new(None));
static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);
static ACTIONS: Lazy<Mutex<Vec<crate::OverlayActionBinding>>> =
    Lazy::new(|| Mutex::new(Vec::new()));

struct NativeOverlay {
    hwnd: HWND,
    sender: flume::Sender<Command>,
    muted: bool,
    settings: crate::OverlayConfig,
    width: i32,
    height: i32,
    last_surface: Option<(i32, i32, i32, i32)>,
    presented: bool,
    awaiting_first_paint: bool,
    surface_width: i32,
    surface_height: i32,
    inset_x: i32,
    inset_y: i32,
    /// 0 = in place, 1 = fully past the nearest top/bottom monitor edge.
    slide: f32,
    x: i32,
    y: i32,
    positioning: bool,
    dragging: bool,
    drag_offset_x: i32,
    drag_offset_y: i32,
    drag_start_x: i32,
    drag_start_y: i32,
    drag_moved: bool,
    was_mouse_down: bool,
    awaiting_initial_release: bool,
    visible: bool,
    pending_single_click: bool,
    suppress_next_left_up: bool,
    suppress_next_click_after_drag: bool,
    acrylic: bool,
    chrome_dark: bool,
}
// HWND is only operated on its GPUI thread, or through asynchronous Win32 positioning.
unsafe impl Send for NativeOverlay {}

pub fn init(_instance: HINSTANCE, muted: bool, settings: &crate::OverlayConfig) -> Result<()> {
    if OVERLAY.lock().unwrap().is_some() {
        return Ok(());
    }
    crate::gpui_overlay::start(muted, settings.clone())
}

pub(super) fn attach(
    hwnd: HWND,
    _muted: bool,
    _settings: crate::OverlayConfig,
    sender: flume::Sender<Command>,
) -> Result<()> {
    unsafe {
        // GPUI's popup starts with a zero style. Explicitly make this a borderless
        // popup so Windows cannot supply the default overlapped-window frame.
        let style = GetWindowLongW(hwnd, GWL_STYLE);
        let decorations =
            (WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX).0;
        SetWindowLongW(
            hwnd,
            GWL_STYLE,
            (style & !(decorations as i32)) | WS_POPUP.0 as i32,
        );
        let policy = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY,
            &policy as *const _ as _,
            size_of_val(&policy) as u32,
        )?;
        // GCL_STYLE is a 32-bit value on every architecture; the Ptr bindings
        // are unavailable on x86 in windows-rs.
        let class_style = GetClassLongW(hwnd, GCL_STYLE);
        SetClassLongW(hwnd, GCL_STYLE, (class_style | CS_DBLCLKS.0) as i32);
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as i32,
        );
        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, overlay_wnd_proc as *const () as _);
        anyhow::ensure!(previous != 0, "subclass GPUI overlay window");
        ORIGINAL_WNDPROC.store(previous as isize, Ordering::Release);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        )?;
    }
    let mut guard = OVERLAY.lock().unwrap();
    let native = guard.as_mut().context("overlay control state missing")?;
    native.hwnd = hwnd;
    native.sender = sender;
    // Keep the latest audio/settings/positioning state received during recreation.
    native.last_surface = None;
    native.presented = false;
    native.awaiting_first_paint = true;
    native.acrylic = false;
    native.apply_click_through();
    // Apply the measured warm-up position before ShowWindow can expose it.
    native.apply_layout();
    Ok(())
}

pub(super) fn prepare(
    muted: bool,
    settings: crate::OverlayConfig,
    sender: flume::Sender<Command>,
) -> Result<()> {
    let native = NativeOverlay {
        hwnd: HWND::default(),
        sender,
        muted,
        settings,
        width: 48,
        height: 48,
        last_surface: None,
        presented: false,
        awaiting_first_paint: true,
        surface_width: 72,
        surface_height: 72,
        inset_x: 12,
        inset_y: 12,
        slide: 0.,
        x: 100,
        y: 100,
        positioning: false,
        dragging: false,
        drag_offset_x: 0,
        drag_offset_y: 0,
        drag_start_x: 0,
        drag_start_y: 0,
        drag_moved: false,
        was_mouse_down: false,
        awaiting_initial_release: false,
        visible: false,
        pending_single_click: false,
        suppress_next_left_up: false,
        suppress_next_click_after_drag: false,
        acrylic: false,
        chrome_dark: true,
    };
    *OVERLAY.lock().unwrap() = Some(native);
    Ok(())
}

pub fn update(muted: bool, settings: &crate::OverlayConfig) {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.muted = match settings.visibility.as_str() {
            "WhenMuted" => true,
            "WhenUnmuted" => false,
            _ => muted,
        };
        overlay.settings = settings.clone();
        if overlay.settings.behaviour != "Button" || overlay.positioning {
            overlay.pending_single_click = false;
            unsafe {
                let _ = KillTimer(overlay.hwnd, ID_SINGLE_CLICK_TIMER);
            }
        }
        unsafe {
            if !overlay.hwnd.0.is_null() {
                let _ = PostMessageW(overlay.hwnd, WM_GPUI_POLICY, WPARAM(0), LPARAM(0));
            }
        }
        overlay.notify();
    }
}

pub fn show() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        if !overlay.visible {
            overlay.visible = true;
            overlay.notify();
        }
    }
}
pub fn hide() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        if overlay.visible {
            overlay.visible = false;
            overlay.notify();
        }
    }
}
pub fn reposition() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
        overlay.notify();
    }
}
pub fn set_positioning(active: bool) -> Option<(f64, f64)> {
    let mut guard = OVERLAY.lock().unwrap();
    let overlay = guard.as_mut()?;
    let position = if active {
        None
    } else {
        Some(overlay.current_percent_position())
    };
    overlay.positioning = active;
    overlay.dragging = false;
    overlay.drag_moved = false;
    overlay.was_mouse_down = false;
    overlay.awaiting_initial_release = active && mouse_down();
    if active {
        overlay.visible = true;
    }
    unsafe {
        if !overlay.hwnd.0.is_null() {
            let _ = PostMessageW(overlay.hwnd, WM_GPUI_POLICY, WPARAM(0), LPARAM(0));
        }
    }
    overlay.notify();
    position
}
pub fn process_drag() -> Option<(f64, f64)> {
    OVERLAY.lock().unwrap().as_mut()?.process_drag()
}
pub fn is_positioning() -> bool {
    OVERLAY
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|overlay| overlay.positioning)
}
pub fn destroy() {
    // GPUI owns the HWND: quit its event loop rather than destroying a foreign-thread window.
    if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
        let _ = overlay.sender.send(Command::Shutdown);
    }
}
pub(super) fn suspend_surface() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.hwnd = HWND::default();
        overlay.presented = false;
        overlay.last_surface = None;
        overlay.pending_single_click = false;
        overlay.dragging = false;
    }
}

pub(super) fn detach() {
    OVERLAY.lock().unwrap().take();
}
pub(super) fn snapshot() -> Option<Snapshot> {
    let guard = OVERLAY.lock().unwrap();
    let overlay = guard.as_ref()?;
    Some(Snapshot {
        muted: overlay.muted,
        settings: overlay.settings.clone(),
        visible: overlay.visible,
        positioning: overlay.positioning,
    })
}
pub(super) fn scale() -> f32 {
    OVERLAY
        .lock()
        .unwrap()
        .as_ref()
        .map_or(1.0, |overlay| overlay.native_scale() as f32)
}
pub(super) fn set_geometry(
    width: i32,
    height: i32,
    surface_width: i32,
    surface_height: i32,
    inset_x: i32,
    inset_y: i32,
    slide: f32,
) {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.slide = slide;
        overlay.width = width;
        overlay.height = height;
        overlay.surface_width = surface_width;
        overlay.surface_height = surface_height;
        overlay.inset_x = inset_x;
        overlay.inset_y = inset_y;
        overlay.apply_layout();
    }
}
pub(super) fn present(visible: bool) {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        if overlay.presented != visible {
            overlay.presented = visible;
            unsafe {
                let _ = PostMessageW(overlay.hwnd, WM_GPUI_PRESENT, WPARAM(0), LPARAM(0));
            }
        }
    }
}
/// Returns false while the HWND is not attached yet.
pub(super) fn set_chrome(acrylic: bool, dark: bool) -> bool {
    let mut guard = OVERLAY.lock().unwrap();
    let Some(overlay) = guard.as_mut() else {
        return false;
    };
    if overlay.acrylic != acrylic || overlay.chrome_dark != dark {
        overlay.acrylic = acrylic;
        overlay.chrome_dark = dark;
        overlay.apply_chrome();
    }
    true
}

fn queue_action(binding: crate::OverlayActionBinding) {
    if binding.action.is_none() {
        return;
    }
    ACTIONS.lock().unwrap().push(binding);
    let hwnd = crate::STATE.lock().unwrap().hwnd;
    unsafe {
        let _ = PostMessageW(hwnd, crate::WM_OVERLAY_ACTION, WPARAM(0), LPARAM(0));
    }
}
pub(crate) fn drain_actions() {
    let actions = std::mem::take(&mut *ACTIONS.lock().unwrap());
    for action in actions {
        crate::run_overlay_action(action);
    }
}
unsafe fn forward_window_message(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
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

fn restore_topmost(hwnd: HWND) {
    // Z-order maintenance must run on the window thread, outside OVERLAY's lock:
    // SetWindowPos synchronously dispatches window messages back into this wndproc.
    // Reassert even when WS_EX_TOPMOST is set: another topmost window can cover us.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
        );
    }
}

impl NativeOverlay {
    fn notify(&self) {
        let _ = self.sender.send(Command::Refresh);
    }
    fn apply_layout(&mut self) {
        if !self.dragging {
            self.x = self.saved_x();
            self.y = self.saved_y();
        }
        self.place_surface();
    }
    fn place_surface(&mut self) {
        let bounds = (
            self.x - self.inset_x,
            self.y - self.inset_y + self.slide_offset(),
            self.surface_width,
            self.surface_height,
        );
        if self.last_surface != Some(bounds) {
            self.last_surface = Some(bounds);
            // Apply after GPUI releases its frame borrows, never re-enter the renderer.
            unsafe {
                let _ = PostMessageW(self.hwnd, WM_GPUI_SURFACE, WPARAM(0), LPARAM(0));
            }
        }
    }
    /// Acrylic cannot fade, so that theme leaves through the nearest top or
    /// bottom monitor edge instead, like the shell's own flyouts.
    fn slide_offset(&self) -> i32 {
        if self.slide <= 0. {
            return 0;
        }
        let rect = self.selected_monitor().rect;
        let surface_top = self.y - self.inset_y;
        let surface_bottom = surface_top + self.surface_height;
        let distance = if surface_top - rect.top < rect.bottom - surface_bottom {
            rect.top - surface_bottom
        } else {
            rect.bottom - surface_top
        };
        (distance as f32 * self.slide).round() as i32
    }

    fn saved_x(&self) -> i32 {
        let monitor = self.selected_monitor();
        let rect = monitor.rect;
        let screen = (rect.right - rect.left).max(self.width);
        rect.left + percent_to_axis(self.settings.position_x, screen, self.width)
    }

    fn saved_y(&self) -> i32 {
        let monitor = self.selected_monitor();
        let rect = monitor.rect;
        let screen = (rect.bottom - rect.top).max(self.height);
        rect.top + percent_to_axis(self.settings.position_y, screen, self.height)
    }

    fn process_drag(&mut self) -> Option<(f64, f64)> {
        if self.hwnd.0.is_null() {
            return None;
        }
        if !self.drag_enabled() {
            self.suppress_next_click_after_drag = false;
            return None;
        }

        let mut cursor = windows::Win32::Foundation::POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut cursor);
        }
        let mouse_down = mouse_down();
        if self.awaiting_initial_release {
            if !mouse_down {
                self.awaiting_initial_release = false;
                self.apply_click_through();
            }
            self.was_mouse_down = mouse_down;
            return None;
        }

        if mouse_down && !self.was_mouse_down && self.contains(cursor.x, cursor.y) {
            self.dragging = true;
            self.drag_offset_x = cursor.x - self.x;
            self.drag_offset_y = cursor.y - self.y;
            self.drag_start_x = cursor.x;
            self.drag_start_y = cursor.y;
            self.drag_moved = false;
        }

        if self.dragging && mouse_down {
            if (cursor.x - self.drag_start_x).abs() >= 3
                || (cursor.y - self.drag_start_y).abs() >= 3
            {
                self.drag_moved = true;
            }
            if !self.drag_moved {
                self.was_mouse_down = mouse_down;
                return None;
            }
            let monitor = self.selected_monitor();
            let rect = monitor.rect;
            let max_x = (rect.right - self.width).max(rect.left);
            let max_y = (rect.bottom - self.height).max(rect.top);
            self.x = (cursor.x - self.drag_offset_x).clamp(rect.left, max_x);
            self.y = (cursor.y - self.drag_offset_y).clamp(rect.top, max_y);
            self.place_surface();
        }

        let mut saved = None;
        if self.dragging && !mouse_down {
            self.dragging = false;
            if self.drag_moved && !self.positioning {
                self.suppress_next_click_after_drag = true;
            }
            if self.drag_moved {
                saved = Some(self.current_percent_position());
            }
            self.drag_moved = false;
        }

        self.was_mouse_down = mouse_down;
        saved
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    fn finish_button_drag_release(&mut self) -> Option<(f64, f64)> {
        if self.positioning
            || self.settings.behaviour != "Button"
            || !self.dragging
            || !self.drag_moved
        {
            return None;
        }

        self.dragging = false;
        self.drag_moved = false;
        self.was_mouse_down = false;
        self.suppress_next_click_after_drag = false;
        self.pending_single_click = false;
        Some(self.current_percent_position())
    }

    fn drag_enabled(&self) -> bool {
        self.positioning || self.settings.behaviour == "Button"
    }

    fn current_percent_position(&mut self) -> (f64, f64) {
        let monitor = self.selected_monitor();
        let rect = monitor.rect;
        let width = (rect.right - rect.left).max(self.width);
        let height = (rect.bottom - rect.top).max(self.height);
        let x = axis_to_percent(self.x - rect.left, width, self.width);
        let y = axis_to_percent(self.y - rect.top, height, self.height);
        self.settings.position_x = x;
        self.settings.position_y = y;
        self.notify();
        (x, y)
    }

    fn selected_monitor(&self) -> MonitorRect {
        selected_monitor(&self.settings.display)
    }

    fn native_scale(&self) -> f64 {
        let user_scale = self.settings.scale.clamp(10, 400) as f64 / 100.0;
        user_scale
            * monitor_dpi_scale(self.selected_monitor().handle)
                .unwrap_or_else(|| dpi_scale(self.hwnd))
    }

    fn set_click_through(&self, click_through: bool) {
        unsafe {
            let style = GetWindowLongW(self.hwnd, GWL_EXSTYLE);
            // HTTRANSPARENT alone only forwards within the same thread. Layered
            // + transparent also passes input to windows owned by other apps.
            let transparent = (WS_EX_TRANSPARENT | WS_EX_LAYERED).0 as i32;
            let next_style = if click_through {
                style | transparent
            } else {
                style & !transparent
            };
            if style != next_style {
                let _ = SetWindowLongW(self.hwnd, GWL_EXSTYLE, next_style);
                if click_through {
                    // Keep opacity in GPUI; this alpha only enables Win32 hit testing.
                    let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), 255, LWA_ALPHA);
                }
            }
        }
        if self.acrylic {
            self.apply_chrome();
        }
    }

    fn apply_chrome(&self) {
        unsafe {
            let extend = if self.acrylic { -1 } else { 0 };
            let _ = DwmExtendFrameIntoClientArea(
                self.hwnd,
                &MARGINS {
                    cxLeftWidth: extend,
                    cxRightWidth: extend,
                    cyTopHeight: extend,
                    cyBottomHeight: extend,
                },
            );
            // DWMWA_COLOR_NONE drops the hairline and any caption tint;
            // DWMWA_COLOR_DEFAULT restores them.
            let color: u32 = if self.acrylic {
                0xFFFF_FFFE
            } else {
                0xFFFF_FFFF
            };
            for attribute in [DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR] {
                let _ = DwmSetWindowAttribute(
                    self.hwnd,
                    attribute,
                    &color as *const _ as _,
                    size_of::<u32>() as u32,
                );
            }
            let dark_mode = i32::from(self.chrome_dark);
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as _,
                size_of::<i32>() as u32,
            );
            // ROUND is the largest radius DWM will put on a borderless popup.
            let corners = if self.acrylic {
                DWMWCP_ROUND
            } else {
                DWMWCP_DONOTROUND
            };
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corners as *const _ as _,
                size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            );
            let backdrop = if self.acrylic {
                DWMSBT_TRANSIENTWINDOW
            } else {
                DWMSBT_NONE
            };
            let _ = DwmSetWindowAttribute(
                self.hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as _,
                size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            );
            let _ = SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
            // Backdrops fall back to a flat fill on inactive frames, and this
            // window never activates; the wndproc keeps the frame "active".
            // NC rendering stays disabled (see attach), which keeps DWM's
            // compact inactive shadow instead of the large active-window one.
            let _ = PostMessageW(self.hwnd, WM_NCACTIVATE, WPARAM(1), LPARAM(0));
        }
    }

    fn apply_click_through(&self) {
        let button_mode = self.settings.behaviour == "Button";
        let click_through = (!self.positioning && !button_mode) || self.awaiting_initial_release;
        self.set_click_through(click_through);
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        // Every pixel, including the shadow gutter, belongs to GPUI.
        WM_NCCALCSIZE => LRESULT(0),
        WM_PAINT => {
            // GPUI's next-frame callback can run before ShowWindow and the
            // first GPU presentation. Start entrance motion only after a paint
            // of the attached, visible HWND has returned to Windows.
            let result = unsafe { forward_window_message(hwnd, msg, wparam, lparam) };
            if unsafe { IsWindowVisible(hwnd).as_bool() } {
                if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
                    if overlay.awaiting_first_paint && overlay.presented {
                        overlay.awaiting_first_paint = false;
                        let _ = overlay.sender.send(Command::SurfaceReady);
                    }
                }
            }
            result
        }
        WM_GPUI_SURFACE => {
            let bounds = OVERLAY
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|overlay| overlay.last_surface);
            if let Some((x, y, width, height)) = bounds {
                unsafe {
                    let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
                }
            }
            LRESULT(0)
        }
        WM_GPUI_PRESENT => {
            let visible = OVERLAY
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|overlay| overlay.presented);
            unsafe {
                let _ = ShowWindow(hwnd, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
                if visible {
                    restore_topmost(hwnd);
                    if SetTimer(hwnd, ID_TOPMOST_TIMER, TOPMOST_INTERVAL_MS, None) == 0 {
                        eprintln!("overlay: failed to start topmost maintenance timer");
                    }
                } else {
                    let _ = KillTimer(hwnd, ID_TOPMOST_TIMER);
                }
            }
            LRESULT(0)
        }
        WM_GPUI_POLICY => {
            if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
                overlay.apply_click_through();
            }
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_NCACTIVATE => {
            let acrylic = OVERLAY
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|overlay| overlay.acrylic);
            let wparam = if acrylic { WPARAM(1) } else { wparam };
            unsafe { forward_window_message(hwnd, msg, wparam, lparam) }
        }
        WM_NCHITTEST => {
            // The transparent shadow gutter must never steal clicks from another app.
            let x = lparam.0 as i16 as i32;
            let y = (lparam.0 >> 16) as i16 as i32;
            if let Ok(guard) = OVERLAY.try_lock() {
                if let Some(overlay) = guard.as_ref() {
                    if !overlay.contains(x, y)
                        || ((!overlay.positioning && overlay.settings.behaviour != "Button")
                            || overlay.awaiting_initial_release)
                    {
                        return LRESULT(HTTRANSPARENT as isize);
                    }
                }
            }
            LRESULT(HTCLIENT as isize)
        }
        WM_SETCURSOR => {
            if OVERLAY
                .lock()
                .unwrap()
                .as_ref()
                .map(|overlay| overlay.positioning)
                .unwrap_or(false)
            {
                unsafe {
                    if let Ok(cursor) = LoadCursorW(None, IDC_SIZEALL) {
                        let _ = SetCursor(cursor);
                    }
                }
                return LRESULT(1);
            }
            unsafe { forward_window_message(hwnd, msg, wparam, lparam) }
        }
        WM_TIMER if wparam.0 == ID_TOPMOST_TIMER => {
            // A timer message may already be queued when the surface is hidden.
            // Never show it again or wake the GPUI renderer from this maintenance path.
            if unsafe { IsWindowVisible(hwnd).as_bool() } {
                restore_topmost(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => unsafe {
            let _ = KillTimer(hwnd, ID_TOPMOST_TIMER);
            forward_window_message(hwnd, msg, wparam, lparam)
        },
        WM_TIMER if wparam.0 == ID_SINGLE_CLICK_TIMER => {
            let action = {
                let mut guard = OVERLAY.lock().unwrap();
                if let Some(overlay) = guard.as_mut() {
                    unsafe {
                        let _ = KillTimer(overlay.hwnd, ID_SINGLE_CLICK_TIMER);
                    }
                    if overlay.pending_single_click {
                        overlay.pending_single_click = false;
                        Some(overlay.settings.single_click.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(action) = action {
                queue_action(action);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            // GPUI owns mouse capture established on button-down. Always release it.
            let forwarded = unsafe { forward_window_message(hwnd, msg, wparam, lparam) };
            let finished_drag = {
                let mut guard = OVERLAY.lock().unwrap();
                guard
                    .as_mut()
                    .and_then(|overlay| overlay.finish_button_drag_release())
            };
            if let Some((x, y)) = finished_drag {
                crate::save_overlay_position(x, y);
                return LRESULT(0);
            }

            let immediate_action = {
                let mut guard = OVERLAY.lock().unwrap();
                if let Some(overlay) = guard.as_mut() {
                    if overlay.settings.behaviour == "Button" && !overlay.positioning {
                        if overlay.suppress_next_click_after_drag {
                            overlay.suppress_next_click_after_drag = false;
                            overlay.pending_single_click = false;
                            unsafe {
                                let _ = KillTimer(overlay.hwnd, ID_SINGLE_CLICK_TIMER);
                            }
                            return LRESULT(0);
                        }
                        if overlay.suppress_next_left_up {
                            overlay.suppress_next_left_up = false;
                            return LRESULT(0);
                        }
                        if overlay.settings.double_click.action.is_none() {
                            Some(overlay.settings.single_click.clone())
                        } else {
                            overlay.pending_single_click = true;
                            unsafe {
                                let _ = SetTimer(
                                    overlay.hwnd,
                                    ID_SINGLE_CLICK_TIMER,
                                    GetDoubleClickTime().max(1),
                                    None,
                                );
                            }
                            return LRESULT(0);
                        }
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(action) = immediate_action {
                queue_action(action);
                return LRESULT(0);
            }
            forwarded
        }
        WM_LBUTTONDBLCLK => {
            let _ = unsafe { forward_window_message(hwnd, WM_LBUTTONDOWN, wparam, lparam) };
            let action = {
                let mut guard = OVERLAY.lock().unwrap();
                if let Some(overlay) = guard.as_mut() {
                    unsafe {
                        let _ = KillTimer(overlay.hwnd, ID_SINGLE_CLICK_TIMER);
                    }
                    overlay.pending_single_click = false;
                    if overlay.settings.behaviour == "Button" && !overlay.positioning {
                        overlay.suppress_next_left_up = true;
                        Some(overlay.settings.double_click.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some(action) = action {
                queue_action(action);
            }
            LRESULT(0)
        }
        WM_MBUTTONUP | WM_RBUTTONUP | WM_MOUSEWHEEL => {
            let forwarded = (msg != WM_MOUSEWHEEL)
                .then(|| unsafe { forward_window_message(hwnd, msg, wparam, lparam) });
            let action = OVERLAY.lock().unwrap().as_ref().and_then(|overlay| {
                if overlay.settings.behaviour != "Button" || overlay.positioning {
                    return None;
                }
                match msg {
                    WM_MBUTTONUP => Some(overlay.settings.middle_click.clone()),
                    WM_RBUTTONUP => Some(overlay.settings.right_click.clone()),
                    WM_MOUSEWHEEL if wheel_delta(wparam) > 0 => {
                        Some(overlay.settings.wheel_up.clone())
                    }
                    WM_MOUSEWHEEL if wheel_delta(wparam) < 0 => {
                        Some(overlay.settings.wheel_down.clone())
                    }
                    _ => None,
                }
            });
            if let Some(action) = action {
                queue_action(action);
                return LRESULT(0);
            }
            forwarded
                .unwrap_or_else(|| unsafe { forward_window_message(hwnd, msg, wparam, lparam) })
        }
        _ => unsafe { forward_window_message(hwnd, msg, wparam, lparam) },
    }
}

fn wheel_delta(wparam: WPARAM) -> i16 {
    ((wparam.0 >> 16) & 0xffff) as u16 as i16
}

fn percent_to_axis(percent: f64, screen: i32, size: i32) -> i32 {
    let available = (screen - size).max(0) as f64;
    (available * percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

fn axis_to_percent(position: i32, screen: i32, size: i32) -> f64 {
    let available = (screen - size).max(1) as f64;
    (position as f64 * 100.0 / available).clamp(0.0, 100.0)
}

fn selected_monitor(display: &str) -> MonitorRect {
    let monitors = monitor_rects();
    let primary = monitors
        .iter()
        .find(|monitor| monitor.primary)
        .or_else(|| monitors.first());

    if display == crate::OVERLAY_DISPLAY_PRIMARY || display.is_empty() {
        return primary.cloned().unwrap_or_default();
    }

    monitors
        .iter()
        .find(|monitor| monitor.id == display)
        .or(primary)
        .cloned()
        .unwrap_or_default()
}

fn monitor_rects() -> Vec<MonitorRect> {
    let mut monitors = Vec::<MonitorRect>::new();
    unsafe {
        let _ = EnumDisplayMonitors(
            HDC::default(),
            None,
            Some(collect_monitor_rect),
            LPARAM(&mut monitors as *mut _ as isize),
        );
    }
    monitors
}

#[derive(Clone, Default)]
struct MonitorRect {
    handle: HMONITOR,
    id: String,
    rect: RECT,
    primary: bool,
}

unsafe extern "system" fn collect_monitor_rect(
    monitor: HMONITOR,
    _hdc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    let monitors = unsafe { &mut *(data.0 as *mut Vec<MonitorRect>) };
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        monitors.push(MonitorRect {
            handle: monitor,
            id: format!("Monitor{}", monitors.len() + 1),
            rect: info.rcMonitor,
            primary: (info.dwFlags & 1) != 0,
        });
    }
    true.into()
}

fn dpi_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    dpi as f64 / 96.0
}

fn monitor_dpi_scale(monitor: HMONITOR) -> Option<f64> {
    if monitor.0.is_null() {
        return None;
    }

    let mut dpi_x = 96;
    let mut dpi_y = 96;
    unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) }.ok()?;
    Some(dpi_x.max(dpi_y).max(96) as f64 / 96.0)
}

fn mouse_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}
