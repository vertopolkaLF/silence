use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, CreatePen,
            CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH, DeleteObject, DrawTextW, EndPaint,
            FF_DONTCARE, FW_MEDIUM, FillRect, OUT_DEFAULT_PRECIS, PAINTSTRUCT, PS_SOLID, RoundRect,
            SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetCursorPos,
            GetSystemMetrics, GetWindowLongW, HWND_TOPMOST, IDC_ARROW, IDC_SIZEALL, LWA_ALPHA,
            LWA_COLORKEY, LoadCursorW, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE,
            SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
            SWP_SHOWWINDOW, SetCursor, SetLayeredWindowAttributes, SetWindowLongW, SetWindowPos,
            ShowWindow, WM_ERASEBKGND, WM_PAINT, WM_SETCURSOR, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};

const CLASS_NAME: PCWSTR = w!("SilenceV2Overlay");
const TRANSPARENT_KEY: COLORREF = COLORREF(0x00ff00ff);
const OVERLAY_ALPHA: u8 = 245;
const FADE_DURATION_MS: u32 = 300;
const FADE_STEPS: u32 = 18;

static OVERLAY: Lazy<Mutex<Option<NativeOverlay>>> = Lazy::new(|| Mutex::new(None));
static FADE_EPOCH: AtomicU64 = AtomicU64::new(0);

struct NativeOverlay {
    hwnd: HWND,
    muted: bool,
    settings: crate::OverlayConfig,
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    positioning: bool,
    dragging: bool,
    drag_offset_x: i32,
    drag_offset_y: i32,
    was_mouse_down: bool,
    awaiting_initial_release: bool,
    visible: bool,
}

unsafe impl Send for NativeOverlay {}

pub fn init(instance: HINSTANCE, muted: bool, settings: &crate::OverlayConfig) -> Result<()> {
    let mut overlay = OVERLAY.lock().unwrap();
    if overlay.is_some() {
        return Ok(());
    }

    unsafe {
        let class = WNDCLASSW {
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hInstance: instance,
            lpszClassName: CLASS_NAME,
            lpfnWndProc: Some(overlay_wnd_proc),
            ..Default::default()
        };
        RegisterClassW(&class);
    }

    let ex_style =
        WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST;
    let hwnd = unsafe {
        CreateWindowExW(
            ex_style,
            CLASS_NAME,
            w!("silence! overlay"),
            WS_POPUP,
            100,
            100,
            48,
            48,
            None,
            None,
            instance,
            None,
        )
    }
    .context("create overlay window")?;

    unsafe {
        SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, OVERLAY_ALPHA, LWA_COLORKEY | LWA_ALPHA)
            .context("configure overlay transparency")?;
    }

    let mut native = NativeOverlay {
        hwnd,
        muted,
        settings: settings.clone(),
        width: 48,
        height: 48,
        x: 100,
        y: 100,
        positioning: false,
        dragging: false,
        drag_offset_x: 0,
        drag_offset_y: 0,
        was_mouse_down: false,
        awaiting_initial_release: false,
        visible: false,
    };
    native.apply_layout();
    *overlay = Some(native);
    Ok(())
}

pub fn update(muted: bool, settings: &crate::OverlayConfig) {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.muted = muted;
        overlay.settings = settings.clone();
        overlay.apply_layout();
        overlay.repaint();
    }
}

pub fn show() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.show();
    }
}

pub fn hide() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.hide();
    }
}

pub fn reposition() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.apply_layout();
    }
}

pub fn set_positioning(active: bool) -> Option<(f64, f64)> {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        let position = if active {
            None
        } else {
            Some(overlay.current_percent_position())
        };
        overlay.positioning = active;
        overlay.dragging = false;
        overlay.was_mouse_down = false;
        overlay.awaiting_initial_release = active && mouse_down();
        overlay.set_click_through(!active || overlay.awaiting_initial_release);
        if active {
            overlay.apply_layout();
            overlay.show();
        }
        return position;
    }

    None
}

pub fn process_drag() -> Option<(f64, f64)> {
    OVERLAY.lock().unwrap().as_mut()?.process_drag()
}

pub fn is_positioning() -> bool {
    OVERLAY
        .lock()
        .unwrap()
        .as_ref()
        .map(|overlay| overlay.positioning)
        .unwrap_or(false)
}

pub fn destroy() {
    if let Some(overlay) = OVERLAY.lock().unwrap().take() {
        unsafe {
            let _ = DestroyWindow(overlay.hwnd);
        }
    }
}

impl NativeOverlay {
    fn show(&mut self) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.width,
                self.height,
                SWP_NOACTIVATE,
            );

            if self.visible {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    self.x,
                    self.y,
                    self.width,
                    self.height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                return;
            }

            self.visible = true;
            let epoch = next_fade_epoch();
            set_alpha(self.hwnd, 0);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.width,
                self.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            fade_alpha(self.hwnd, epoch, 0, OVERLAY_ALPHA, false);
        }
    }

    fn hide(&mut self) {
        unsafe {
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                return;
            }

            self.visible = false;
            let epoch = next_fade_epoch();
            fade_alpha(self.hwnd, epoch, OVERLAY_ALPHA, 0, true);
        }
    }

    fn apply_layout(&mut self) {
        let scale = self.settings.scale.clamp(10, 400) as f64 / 100.0;
        self.height = (48.0 * scale).round() as i32;
        self.width = if self.settings.show_text {
            (210.0 * scale).round() as i32
        } else {
            self.height
        };
        if !self.dragging {
            self.x = self.saved_x();
            self.y = self.saved_y();
        }

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.width,
                self.height,
                SWP_NOACTIVATE,
            );
        }
    }

    fn saved_x(&self) -> i32 {
        let screen = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(self.width);
        percent_to_axis(self.settings.position_x, screen, self.width)
    }

    fn saved_y(&self) -> i32 {
        let screen = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(self.height);
        percent_to_axis(self.settings.position_y, screen, self.height)
    }

    fn process_drag(&mut self) -> Option<(f64, f64)> {
        if !self.positioning {
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
                self.set_click_through(false);
            }
            self.was_mouse_down = mouse_down;
            return None;
        }

        if mouse_down && !self.was_mouse_down && self.contains(cursor.x, cursor.y) {
            self.dragging = true;
            self.drag_offset_x = cursor.x - self.x;
            self.drag_offset_y = cursor.y - self.y;
        }

        if self.dragging && mouse_down {
            self.x = (cursor.x - self.drag_offset_x).clamp(0, self.screen_width() - self.width);
            self.y = (cursor.y - self.drag_offset_y).clamp(0, self.screen_height() - self.height);
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    self.x,
                    self.y,
                    self.width,
                    self.height,
                    SWP_NOACTIVATE,
                );
            }
        }

        let mut saved = None;
        if self.dragging && !mouse_down {
            self.dragging = false;
            saved = Some(self.current_percent_position());
        }

        self.was_mouse_down = mouse_down;
        saved
    }

    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }

    fn current_percent_position(&mut self) -> (f64, f64) {
        let width = self.screen_width();
        let height = self.screen_height();
        let x = axis_to_percent(self.x, width, self.width);
        let y = axis_to_percent(self.y, height, self.height);
        self.settings.position_x = x;
        self.settings.position_y = y;
        (x, y)
    }

    fn screen_width(&self) -> i32 {
        unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(self.width)
    }

    fn screen_height(&self) -> i32 {
        unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(self.height)
    }

    fn set_click_through(&self, click_through: bool) {
        unsafe {
            let style = GetWindowLongW(self.hwnd, GWL_EXSTYLE);
            let transparent = WS_EX_TRANSPARENT.0 as i32;
            let next_style = if click_through {
                style | transparent
            } else {
                style & !transparent
            };
            let _ = SetWindowLongW(self.hwnd, GWL_EXSTYLE, next_style);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER,
            );
        }
    }

    fn repaint(&self) {
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(self.hwnd, None, false);
        }
    }

    fn paint(&self) {
        unsafe {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(self.hwnd, &mut ps);
            let full = RECT {
                left: 0,
                top: 0,
                right: self.width,
                bottom: self.height,
            };
            let key_brush = CreateSolidBrush(TRANSPARENT_KEY);
            FillRect(hdc, &full, key_brush);
            let _ = DeleteObject(key_brush);

            let background = if self.muted {
                colorref(42, 18, 18)
            } else {
                colorref(18, 35, 26)
            };
            let border = if self.muted {
                colorref(132, 51, 51)
            } else {
                colorref(54, 120, 83)
            };
            let accent = if self.muted {
                colorref(245, 95, 95)
            } else {
                colorref(78, 210, 132)
            };
            let radius = (8.0 * (self.height as f64 / 48.0)).round() as i32;
            let brush = CreateSolidBrush(background);
            let pen = CreatePen(PS_SOLID, 1, border);
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = RoundRect(hdc, 0, 0, self.width, self.height, radius, radius);
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            let _ = DeleteObject(pen);
            let _ = DeleteObject(brush);

            let _ = SetBkMode(hdc, TRANSPARENT);
            let _ = SetTextColor(hdc, accent);

            let font_size = -(self.height as f64 * 0.58).round() as i32;
            let icon_face = crate::wide("Segoe Fluent Icons");
            let icon_font = CreateFontW(
                font_size,
                0,
                0,
                0,
                FW_MEDIUM.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR(icon_face.as_ptr()),
            );
            let old_font = SelectObject(hdc, icon_font);
            let glyph = if self.muted { "\u{F781}" } else { "\u{E720}" };
            let mut icon_text: Vec<u16> = glyph.encode_utf16().collect();
            let icon_right = self.height;
            let mut icon_rect = RECT {
                left: 0,
                top: 0,
                right: icon_right,
                bottom: self.height,
            };
            DrawTextW(
                hdc,
                &mut icon_text,
                &mut icon_rect,
                windows::Win32::Graphics::Gdi::DT_CENTER
                    | windows::Win32::Graphics::Gdi::DT_VCENTER
                    | windows::Win32::Graphics::Gdi::DT_SINGLELINE,
            );
            let _ = SelectObject(hdc, old_font);
            let _ = DeleteObject(icon_font);

            if self.settings.show_text {
                let text_face = crate::wide("Geist");
                let text_font = CreateFontW(
                    -(self.height as f64 * 0.28).round() as i32,
                    0,
                    0,
                    0,
                    FW_MEDIUM.0 as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    CLEARTYPE_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    PCWSTR(text_face.as_ptr()),
                );
                let old_font = SelectObject(hdc, text_font);
                let _ = SetTextColor(hdc, colorref(245, 245, 245));
                let mut label: Vec<u16> = if self.muted {
                    "Microphone muted"
                } else {
                    "Microphone on"
                }
                .encode_utf16()
                .collect();
                let mut text_rect = RECT {
                    left: self.height - 3,
                    top: 0,
                    right: self.width - 12,
                    bottom: self.height,
                };
                DrawTextW(
                    hdc,
                    &mut label,
                    &mut text_rect,
                    windows::Win32::Graphics::Gdi::DT_VCENTER
                        | windows::Win32::Graphics::Gdi::DT_SINGLELINE,
                );
                let _ = SelectObject(hdc, old_font);
                let _ = DeleteObject(text_font);
            }

            let _ = EndPaint(self.hwnd, &ps);
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
                overlay.paint();
            } else {
                let mut ps = PAINTSTRUCT::default();
                unsafe {
                    BeginPaint(hwnd, &mut ps);
                    let _ = EndPaint(hwnd, &ps);
                }
            }
            LRESULT(0)
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
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn percent_to_axis(percent: f64, screen: i32, size: i32) -> i32 {
    let available = (screen - size).max(0) as f64;
    (available * percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

fn axis_to_percent(position: i32, screen: i32, size: i32) -> f64 {
    let available = (screen - size).max(1) as f64;
    (position as f64 * 100.0 / available).clamp(0.0, 100.0)
}

fn next_fade_epoch() -> u64 {
    FADE_EPOCH.fetch_add(1, Ordering::SeqCst) + 1
}

fn fade_alpha(hwnd: HWND, epoch: u64, from: u8, to: u8, hide_after: bool) {
    let hwnd_value = hwnd.0 as isize;
    thread::spawn(move || {
        let hwnd = HWND(hwnd_value as _);
        for step in 0..=FADE_STEPS {
            if FADE_EPOCH.load(Ordering::SeqCst) != epoch {
                return;
            }

            let progress = step as f64 / FADE_STEPS as f64;
            let alpha = from as f64 + (to as f64 - from as f64) * progress;
            set_alpha(hwnd, alpha.round().clamp(0.0, 255.0) as u8);
            thread::sleep(Duration::from_millis(
                (FADE_DURATION_MS / FADE_STEPS).max(1) as u64,
            ));
        }

        if hide_after && FADE_EPOCH.load(Ordering::SeqCst) == epoch {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
    });
}

fn set_alpha(hwnd: HWND, alpha: u8) {
    unsafe {
        let _ = SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, alpha, LWA_COLORKEY | LWA_ALPHA);
    }
}

fn mouse_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}
