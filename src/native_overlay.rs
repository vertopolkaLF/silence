use std::sync::Mutex;

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
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GetSystemMetrics, HWND_TOPMOST,
            IDC_ARROW, LWA_ALPHA, LWA_COLORKEY, LoadCursorW, RegisterClassW, SM_CXSCREEN,
            SM_CYSCREEN, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW,
            SetLayeredWindowAttributes, SetWindowPos, ShowWindow, WM_ERASEBKGND, WM_PAINT,
            WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
            WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};

const CLASS_NAME: PCWSTR = w!("SilenceV2Overlay");
const TRANSPARENT_KEY: COLORREF = COLORREF(0x00ff00ff);

static OVERLAY: Lazy<Mutex<Option<NativeOverlay>>> = Lazy::new(|| Mutex::new(None));

struct NativeOverlay {
    hwnd: HWND,
    muted: bool,
    settings: crate::OverlayConfig,
    width: i32,
    height: i32,
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
        SetLayeredWindowAttributes(hwnd, TRANSPARENT_KEY, 245, LWA_COLORKEY | LWA_ALPHA)
            .context("configure overlay transparency")?;
    }

    let mut native = NativeOverlay {
        hwnd,
        muted,
        settings: settings.clone(),
        width: 48,
        height: 48,
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
    if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
        unsafe {
            let _ = ShowWindow(overlay.hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(
                overlay.hwnd,
                HWND_TOPMOST,
                overlay.x(),
                overlay.y(),
                overlay.width,
                overlay.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }
    }
}

pub fn hide() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_ref() {
        unsafe {
            let _ = ShowWindow(overlay.hwnd, SW_HIDE);
        }
    }
}

pub fn reposition() {
    if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
        overlay.apply_layout();
    }
}

pub fn destroy() {
    if let Some(overlay) = OVERLAY.lock().unwrap().take() {
        unsafe {
            let _ = DestroyWindow(overlay.hwnd);
        }
    }
}

impl NativeOverlay {
    fn apply_layout(&mut self) {
        let scale = self.settings.scale.clamp(10, 400) as f64 / 100.0;
        self.height = (48.0 * scale).round() as i32;
        self.width = if self.settings.show_text {
            (210.0 * scale).round() as i32
        } else {
            self.height
        };

        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x(),
                self.y(),
                self.width,
                self.height,
                SWP_NOACTIVATE,
            );
        }
    }

    fn x(&self) -> i32 {
        let screen = unsafe { GetSystemMetrics(SM_CXSCREEN) }.max(self.width);
        percent_to_axis(self.settings.position_x, screen, self.width)
    }

    fn y(&self) -> i32 {
        let screen = unsafe { GetSystemMetrics(SM_CYSCREEN) }.max(self.height);
        percent_to_axis(self.settings.position_y, screen, self.height)
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
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn percent_to_axis(percent: f64, screen: i32, size: i32) -> i32 {
    let available = (screen - size).max(0) as f64;
    (available * percent.clamp(0.0, 100.0) / 100.0).round() as i32
}

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}
