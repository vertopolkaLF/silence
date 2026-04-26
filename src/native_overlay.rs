use std::{collections::HashMap, ffi::c_void, mem::size_of, ptr::null_mut, sync::Mutex};

use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use resvg::{tiny_skia, usvg};
use windows::{
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM},
        Graphics::Gdi::{
            AC_SRC_ALPHA, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
            CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateCompatibleDC, CreateDIBSection,
            CreateFontW, CreatePen, CreateSolidBrush, DEFAULT_CHARSET, DEFAULT_PITCH,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, DrawTextW, Ellipse, FF_DONTCARE, FW_MEDIUM,
            FW_NORMAL, GetTextExtentPoint32W, HDC, OUT_DEFAULT_PRECIS, PS_SOLID, RoundRect,
            SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
        },
        UI::HiDpi::GetDpiForWindow,
        UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, GWL_EXSTYLE, GetCursorPos,
            GetSystemMetrics, GetWindowLongW, HWND_TOPMOST, IDC_ARROW, IDC_SIZEALL, KillTimer,
            LoadCursorW, RegisterClassW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SWP_FRAMECHANGED,
            SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_SHOWWINDOW, SetCursor,
            SetTimer, SetWindowLongW, SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow,
            WM_ERASEBKGND, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};

const CLASS_NAME: PCWSTR = w!("SilenceV2Overlay");
const FADE_DURATION_MS: u32 = 300;
const FADE_STEPS: u32 = 18;
const CONTENT_TRANSITION_MS: u32 = 300;
const CONTENT_TRANSITION_STEPS: u32 = 18;
const ID_CONTENT_TRANSITION_TIMER: usize = 30;
const ID_WINDOW_FADE_TIMER: usize = 31;

static OVERLAY: Lazy<Mutex<Option<NativeOverlay>>> = Lazy::new(|| Mutex::new(None));
static ICON_MASK_CACHE: Lazy<Mutex<HashMap<(String, bool, u32), Vec<u8>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
struct OverlayMetrics {
    padding: i32,
    right_padding: i32,
    gap: i32,
    icon_size: i32,
    icon_font_size: i32,
    text_font_size: i32,
    text_y_offset: i32,
}

struct NativeOverlay {
    hwnd: HWND,
    muted: bool,
    transition_from_muted: Option<bool>,
    transition_progress: f64,
    transition_step: u32,
    transition_start_width: i32,
    transition_target_width: i32,
    window_alpha: f64,
    fade_from_alpha: f64,
    fade_to_alpha: f64,
    fade_step: u32,
    fade_hide_after: bool,
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

    let mut native = NativeOverlay {
        hwnd,
        muted,
        transition_from_muted: None,
        transition_progress: 1.0,
        transition_step: 0,
        transition_start_width: 48,
        transition_target_width: 48,
        window_alpha: 1.0,
        fade_from_alpha: 1.0,
        fade_to_alpha: 1.0,
        fade_step: 0,
        fade_hide_after: false,
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
        let next_muted = displayed_mute_state(muted, settings);
        let previous_muted = overlay.muted;
        overlay.settings = settings.clone();
        if previous_muted != next_muted {
            overlay.start_content_transition(previous_muted, next_muted);
        } else {
            overlay.muted = next_muted;
        }
        overlay.apply_layout();
        overlay.repaint();
    }
}

fn displayed_mute_state(muted: bool, settings: &crate::OverlayConfig) -> bool {
    match settings.visibility.as_str() {
        "WhenMuted" => true,
        "WhenUnmuted" => false,
        _ => muted,
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
            self.start_window_fade(0.0, 1.0, false);
            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                self.x,
                self.y,
                self.width,
                self.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            self.repaint();
        }
    }

    fn hide(&mut self) {
        unsafe {
            if !self.visible {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                return;
            }

            self.visible = false;
            self.start_window_fade(self.window_alpha, 0.0, true);
        }
    }

    fn apply_layout(&mut self) {
        let target_width = self.target_width_for(self.muted);
        if self.transition_from_muted.is_some() {
            self.transition_target_width = target_width;
            self.width = lerp_i32(
                self.transition_start_width,
                self.transition_target_width,
                width_transition_progress(self.transition_progress),
            );
        } else {
            self.width = target_width;
        }

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

    fn target_width_for(&mut self, muted: bool) -> i32 {
        let scale = self.native_scale();
        if self.settings.variant == "Dot" {
            self.height = (24.0 * scale).round().max(4.0) as i32;
            return self.height;
        }

        self.height = (48.0 * scale).round() as i32;
        if !self.settings.show_text {
            return self.height;
        }

        let metrics = overlay_metrics(self.height);
        let text_width = measure_text_width(overlay_label(muted), metrics.text_font_size);
        metrics.padding + metrics.icon_size + metrics.gap + text_width + metrics.right_padding
    }

    fn start_content_transition(&mut self, from_muted: bool, to_muted: bool) {
        self.transition_from_muted = Some(from_muted);
        self.transition_progress = 0.0;
        self.transition_step = 0;
        self.transition_start_width = self.width.max(1);
        self.muted = to_muted;
        self.transition_target_width = self.target_width_for(to_muted);
        unsafe {
            let _ = KillTimer(self.hwnd, ID_CONTENT_TRANSITION_TIMER);
            let _ = SetTimer(
                self.hwnd,
                ID_CONTENT_TRANSITION_TIMER,
                (CONTENT_TRANSITION_MS / CONTENT_TRANSITION_STEPS).max(1),
                None,
            );
        }
    }

    fn process_content_transition(&mut self) {
        if self.transition_from_muted.is_none() {
            unsafe {
                let _ = KillTimer(self.hwnd, ID_CONTENT_TRANSITION_TIMER);
            }
            return;
        }

        self.transition_step += 1;
        let progress =
            (self.transition_step as f64 / CONTENT_TRANSITION_STEPS as f64).clamp(0.0, 1.0);
        self.transition_progress = progress;
        self.apply_layout();
        self.repaint();

        if self.transition_step >= CONTENT_TRANSITION_STEPS {
            self.transition_from_muted = None;
            self.transition_progress = 1.0;
            self.transition_step = 0;
            self.transition_start_width = self.width;
            self.transition_target_width = self.width;
            self.apply_layout();
            self.repaint();
            unsafe {
                let _ = KillTimer(self.hwnd, ID_CONTENT_TRANSITION_TIMER);
            }
        }
    }

    fn start_window_fade(&mut self, from: f64, to: f64, hide_after: bool) {
        self.window_alpha = from.clamp(0.0, 1.0);
        self.fade_from_alpha = self.window_alpha;
        self.fade_to_alpha = to.clamp(0.0, 1.0);
        self.fade_step = 0;
        self.fade_hide_after = hide_after;
        unsafe {
            let _ = KillTimer(self.hwnd, ID_WINDOW_FADE_TIMER);
            let _ = SetTimer(
                self.hwnd,
                ID_WINDOW_FADE_TIMER,
                (FADE_DURATION_MS / FADE_STEPS).max(1),
                None,
            );
        }
    }

    fn process_window_fade(&mut self) {
        self.fade_step += 1;
        let progress = (self.fade_step as f64 / FADE_STEPS as f64).clamp(0.0, 1.0);
        self.window_alpha = self.fade_from_alpha
            + (self.fade_to_alpha - self.fade_from_alpha) * ease_in_out(progress);
        self.repaint();

        if self.fade_step >= FADE_STEPS {
            self.window_alpha = self.fade_to_alpha;
            self.repaint();
            unsafe {
                let _ = KillTimer(self.hwnd, ID_WINDOW_FADE_TIMER);
                if self.fade_hide_after {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
            }
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

    fn native_scale(&self) -> f64 {
        let user_scale = self.settings.scale.clamp(10, 400) as f64 / 100.0;
        user_scale * dpi_scale(self.hwnd)
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
        self.render_layered();
    }

    fn render_layered(&self) {
        if self.width <= 0 || self.height <= 0 {
            return;
        }

        unsafe {
            let screen_hdc = CreateCompatibleDC(None);
            if screen_hdc.0.is_null() {
                return;
            }

            let mut bits: *mut c_void = null_mut();
            let mut info = BITMAPINFO::default();
            info.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: self.width,
                biHeight: -self.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            };
            let bitmap =
                match CreateDIBSection(screen_hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(bitmap) => bitmap,
                    Err(_) => {
                        let _ = DeleteDC(screen_hdc);
                        return;
                    }
                };

            let old_bitmap = SelectObject(screen_hdc, bitmap);
            clear_argb(bits, self.width, self.height);
            let hdc = screen_hdc;

            if self.settings.variant == "Dot" {
                let dot_color = if self.transition_from_muted.is_some() {
                    transition_color(
                        state_accent(self.transition_from_muted.unwrap_or(self.muted)),
                        state_accent(self.muted),
                        content_in_opacity(self.transition_progress),
                    )
                } else {
                    state_accent(self.muted)
                };
                let content_opacity = self.settings.content_opacity.clamp(20, 100);
                let accent = colorref_tuple(blend_rgb(
                    (0, 0, 0),
                    dot_color,
                    content_opacity as f64 / 100.0,
                ));
                let brush = CreateSolidBrush(accent);
                let pen = CreatePen(PS_SOLID, 0, accent);
                let old_brush = SelectObject(hdc, brush);
                let old_pen = SelectObject(hdc, pen);
                let _ = Ellipse(hdc, 0, 0, self.width, self.height);
                let _ = SelectObject(hdc, old_pen);
                let _ = SelectObject(hdc, old_brush);
                let _ = DeleteObject(pen);
                let _ = DeleteObject(brush);
                premultiply_argb(
                    bits,
                    self.width,
                    self.height,
                    (self.settings.content_opacity.clamp(20, 100) as f64 / 100.0)
                        * self.window_alpha,
                );
                self.update_layered(hdc);
                let _ = SelectObject(screen_hdc, old_bitmap);
                let _ = DeleteObject(bitmap);
                let _ = DeleteDC(screen_hdc);
                return;
            }

            let dark_background = self.settings.background_style != "Light";
            let background_rgb = if dark_background {
                (30, 30, 30)
            } else {
                (255, 255, 255)
            };
            let border = if dark_background {
                colorref(68, 68, 68)
            } else {
                colorref(218, 218, 218)
            };
            let corner_radius =
                (self.settings.border_radius.min(24) as f64 * self.native_scale()).round() as i32;
            let corner_diameter = (corner_radius * 2).min(self.height).max(0);
            let brush = CreateSolidBrush(background_fill(
                background_rgb,
                self.settings.background_opacity,
            ));
            let pen = if self.settings.show_border || self.positioning {
                let border_color = if self.positioning {
                    colorref(0, 120, 215)
                } else {
                    border
                };
                CreatePen(PS_SOLID, if self.positioning { 2 } else { 1 }, border_color)
            } else {
                CreatePen(
                    PS_SOLID,
                    0,
                    background_fill(background_rgb, self.settings.background_opacity),
                )
            };
            let _ = SetBkMode(hdc, TRANSPARENT);
            let old_brush = SelectObject(hdc, brush);
            let old_pen = SelectObject(hdc, pen);
            let _ = RoundRect(
                hdc,
                0,
                0,
                self.width,
                self.height,
                corner_diameter,
                corner_diameter,
            );
            let _ = SelectObject(hdc, old_pen);
            let _ = SelectObject(hdc, old_brush);
            let _ = DeleteObject(pen);
            let _ = DeleteObject(brush);

            let metrics = overlay_metrics(self.height);
            finalize_overlay_argb(
                bits,
                self.width,
                self.height,
                background_rgb,
                self.settings.background_opacity,
                self.window_alpha,
            );

            if let Some(from_muted) = self.transition_from_muted {
                let outgoing_opacity = content_out_opacity(self.transition_progress);
                let incoming_opacity = content_in_opacity(self.transition_progress);
                self.compose_content(
                    bits,
                    from_muted,
                    outgoing_opacity,
                    dark_background,
                    &metrics,
                );
                self.compose_content(
                    bits,
                    self.muted,
                    incoming_opacity,
                    dark_background,
                    &metrics,
                );
            } else {
                self.compose_content(bits, self.muted, 1.0, dark_background, &metrics);
            }

            self.update_layered(hdc);
            let _ = SelectObject(screen_hdc, old_bitmap);
            let _ = DeleteObject(bitmap);
            let _ = DeleteDC(screen_hdc);
        }
    }

    fn update_layered(&self, hdc: HDC) {
        unsafe {
            let dst = POINT {
                x: self.x,
                y: self.y,
            };
            let size = SIZE {
                cx: self.width,
                cy: self.height,
            };
            let src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: 0,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&dst),
                Some(&size),
                hdc,
                Some(&src),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            );
        }
    }

    fn compose_content(
        &self,
        target_bits: *mut c_void,
        muted: bool,
        opacity_factor: f64,
        dark_background: bool,
        metrics: &OverlayMetrics,
    ) {
        let opacity = (self.settings.content_opacity.clamp(20, 100) as f64 / 100.0)
            * opacity_factor.clamp(0.0, 1.0)
            * self.window_alpha;
        if opacity <= 0.0 {
            return;
        }

        let icon_color = if self.settings.icon_style == "Monochrome" {
            if dark_background {
                (255, 255, 255)
            } else {
                (0, 0, 0)
            }
        } else {
            state_accent(muted)
        };

        let icon_left = if self.settings.show_text {
            metrics.padding
        } else {
            (self.width - metrics.icon_size) / 2
        };
        let icon_top = (self.height - metrics.icon_size) / 2;
        if let Some(mask) = overlay_icon_mask(
            &self.settings.icon_pair,
            muted,
            metrics.icon_size.max(1) as u32,
        ) {
            composite_masked_subrect(
                target_bits,
                self.width,
                self.height,
                &mask,
                metrics.icon_size.max(1),
                metrics.icon_size.max(1),
                icon_left.max(0),
                icon_top.max(0),
                icon_color,
                opacity,
            );
        } else if let Some(mask) = render_text_mask(self.width, self.height, |hdc| unsafe {
            let icon_face = crate::wide("Segoe Fluent Icons");
            let icon_font = CreateFontW(
                metrics.icon_font_size,
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
                ANTIALIASED_QUALITY.0 as u32,
                (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                PCWSTR(icon_face.as_ptr()),
            );
            let old_font = SelectObject(hdc, icon_font);
            let glyph = if muted { "\u{F781}" } else { "\u{E720}" };
            let mut icon_text: Vec<u16> = glyph.encode_utf16().collect();
            let mut icon_rect = RECT {
                left: if self.settings.show_text {
                    metrics.padding
                } else {
                    0
                },
                top: 0,
                right: if self.settings.show_text {
                    metrics.padding + metrics.icon_size
                } else {
                    self.width
                },
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
        }) {
            composite_masked_color(
                target_bits,
                self.width,
                self.height,
                &mask,
                icon_color,
                opacity,
            );
        }

        if self.settings.show_text {
            let text_color = if dark_background {
                (245, 245, 245)
            } else {
                (18, 18, 18)
            };
            if let Some(mask) = render_text_mask(self.width, self.height, |hdc| unsafe {
                let text_face = crate::wide("Segoe UI");
                let text_font = CreateFontW(
                    metrics.text_font_size,
                    0,
                    0,
                    0,
                    FW_NORMAL.0 as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET.0 as u32,
                    OUT_DEFAULT_PRECIS.0 as u32,
                    CLIP_DEFAULT_PRECIS.0 as u32,
                    ANTIALIASED_QUALITY.0 as u32,
                    (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
                    PCWSTR(text_face.as_ptr()),
                );
                let old_font = SelectObject(hdc, text_font);
                let mut label: Vec<u16> = overlay_label(muted).encode_utf16().collect();
                let mut text_rect = RECT {
                    left: metrics.padding + metrics.icon_size + metrics.gap,
                    top: metrics.text_y_offset,
                    right: self.width - metrics.right_padding,
                    bottom: self.height + metrics.text_y_offset,
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
            }) {
                composite_masked_color(
                    target_bits,
                    self.width,
                    self.height,
                    &mask,
                    text_color,
                    opacity,
                );
            }
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
                overlay.repaint();
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
        WM_TIMER if wparam.0 == ID_CONTENT_TRANSITION_TIMER => {
            if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
                overlay.process_content_transition();
            }
            LRESULT(0)
        }
        WM_TIMER if wparam.0 == ID_WINDOW_FADE_TIMER => {
            if let Some(overlay) = OVERLAY.lock().unwrap().as_mut() {
                overlay.process_window_fade();
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

fn axis_to_percent(position: i32, screen: i32, size: i32) -> f64 {
    let available = (screen - size).max(1) as f64;
    (position as f64 * 100.0 / available).clamp(0.0, 100.0)
}

fn background_fill(rgb: (u8, u8, u8), opacity: u8) -> COLORREF {
    let _ = opacity;
    colorref_tuple(rgb)
}

fn overlay_metrics(height: i32) -> OverlayMetrics {
    let scale = height as f64 / 48.0;
    OverlayMetrics {
        padding: (10.0 * scale).round().max(4.0) as i32,
        right_padding: (16.0 * scale).round().max(6.0) as i32,
        gap: (10.0 * scale).round().max(4.0) as i32,
        icon_size: (28.0 * scale).round().max(10.0) as i32,
        icon_font_size: -((height as f64 * 0.58).round() as i32),
        text_font_size: -((height as f64 * 0.33).round() as i32),
        text_y_offset: (-((1.5 * scale).round() as i32)).min(-1),
    }
}

fn overlay_label(muted: bool) -> &'static str {
    if muted {
        "Microphone muted"
    } else {
        "Microphone on"
    }
}

fn measure_text_width(text: &str, font_size: i32) -> i32 {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.0.is_null() {
            return fallback_text_width(text, font_size);
        }

        let text_face = crate::wide("Segoe UI");
        let font = CreateFontW(
            font_size,
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
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
        let old_font = SelectObject(hdc, font);
        let text_utf16: Vec<u16> = text.encode_utf16().collect();
        let mut size = SIZE::default();
        let measured = GetTextExtentPoint32W(hdc, &text_utf16, &mut size).as_bool();
        let _ = SelectObject(hdc, old_font);
        let _ = DeleteObject(font);
        let _ = DeleteDC(hdc);

        if measured {
            size.cx.max(1)
        } else {
            fallback_text_width(text, font_size)
        }
    }
}

fn fallback_text_width(text: &str, font_size: i32) -> i32 {
    let px = font_size.abs().max(1) as f64;
    (text.chars().count() as f64 * px * 0.56).round() as i32
}

fn render_text_mask(width: i32, height: i32, draw: impl FnOnce(HDC)) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 {
        return None;
    }

    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.0.is_null() {
            return None;
        }

        let mut bits: *mut c_void = null_mut();
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        };

        let bitmap = match CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(bitmap) => bitmap,
            Err(_) => {
                let _ = DeleteDC(hdc);
                return None;
            }
        };

        let old_bitmap = SelectObject(hdc, bitmap);
        clear_argb_to(bits, width, height, 0);
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, colorref(255, 255, 255));
        draw(hdc);

        let mask = if bits.is_null() {
            None
        } else {
            let pixels = std::slice::from_raw_parts(bits as *const u32, (width * height) as usize);
            Some(
                pixels
                    .iter()
                    .map(|pixel| {
                        let [b, g, r, _] = pixel.to_le_bytes();
                        r.max(g).max(b)
                    })
                    .collect(),
            )
        };

        let _ = SelectObject(hdc, old_bitmap);
        let _ = DeleteObject(bitmap);
        let _ = DeleteDC(hdc);
        mask
    }
}

fn clear_argb(bits: *mut c_void, width: i32, height: i32) {
    clear_argb_to(bits, width, height, 0x00ff00ff);
}

fn clear_argb_to(bits: *mut c_void, width: i32, height: i32, value: u32) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }

    unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        pixels.fill(value);
    }
}

fn composite_masked_color(
    target_bits: *mut c_void,
    width: i32,
    height: i32,
    mask: &[u8],
    color: (u8, u8, u8),
    opacity: f64,
) {
    if target_bits.is_null() || width <= 0 || height <= 0 {
        return;
    }

    let pixel_count = (width * height) as usize;
    if mask.len() < pixel_count {
        return;
    }

    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }

    unsafe {
        let target = std::slice::from_raw_parts_mut(target_bits as *mut u32, pixel_count);
        for (dst, coverage) in target.iter_mut().zip(mask.iter()) {
            let src_alpha = (*coverage as f64 / 255.0) * opacity;
            if src_alpha <= 0.0 {
                continue;
            }

            let [dst_b, dst_g, dst_r, dst_a] = dst.to_le_bytes();
            let inv_alpha = 1.0 - src_alpha;
            let out_a = src_alpha + (dst_a as f64 / 255.0) * inv_alpha;
            let out_r = color.0 as f64 * src_alpha + dst_r as f64 * inv_alpha;
            let out_g = color.1 as f64 * src_alpha + dst_g as f64 * inv_alpha;
            let out_b = color.2 as f64 * src_alpha + dst_b as f64 * inv_alpha;

            *dst = u32::from_le_bytes([
                out_b.round().clamp(0.0, 255.0) as u8,
                out_g.round().clamp(0.0, 255.0) as u8,
                out_r.round().clamp(0.0, 255.0) as u8,
                (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
            ]);
        }
    }
}

fn composite_masked_subrect(
    target_bits: *mut c_void,
    target_width: i32,
    target_height: i32,
    mask: &[u8],
    mask_width: i32,
    mask_height: i32,
    offset_x: i32,
    offset_y: i32,
    color: (u8, u8, u8),
    opacity: f64,
) {
    if target_bits.is_null()
        || target_width <= 0
        || target_height <= 0
        || mask_width <= 0
        || mask_height <= 0
    {
        return;
    }

    let pixel_count = (mask_width * mask_height) as usize;
    if mask.len() < pixel_count {
        return;
    }

    let opacity = opacity.clamp(0.0, 1.0);
    if opacity <= 0.0 {
        return;
    }

    unsafe {
        let target = std::slice::from_raw_parts_mut(
            target_bits as *mut u32,
            (target_width * target_height) as usize,
        );
        for mask_y in 0..mask_height {
            let dst_y = offset_y + mask_y;
            if !(0..target_height).contains(&dst_y) {
                continue;
            }
            for mask_x in 0..mask_width {
                let dst_x = offset_x + mask_x;
                if !(0..target_width).contains(&dst_x) {
                    continue;
                }

                let mask_index = (mask_y * mask_width + mask_x) as usize;
                let coverage = mask[mask_index];
                let src_alpha = (coverage as f64 / 255.0) * opacity;
                if src_alpha <= 0.0 {
                    continue;
                }

                let dst_index = (dst_y * target_width + dst_x) as usize;
                let dst = &mut target[dst_index];
                let [dst_b, dst_g, dst_r, dst_a] = dst.to_le_bytes();
                let inv_alpha = 1.0 - src_alpha;
                let out_a = src_alpha + (dst_a as f64 / 255.0) * inv_alpha;
                let out_r = color.0 as f64 * src_alpha + dst_r as f64 * inv_alpha;
                let out_g = color.1 as f64 * src_alpha + dst_g as f64 * inv_alpha;
                let out_b = color.2 as f64 * src_alpha + dst_b as f64 * inv_alpha;

                *dst = u32::from_le_bytes([
                    out_b.round().clamp(0.0, 255.0) as u8,
                    out_g.round().clamp(0.0, 255.0) as u8,
                    out_r.round().clamp(0.0, 255.0) as u8,
                    (out_a * 255.0).round().clamp(0.0, 255.0) as u8,
                ]);
            }
        }
    }
}

fn overlay_icon_mask(icon_pair: &str, muted: bool, size: u32) -> Option<Vec<u8>> {
    let key = (icon_pair.to_string(), muted, size);
    if let Some(mask) = ICON_MASK_CACHE.lock().unwrap().get(&key).cloned() {
        return Some(mask);
    }

    let svg = crate::overlay_icons::overlay_icon_svg(icon_pair, muted);
    let tree = usvg::Tree::from_str(svg, &usvg::Options::default()).ok()?;
    let svg_size = tree.size().to_int_size();
    let scale = (size as f32 / svg_size.width() as f32).min(size as f32 / svg_size.height() as f32);
    let mut pixmap = tiny_skia::Pixmap::new(size, size)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mask = pixmap
        .take_demultiplied()
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect::<Vec<_>>();
    ICON_MASK_CACHE.lock().unwrap().insert(key, mask.clone());
    Some(mask)
}

fn finalize_overlay_argb(
    bits: *mut c_void,
    width: i32,
    height: i32,
    background_rgb: (u8, u8, u8),
    background_opacity: u8,
    window_alpha: f64,
) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }

    let background_alpha = (background_opacity.min(100) as f64 / 100.0) * window_alpha;
    unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        for pixel in pixels {
            let [b, g, r, _] = pixel.to_le_bytes();
            if r == 255 && g == 0 && b == 255 {
                *pixel = 0;
                continue;
            }

            let is_background = color_distance_sq((r, g, b), background_rgb) <= 3;
            let alpha = if is_background {
                background_alpha
            } else {
                window_alpha
            };
            *pixel = premultiply_pixel(r, g, b, alpha);
        }
    }
}

fn premultiply_argb(bits: *mut c_void, width: i32, height: i32, alpha: f64) {
    if bits.is_null() || width <= 0 || height <= 0 {
        return;
    }

    unsafe {
        let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (width * height) as usize);
        for pixel in pixels {
            let [b, g, r, _] = pixel.to_le_bytes();
            if r == 255 && g == 0 && b == 255 {
                *pixel = 0;
            } else {
                *pixel = premultiply_pixel(r, g, b, alpha);
            }
        }
    }
}

fn premultiply_pixel(r: u8, g: u8, b: u8, alpha: f64) -> u32 {
    let alpha = alpha.clamp(0.0, 1.0);
    let a = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
    let r = (r as f64 * alpha).round().clamp(0.0, 255.0) as u8;
    let g = (g as f64 * alpha).round().clamp(0.0, 255.0) as u8;
    let b = (b as f64 * alpha).round().clamp(0.0, 255.0) as u8;
    u32::from_le_bytes([b, g, r, a])
}

fn color_distance_sq(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let dr = a.0 as i32 - b.0 as i32;
    let dg = a.1 as i32 - b.1 as i32;
    let db = a.2 as i32 - b.2 as i32;
    (dr * dr + dg * dg + db * db) as u32
}

fn dpi_scale(hwnd: HWND) -> f64 {
    let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
    dpi as f64 / 96.0
}

fn mouse_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000) != 0 }
}

fn colorref(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF(r as u32 | ((g as u32) << 8) | ((b as u32) << 16))
}

fn colorref_tuple((r, g, b): (u8, u8, u8)) -> COLORREF {
    colorref(r, g, b)
}

fn state_accent(muted: bool) -> (u8, u8, u8) {
    if muted { (220, 53, 69) } else { (40, 167, 69) }
}

fn transition_color(from: (u8, u8, u8), to: (u8, u8, u8), progress: f64) -> (u8, u8, u8) {
    (
        lerp_u8(from.0, to.0, progress),
        lerp_u8(from.1, to.1, progress),
        lerp_u8(from.2, to.2, progress),
    )
}

fn blend_rgb(from: (u8, u8, u8), to: (u8, u8, u8), amount: f64) -> (u8, u8, u8) {
    (
        lerp_u8(from.0, to.0, amount),
        lerp_u8(from.1, to.1, amount),
        lerp_u8(from.2, to.2, amount),
    )
}

fn lerp_i32(from: i32, to: i32, progress: f64) -> i32 {
    (from as f64 + (to - from) as f64 * progress.clamp(0.0, 1.0)).round() as i32
}

fn lerp_u8(from: u8, to: u8, progress: f64) -> u8 {
    (from as f64 + (to as f64 - from as f64) * progress.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8
}

fn width_transition_progress(progress: f64) -> f64 {
    ease_in_out((progress / 0.5).clamp(0.0, 1.0))
}

fn content_out_opacity(progress: f64) -> f64 {
    if progress < 0.5 {
        1.0 - ease_in_out(progress / 0.5)
    } else {
        0.0
    }
}

fn content_in_opacity(progress: f64) -> f64 {
    if progress < 0.5 {
        0.0
    } else {
        let local = (progress - 0.5) / 0.5;
        ease_in_out(local)
    }
}

fn ease_in_out(progress: f64) -> f64 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}
