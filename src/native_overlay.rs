//! Windows overlay behavior. All pixels and frame timing belong to GPUI.
use crate::gpui_overlay::{Command, Snapshot};
use anyhow::Result;
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
        Dwm::{DWMNCRP_DISABLED, DWMWA_NCRENDERING_POLICY, DwmSetWindowAttribute},
        Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
    },
    UI::{
        HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI},
        Input::KeyboardAndMouse::{GetAsyncKeyState, GetDoubleClickTime, VK_LBUTTON},
        WindowsAndMessaging::*,
    },
};

const WM_GPUI_SURFACE: u32 = WM_APP + 50;
const WM_GPUI_PRESENT: u32 = WM_APP + 51;
const WM_GPUI_POLICY: u32 = WM_APP + 52;
const ID_SINGLE_CLICK_TIMER: usize = 32;
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
    surface_width: i32,
    surface_height: i32,
    inset_x: i32,
    inset_y: i32,
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
    muted: bool,
    settings: crate::OverlayConfig,
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
        let class_style = GetClassLongPtrW(hwnd, GCL_STYLE) as usize;
        SetClassLongPtrW(hwnd, GCL_STYLE, (class_style | CS_DBLCLKS.0 as usize) as _);
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
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        )?;
    }
    let native = NativeOverlay {
        hwnd,
        sender,
        muted,
        settings,
        width: 48,
        height: 48,
        last_surface: None,
        presented: false,
        surface_width: 72,
        surface_height: 72,
        inset_x: 12,
        inset_y: 12,
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
    };
    native.apply_click_through();
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
            let _ = PostMessageW(overlay.hwnd, WM_GPUI_POLICY, WPARAM(0), LPARAM(0));
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
        let _ = PostMessageW(overlay.hwnd, WM_GPUI_POLICY, WPARAM(0), LPARAM(0));
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
) {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
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
            self.y - self.inset_y,
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
            if style == next_style {
                return;
            }
            let _ = SetWindowLongW(self.hwnd, GWL_EXSTYLE, next_style);
            if click_through {
                // Keep opacity in GPUI; this alpha only enables Win32 hit testing.
                let _ = SetLayeredWindowAttributes(self.hwnd, COLORREF(0), 255, LWA_ALPHA);
            }
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
