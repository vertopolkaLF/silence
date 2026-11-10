//! Volume popup above the tray icon: the device slider plus one slider per
//! app, like EarTrumpet, drawn as a Windows 11 flyout on fake Mica.
//!
//! Runs on the volume OSD's GPUI thread as a second, focusable window. Device
//! volume writes go back to the tray thread as window messages; the tray then
//! sends the new state here through `PopupCommand::Update`. App volumes are
//! read and written here directly through [`crate::volume_sessions`], polled
//! while the popup is open.

use super::{motion::Motion, theme::System};
use crate::settings_ui::anim::{self, Easing, Transition};
use crate::volume_popup::{CARD_W, GUTTER, PopupCommand, VolumePopupState};
use crate::volume_sessions::{self, AppVolume};
use gpui::{
    App, Bounds, BoxShadow, Context, ElementId, FocusHandle, FontWeight, Image, ImageFormat,
    InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    ObjectFit, ParentElement, Render, Rgba, ScrollDelta, ScrollWheelEvent,
    StatefulInteractiveElement, Styled, Task, Window, WindowBackgroundAppearance, WindowBounds,
    WindowHandle, WindowKind, WindowOptions, div, img, point, prelude::*, px, rgb, rgba, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::{HWND, RECT},
    UI::WindowsAndMessaging::{
        HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos,
    },
};

// Row geometry (logical pixels). Every row shares the same columns so the
// sliders line up: [pad][button][gap][slider][gap][number][pad].
const PAD_LEFT: f32 = 8.0;
const PAD_RIGHT: f32 = 12.0;
const BUTTON: f32 = 36.0;
const GAP: f32 = 8.0;
const NUMBER_W: f32 = 26.0;
/// Window-relative x of every slider's left edge.
const TRACK_X: f32 = GUTTER + PAD_LEFT + BUTTON + GAP;
/// Slider beside a number column, and a labeled row's full-width slider.
const TRACK_W: f32 = CARD_W - PAD_LEFT - BUTTON - GAP - GAP - NUMBER_W - PAD_RIGHT;
const TRACK_W_WIDE: f32 = CARD_W - PAD_LEFT - BUTTON - GAP - PAD_RIGHT;
const SLIDER_H: f32 = 24.0;
const THUMB: f32 = 20.0;
const COMPACT_ROW_H: f32 = 40.0;
/// Name-and-number line of a labeled row.
const LABEL_H: f32 = 18.0;
const LABELED_ROW_H: f32 = 4.0 + LABEL_H + SLIDER_H + 4.0;
const HEADER_H: f32 = 30.0;
const DIVIDER_H: f32 = 13.0;
const PAD_Y: f32 = 6.0;
const APP_ICON: f32 = 20.0;
const DEVICE_GLYPH: f32 = 16.0;
const RADIUS: f32 = 8.0;
const APP_STEP: i32 = 2;

const APPEAR_MS: u64 = 160;
const CLOSE_MS: u64 = 130;
const RESIZE_MS: u64 = 200;
const POLL: Duration = Duration::from_millis(500);
/// Thumb at rest: a bare accent dot. The ring grows around it on hover.
const THUMB_DOT: f32 = 12.0;
/// Slider level glide on wheel, keys and outside changes.
const LEVEL: Transition = Transition::new(140, Easing::Bezier(0.33, 1., 0.68, 1.));
/// Hover backgrounds and the thumb's inner dot.
const HOVER: Transition = Transition::new(100, Easing::EaseOut);
const PRESS: Transition = Transition::new(80, Easing::EaseOut);
/// Closes on its own if Windows never let it take focus.
const UNFOCUSED_TIMEOUT: Duration = Duration::from_secs(6);

/// Row style from Settings → Volume → Tray icon → Slider popup.
#[derive(Clone, Copy, PartialEq)]
struct Layout {
    /// Names above full-width sliders instead of one line per row.
    labeled: bool,
    /// Device name as a header above the device row.
    header: bool,
    /// Device row under the app list instead of above it.
    device_at_bottom: bool,
}

impl Layout {
    fn load() -> Self {
        let config = crate::STATE.lock().unwrap().volume.popup.clone();
        Self {
            labeled: config.layout == crate::VolumePopupLayout::Labeled,
            header: config.device_header,
            device_at_bottom: config.device_at_bottom,
        }
    }

    /// With a header the device row needs no name of its own.
    fn device_labeled(self) -> bool {
        self.labeled && !self.header
    }

    fn device_row_h(self) -> f32 {
        if self.device_labeled() { LABELED_ROW_H } else { COMPACT_ROW_H }
    }

    fn app_row_h(self) -> f32 {
        if self.labeled { LABELED_ROW_H } else { COMPACT_ROW_H }
    }

    fn header_h(self) -> f32 {
        if self.header { HEADER_H } else { 0.0 }
    }

    /// Space under the last row: its button sits as far from the card's
    /// bottom edge as every button does from the left edge.
    fn pad_bottom(self, last_row_h: f32) -> f32 {
        (PAD_LEFT - (last_row_h - BUTTON) / 2.0).max(0.0)
    }

    /// Last row on the card: the device, or the last app when the device is on top.
    fn last_row_h(self, apps: usize) -> f32 {
        if apps == 0 || self.device_at_bottom {
            self.device_row_h()
        } else {
            self.app_row_h()
        }
    }

    /// Everything but the app rows, divider included.
    fn fixed_h(self) -> f32 {
        PAD_Y
            + self.pad_bottom(self.last_row_h(1))
            + self.header_h()
            + self.device_row_h()
            + DIVIDER_H
    }

    fn card_height(self, apps: usize) -> f32 {
        let apps = apps.min(self.max_visible_apps());
        let list = if apps == 0 {
            0.0
        } else {
            DIVIDER_H + apps as f32 * self.app_row_h()
        };
        PAD_Y + self.pad_bottom(self.last_row_h(apps)) + self.header_h() + self.device_row_h() + list
    }

    /// As many app rows as fit the screen; more only scroll.
    fn max_visible_apps(self) -> usize {
        (((max_card_height() - self.fixed_h()) / self.app_row_h()).floor() as usize).max(3)
    }

    fn track_w(self, target: &Target) -> f32 {
        let labeled = match target {
            Target::Device => self.device_labeled(),
            Target::App(_) => self.labeled,
        };
        if labeled { TRACK_W_WIDE } else { TRACK_W }
    }
}

/// The card may grow to the primary work area's height. The window is
/// created this tall (GPUI never grows a window past its first size) and
/// shrunk to fit.
fn max_card_height() -> f32 {
    static HEIGHT: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *HEIGHT.get_or_init(|| {
        use windows::Win32::UI::{
            HiDpi::GetDpiForSystem,
            WindowsAndMessaging::{
                SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        };
        let mut work = RECT::default();
        let _ = unsafe {
            SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some((&mut work as *mut RECT).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
        };
        let scale = unsafe { GetDpiForSystem() }.max(96) as f32 / 96.0;
        let work_h = (work.bottom - work.top) as f32 / scale;
        (work_h - 2.0 * GUTTER - 24.0).max(240.0)
    })
}

thread_local! {
    static POPUP: RefCell<Option<(WindowHandle<PopupView>, HWND)>> = const { RefCell::new(None) };
}

pub(crate) fn handle(command: PopupCommand, cx: &mut App) {
    match command {
        PopupCommand::Show { anchor, state } => show(anchor, state, cx),
        PopupCommand::Update(state) => with_view(cx, |view, cx| view.set_state(state, cx)),
        PopupCommand::Close => with_view(cx, |view, cx| view.close(cx)),
        PopupCommand::Prewarm => {
            if POPUP.with(|popup| popup.borrow().is_none()) {
                create(cx);
            }
            with_view(cx, |view, cx| view.refresh_apps(cx));
        }
    }
}

fn with_view(cx: &mut App, f: impl FnOnce(&mut PopupView, &mut Context<PopupView>)) {
    let Some((handle, _)) = POPUP.with(|popup| popup.borrow().clone()) else {
        return;
    };
    let _ = handle.update(cx, |view, _, cx| f(view, cx));
}

fn popup_hwnd() -> Option<HWND> {
    POPUP.with(|popup| popup.borrow().as_ref().map(|(_, hwnd)| *hwnd))
}

fn show(anchor: RECT, state: VolumePopupState, cx: &mut App) {
    let existing = POPUP.with(|popup| popup.borrow().clone());
    let Some((handle, hwnd)) = existing.or_else(|| create(cx)) else {
        return;
    };
    let card_h = handle
        .update(cx, |view, window, cx| {
            view.set_state(state, cx);
            view.reopen(window, cx);
            view.window_card_h
        })
        .ok();
    match card_h {
        Some(card_h) => place_later(cx, hwnd, anchor, card_h, true),
        None => {
            POPUP.with(|popup| popup.borrow_mut().take());
        }
    }
}

/// The popup window lives for the whole session, hidden between uses, so a
/// click only has to move and show it.
fn create(cx: &mut App) -> Option<(WindowHandle<PopupView>, HWND)> {
    let state = VolumePopupState {
        percent: 0,
        muted: false,
        device_name: String::new(),
    };
    let opened = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(0.), px(0.)),
                size(
                    px(CARD_W + GUTTER * 2.0),
                    px(max_card_height() + GUTTER * 2.0),
                ),
            ))),
            titlebar: None,
            focus: false,
            show: false,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        |window, cx| cx.new(|cx| PopupView::new(state, window, cx)),
    );
    let Ok(handle) = opened else {
        eprintln!("volume popup: failed to open window");
        return None;
    };
    let hwnd = handle
        .update(cx, |_, window, _| {
            let RawWindowHandle::Win32(raw) = HasWindowHandle::window_handle(window).ok()?.as_raw()
            else {
                return None;
            };
            Some(HWND(raw.hwnd.get() as *mut _))
        })
        .ok()
        .flatten();
    let Some(hwnd) = hwnd else {
        eprintln!("volume popup: no Win32 handle");
        return None;
    };
    crate::volume_popup::attach(hwnd);
    let _ = handle.update(cx, |view, _, _| {
        view.backdrop = match super::backdrop::Backdrop::new(
            hwnd,
            luminosity(view.system.light),
            super::backdrop::Material::Mica,
        ) {
            Ok(backdrop) => Some(backdrop),
            Err(error) => {
                // Windows 10: the wallpaper-crop fallback stays in use.
                eprintln!("volume popup: no Mica backdrop: {error:#}");
                None
            }
        };
    });
    POPUP.with(|popup| *popup.borrow_mut() = Some((handle, hwnd)));
    Some((handle, hwnd))
}

/// [`place`] once GPUI has released the app. `SetWindowPos` delivers
/// `WM_SIZE` synchronously; inside an update GPUI cannot apply it, keeps the
/// old viewport and draws the card offset and cropped.
fn place_later(cx: &App, hwnd: HWND, anchor: RECT, card_h: f32, activate: bool) {
    cx.spawn(async move |_| place(hwnd, anchor, card_h, activate))
        .detach();
}

/// Moves (and with `activate`, shows and focuses) the window for a card
/// `card_h` tall above `anchor`. Never call it inside a GPUI update.
fn place(hwnd: HWND, anchor: RECT, card_h: f32, activate: bool) {
    let (x, y, width, height) = crate::volume_popup::placement(anchor, card_h);
    unsafe {
        if activate {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(hwnd);
        } else {
            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_NOACTIVATE);
        }
    }
}

/// Lightness the Mica wallpaper is blended to (candle-tray's tuning).
fn luminosity(light: bool) -> u8 {
    if light { 245 } else { 52 }
}

struct Colors {
    surface: Rgba,
    /// Over the fake (wallpaper-crop) Mica.
    mica_tint: Rgba,
    /// Over the real Mica backdrop: WinUI's tint at half opacity.
    glass_tint: Rgba,
    border: Rgba,
    divider: Rgba,
    text: Rgba,
    secondary: Rgba,
    rail: Rgba,
    fill: Rgba,
    thumb_outer: Rgba,
    thumb_border: Rgba,
    hover: Rgba,
    shadow: Rgba,
}

impl Colors {
    fn load(system: System) -> Self {
        let (r, g, b) = system.accent;
        let accent = Rgba {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: 1.0,
        };
        if system.light {
            Self {
                surface: rgb(0xF3F3F3),
                mica_tint: rgba(0xF3F3_F3A6),
                glass_tint: rgba(0xF3F3_F380),
                border: rgba(0x0000_0018),
                divider: rgba(0x0000_0014),
                text: rgb(0x1B1B1B),
                secondary: rgb(0x5F5F5F),
                rail: rgba(0x0000_0026),
                fill: accent,
                thumb_outer: rgb(0xFFFFFF),
                thumb_border: rgba(0x0000_0026),
                hover: rgba(0x0000_000F),
                shadow: rgba(0x0000_0030),
            }
        } else {
            Self {
                surface: rgb(0x2C2C2C),
                mica_tint: rgba(0x2020_20BA),
                glass_tint: rgba(0x2020_2080),
                border: rgba(0xFFFF_FF1A),
                divider: rgba(0xFFFF_FF14),
                text: rgb(0xFFFFFF),
                secondary: rgb(0xC5C5C5),
                rail: rgba(0xFFFF_FF2E),
                fill: accent,
                thumb_outer: rgb(0x454545),
                thumb_border: rgba(0xFFFF_FF17),
                hover: rgba(0xFFFF_FF12),
                shadow: rgba(0x0000_0070),
            }
        }
    }
}

#[derive(Clone, PartialEq)]
enum Target {
    Device,
    App(String),
}

impl Target {
    /// Element id for per-target animation state, stable as rows reorder.
    fn key(&self, part: &str) -> ElementId {
        let name = match self {
            Self::Device => format!("popup-{part}-device"),
            Self::App(key) => format!("popup-{part}-app-{key}"),
        };
        ElementId::Name(name.into())
    }
}

struct AppRow {
    app: AppVolume,
    icon: Option<Arc<Image>>,
    /// Shown level while dragging, ahead of the session's own value.
    local_percent: Option<u8>,
    /// 0..1: the row grows in when an app starts playing and collapses
    /// when it goes away, so the rows below slide instead of jumping.
    presence: Motion,
    /// Session ended; kept until it has collapsed.
    leaving: bool,
}

struct PopupView {
    state: VolumePopupState,
    /// Device level while dragging, ahead of the tray's confirmation.
    local_percent: Option<u8>,
    apps: Vec<AppRow>,
    icons: HashMap<std::path::PathBuf, Option<Arc<Image>>>,
    dragging: Option<Target>,
    hovered_slider: Option<Target>,
    hovered_button: Option<Target>,
    layout: Layout,
    wheel: f32,
    system: System,
    appear: Motion,
    /// Card height on screen, animated when apps come and go.
    card_h: Motion,
    /// Card height the window is currently sized for.
    window_card_h: f32,
    closing: bool,
    was_active: bool,
    focus: FocusHandle,
    timeout: Option<Task<()>>,
    poll: Option<Task<()>>,
    shrink: Option<Task<()>>,
    /// Real Mica under the window; `None` before Windows 11.
    backdrop: Option<super::backdrop::Backdrop>,
    _activation: gpui::Subscription,
}

impl PopupView {
    fn new(state: VolumePopupState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let activation = cx.observe_window_activation(window, |view, window, cx| {
            if window.is_window_active() {
                view.was_active = true;
                view.timeout = None;
            } else if view.was_active {
                view.close(cx);
            }
        });
        let layout = Layout::load();
        let height = layout.card_height(0);
        Self {
            state,
            local_percent: None,
            apps: Vec::new(),
            icons: HashMap::new(),
            dragging: None,
            hovered_slider: None,
            hovered_button: None,
            layout,
            wheel: 0.0,
            system: System::load(),
            // Created hidden (prewarmed); `reopen` shows it.
            appear: Motion::decelerate(0.0, APPEAR_MS),
            card_h: Motion::new(height, RESIZE_MS),
            window_card_h: height,
            closing: true,
            was_active: false,
            focus: cx.focus_handle(),
            timeout: None,
            poll: None,
            shrink: None,
            backdrop: None,
            _activation: activation,
        }
    }

    fn arm_timeout(&mut self, cx: &mut Context<Self>) {
        self.timeout = Some(cx.spawn(async move |view, cx| {
            cx.background_executor().timer(UNFOCUSED_TIMEOUT).await;
            let _ = view.update(cx, |view, cx| {
                if !view.was_active {
                    view.close(cx);
                }
            });
        }));
    }

    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.system = System::load();
        // Settings can change while the popup is hidden; size for them
        // before it is placed.
        self.apply_layout(cx);
        if self.closing {
            self.closing = false;
            self.appear.retarget(1.0, Instant::now());
        }
        self.was_active = false;
        self.arm_timeout(cx);
        self.start_polling(cx);
        crate::volume_popup::mark_open(true);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        self.poll = Some(cx.spawn(async move |view, cx| {
            loop {
                let Ok(()) = view.update(cx, |view, cx| view.refresh_apps(cx)) else {
                    return;
                };
                cx.background_executor().timer(POLL).await;
            }
        }));
    }

    fn apply_layout(&mut self, cx: &mut Context<Self>) {
        let layout = Layout::load();
        if layout != self.layout {
            self.layout = layout;
            self.resize(cx);
        }
    }

    /// Reads the app sessions (and new icons) off the UI thread.
    fn refresh_apps(&mut self, cx: &mut Context<Self>) {
        self.apply_layout(cx);
        let icon_px = (APP_ICON * 2.0) as u32;
        let known: Vec<_> = self.icons.keys().cloned().collect();
        cx.spawn(async move |view, cx| {
            let (apps, icons) = cx
                .background_executor()
                .spawn(async move {
                    let apps = volume_sessions::app_volumes();
                    let icons = apps
                        .iter()
                        .filter_map(|app| app.icon_path.clone())
                        .filter(|path| !known.contains(path))
                        .map(|path| {
                            let png = volume_sessions::app_icon_png(&path, icon_px);
                            (path, png)
                        })
                        .collect::<Vec<_>>();
                    (apps, icons)
                })
                .await;
            let _ = view.update(cx, |view, cx| view.set_apps(apps, icons, cx));
        })
        .detach();
    }

    fn set_apps(
        &mut self,
        apps: Vec<AppVolume>,
        icons: Vec<(std::path::PathBuf, Option<Arc<Vec<u8>>>)>,
        cx: &mut Context<Self>,
    ) {
        for (path, png) in icons {
            let image = png.map(|png| Arc::new(Image::from_bytes(ImageFormat::Png, (*png).clone())));
            self.icons.insert(path, image);
        }
        let now = Instant::now();
        let animate = !self.closing;
        let live_before = self.live_apps();
        let mut rows = Vec::with_capacity(apps.len());
        for app in apps {
            let previous = self.apps.iter().find(|row| row.app.key == app.key);
            let dragging = self.dragging == Some(Target::App(app.key.clone()));
            let local_percent = previous.and_then(|row| row.local_percent);
            let mut presence = match previous {
                Some(row) if animate => row.presence.clone(),
                None if animate => Motion::new(0.0, RESIZE_MS),
                _ => Motion::new(1.0, RESIZE_MS),
            };
            presence.retarget(1.0, now);
            let icon = app
                .icon_path
                .as_ref()
                .and_then(|path| self.icons.get(path).cloned().flatten());
            rows.push(AppRow {
                app,
                icon,
                // Keep the dragged value until the session reports it.
                local_percent: if dragging { local_percent } else { None },
                presence,
                leaving: false,
            });
        }
        // While open, gone apps collapse where they were instead of vanishing.
        if animate {
            for (index, old) in self.apps.iter().enumerate() {
                if rows.iter().any(|row| row.app.key == old.app.key) {
                    continue;
                }
                let mut presence = old.presence.clone();
                presence.retarget(0.0, now);
                rows.insert(
                    index.min(rows.len()),
                    AppRow {
                        app: old.app.clone(),
                        icon: old.icon.clone(),
                        local_percent: None,
                        presence,
                        leaving: true,
                    },
                );
            }
        }
        let changed = rows.len() != self.apps.len()
            || rows.iter().zip(&self.apps).any(|(a, b)| {
                a.app != b.app || a.icon.is_some() != b.icon.is_some() || a.leaving != b.leaving
            });
        self.apps = rows;
        if self.live_apps() != live_before {
            self.resize(cx);
        }
        if changed {
            cx.notify();
        }
    }

    fn live_apps(&self) -> usize {
        self.apps.iter().filter(|row| !row.leaving).count()
    }

    /// Animates the card to fit the rows. The window grows at once and
    /// shrinks after the card has, so the animation is never clipped.
    fn resize(&mut self, cx: &mut Context<Self>) {
        let target = self.layout.card_height(self.live_apps());
        let now = Instant::now();
        if self.closing {
            self.card_h = Motion::new(target, RESIZE_MS);
        } else {
            self.card_h.retarget(target, now);
        }
        let Some(hwnd) = popup_hwnd() else {
            return;
        };
        if target >= self.window_card_h || self.closing {
            self.shrink = None;
            self.window_card_h = target;
            if !self.closing {
                place_later(cx, hwnd, crate::volume_popup::last_anchor(), target, false);
            }
        } else {
            self.shrink = Some(cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(RESIZE_MS + 20))
                    .await;
                let open = view.update(cx, |view, _| {
                    view.window_card_h = target;
                    !view.closing
                });
                // Outside the update; see `place_later`.
                if open.unwrap_or(false) {
                    place(hwnd, crate::volume_popup::last_anchor(), target, false);
                }
            }));
        }
        cx.notify();
    }

    fn set_state(&mut self, state: VolumePopupState, cx: &mut Context<Self>) {
        if self.dragging != Some(Target::Device) {
            self.local_percent = None;
        }
        self.state = state;
        self.apply_layout(cx);
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.dragging = None;
        self.poll = None;
        // Don't replay collapses on the next open.
        self.apps.retain(|row| !row.leaving);
        crate::volume_popup::mark_open(false);
        self.appear.retarget(0.0, Instant::now());
        cx.notify();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(CLOSE_MS + 20))
                .await;
            let _ = view.update(cx, |view, _| {
                if view.closing
                    && let Some(hwnd) = popup_hwnd()
                {
                    // Hide, don't destroy: the next click reuses the window.
                    unsafe {
                        let _ = windows::Win32::UI::WindowsAndMessaging::ShowWindow(
                            hwnd,
                            windows::Win32::UI::WindowsAndMessaging::SW_HIDE,
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn device_percent(&self) -> u8 {
        self.local_percent.unwrap_or(self.state.percent).min(100)
    }

    fn set_from_x(&mut self, target: &Target, x: f32, cx: &mut Context<Self>) {
        let usable = self.layout.track_w(target) - THUMB;
        let fraction = ((x - TRACK_X - THUMB / 2.0) / usable).clamp(0.0, 1.0);
        let percent = (fraction * 100.0).round() as u8;
        match target {
            Target::Device => {
                if self.local_percent != Some(percent) {
                    self.local_percent = Some(percent);
                    crate::request_popup_volume(percent);
                    cx.notify();
                }
            }
            Target::App(key) => {
                if let Some(row) = self.apps.iter_mut().find(|row| &row.app.key == key)
                    && row.local_percent.unwrap_or(row.app.percent) != percent
                {
                    row.local_percent = Some(percent);
                    volume_sessions::set_app_volume(key, percent);
                    cx.notify();
                }
            }
        }
    }

    fn wheel_steps(&mut self, event: &ScrollWheelEvent) -> i32 {
        self.wheel += match event.delta {
            // One wheel notch is reported as a few lines.
            ScrollDelta::Lines(delta) => delta.y / 3.0,
            ScrollDelta::Pixels(delta) => delta.y.as_f32() / 60.0,
        };
        let steps = self.wheel.trunc();
        self.wheel -= steps;
        steps as i32
    }

    fn on_device_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let steps = self.wheel_steps(event);
        if steps != 0 {
            self.local_percent = None;
            crate::request_popup_step(steps);
        }
        cx.stop_propagation();
    }

    fn on_app_wheel(&mut self, key: &str, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let steps = self.wheel_steps(event);
        if steps != 0
            && let Some(row) = self.apps.iter_mut().find(|row| row.app.key == key)
        {
            let current = i32::from(row.local_percent.unwrap_or(row.app.percent));
            let percent = (current + steps * APP_STEP).clamp(0, 100) as u8;
            row.local_percent = Some(percent);
            volume_sessions::set_app_volume(key, percent);
            cx.notify();
        }
        cx.stop_propagation();
    }

    fn toggle_app_mute(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(row) = self.apps.iter_mut().find(|row| row.app.key == key) {
            row.app.muted = !row.app.muted;
            volume_sessions::set_app_mute(key, row.app.muted);
            cx.notify();
        }
    }

    /// Segoe Fluent Icons volume glyph, like the system flyout.
    fn device_glyph(&self) -> char {
        let code = if self.state.muted {
            0xE74F
        } else {
            match self.device_percent() {
                0 => 0xE992,
                1..=33 => 0xE993,
                34..=66 => 0xE994,
                _ => 0xE995,
            }
        };
        char::from_u32(code).unwrap_or(' ')
    }

    fn slider(
        &self,
        id: ElementId,
        target: Target,
        percent: u8,
        muted: bool,
        colors: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let dragging = self.dragging.as_ref() == Some(&target);
        // The thumb follows the pointer exactly; other changes glide.
        let level = anim::tween(
            window,
            cx,
            target.key("level"),
            f32::from(percent) / 100.0,
            if dragging { Transition::new(0, Easing::Linear) } else { LEVEL },
        );
        let track_w = self.layout.track_w(&target);
        let usable = track_w - THUMB;
        let thumb_x = usable * level;
        let fill = if muted { colors.secondary } else { colors.fill };
        let (inner, transition) = if dragging {
            (8.0, PRESS)
        } else if self.hovered_slider.as_ref() == Some(&target) {
            (14.0, HOVER)
        } else {
            (12.0, HOVER)
        };
        let inner = anim::tween(window, cx, target.key("thumb"), inner, transition);
        // Only the device thumb keeps its ring; app thumbs rest as a dot so
        // a long list reads as levels, not as a column of knobs.
        let ringed = dragging
            || target == Target::Device
            || self.hovered_slider.as_ref() == Some(&target);
        let ring = anim::tween(window, cx, target.key("ring"), ringed as u8 as f32, HOVER);
        let outer = THUMB_DOT + (THUMB - THUMB_DOT) * ring;
        let hover_target = target.clone();
        div()
            .id(id)
            .relative()
            .w(px(track_w))
            .h(px(SLIDER_H))
            .flex_shrink_0()
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                if *hovered {
                    view.hovered_slider = Some(hover_target.clone());
                } else if view.hovered_slider.as_ref() == Some(&hover_target) {
                    view.hovered_slider = None;
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _, cx| {
                    view.dragging = Some(target.clone());
                    view.set_from_x(&target, event.position.x.as_f32(), cx);
                }),
            )
            // Rail
            .child(
                div()
                    .absolute()
                    .left(px(THUMB / 2.0))
                    .top(px(SLIDER_H / 2.0 - 2.0))
                    .w(px(usable))
                    .h(px(4.))
                    .rounded_full()
                    .bg(colors.rail),
            )
            // Fill
            .child(
                div()
                    .absolute()
                    .left(px(THUMB / 2.0))
                    .top(px(SLIDER_H / 2.0 - 2.0))
                    .w(px(thumb_x))
                    .h(px(4.))
                    .rounded_full()
                    .bg(fill),
            )
            // Thumb
            .child(
                div()
                    .absolute()
                    .left(px(thumb_x + (THUMB - outer) / 2.0))
                    .top(px(SLIDER_H / 2.0 - outer / 2.0))
                    .size(px(outer))
                    .rounded_full()
                    .bg(Rgba {
                        a: colors.thumb_outer.a * ring,
                        ..colors.thumb_outer
                    })
                    .border_1()
                    .border_color(Rgba {
                        a: colors.thumb_border.a * ring,
                        ..colors.thumb_border
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(inner))
                            .rounded_full()
                            .bg(fill),
                    ),
            )
    }

    fn button(
        &self,
        id: ElementId,
        target: Target,
        colors: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let hovered = self.hovered_button.as_ref() == Some(&target);
        let hover = anim::tween(window, cx, target.key("button"), hovered as u8 as f32, HOVER);
        let hover_target = target.clone();
        div()
            .id(id)
            .size(px(BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .when(hover > 0.0, |d| {
                d.bg(Rgba {
                    a: colors.hover.a * hover,
                    ..colors.hover
                })
            })
            .on_hover(cx.listener(move |view, hovered: &bool, _, cx| {
                if *hovered {
                    view.hovered_button = Some(hover_target.clone());
                } else if view.hovered_button.as_ref() == Some(&hover_target) {
                    view.hovered_button = None;
                }
                cx.notify();
            }))
    }

    fn number_text(percent: u8, muted: bool, strong: bool, colors: &Colors) -> gpui::Div {
        div()
            .flex_shrink_0()
            .text_size(px(if strong { 14. } else { 12. }))
            .font_weight(FontWeight(if strong { 600. } else { 400. }))
            // App numbers stay quiet; only the device level is loud.
            .text_color(if muted || !strong { colors.secondary } else { colors.text })
            .child(percent.to_string())
    }

    /// One row in either layout: compact is `[button] slider number` on one
    /// line; labeled puts `name … number` above a full-width slider.
    #[allow(clippy::too_many_arguments)]
    fn row(
        id: ElementId,
        labeled: bool,
        button: gpui::Stateful<gpui::Div>,
        name: gpui::Div,
        slider: gpui::Stateful<gpui::Div>,
        percent: u8,
        muted: bool,
        strong: bool,
        colors: &Colors,
    ) -> gpui::Stateful<gpui::Div> {
        let number = Self::number_text(percent, muted, strong, colors);
        let row = div()
            .id(id)
            .flex_shrink_0()
            .pl(px(PAD_LEFT))
            .pr(px(PAD_RIGHT))
            .flex()
            .items_center()
            .gap(px(GAP));
        if !labeled {
            return row.h(px(COMPACT_ROW_H)).child(button).child(slider).child(
                number
                    .w(px(NUMBER_W))
                    .flex()
                    .justify_end(),
            );
        }
        row.h(px(LABELED_ROW_H)).child(button).child(
            div()
                .flex()
                .flex_col()
                .child(
                    // Text lines up with the rail, not the thumb's overhang.
                    div()
                        .h(px(LABEL_H))
                        .w(px(TRACK_W_WIDE))
                        .px(px(THUMB / 2.0 - 1.0))
                        .flex()
                        .items_center()
                        .gap(px(GAP))
                        .child(
                            name.flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis(),
                        )
                        .child(number),
                )
                .child(slider),
        )
    }

    fn device_name(&self) -> String {
        if self.state.device_name.is_empty() {
            "Output".to_string()
        } else {
            self.state.device_name.clone()
        }
    }

    fn device_row(
        &self,
        colors: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let percent = self.device_percent();
        let labeled = self.layout.device_labeled();
        let mut button = self
            .button("popup-mute".into(), Target::Device, colors, window, cx)
            .on_click(cx.listener(|_, _, _, _| crate::request_popup_mute()))
            .child(glyph_box(self.device_glyph(), DEVICE_GLYPH, APP_ICON, colors.text));
        if !labeled && !self.layout.header {
            button = button.tooltip(name_tooltip(self.device_name(), self.system.light));
        }
        let slider = self.slider(
            "popup-slider".into(),
            Target::Device,
            percent,
            self.state.muted,
            colors,
            window,
            cx,
        );
        Self::row(
            "popup-device".into(),
            labeled,
            button,
            div()
                .text_size(px(14.))
                .font_weight(FontWeight(600.))
                .text_color(colors.text)
                .child(self.device_name()),
            slider,
            percent,
            self.state.muted,
            true,
            colors,
        )
        .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
            view.on_device_wheel(event, cx)
        }))
    }

    fn app_row(
        &self,
        row: &AppRow,
        colors: &Colors,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let key = row.app.key.clone();
        let target = Target::App(key.clone());
        let percent = row.local_percent.unwrap_or(row.app.percent).min(100);
        let muted = row.app.muted;
        let mute = anim::tween(window, cx, target.key("mute"), muted as u8 as f32, HOVER);
        let icon: gpui::AnyElement = match &row.icon {
            Some(image) => img(image.clone())
                .size(px(APP_ICON))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => glyph_box(
                char::from_u32(if row.app.system { 0xE7F5 } else { 0xECAA }).unwrap_or(' '),
                DEVICE_GLYPH,
                APP_ICON,
                colors.text,
            )
            .into_any_element(),
        };
        let badge_bg = if self.system.light {
            rgb(0xF3F3F3)
        } else {
            rgb(0x2C2C2C)
        };
        let click_key = key.clone();
        // The badge pops in from its corner as the icon dims.
        let badge = 14.0 * mute;
        let mut button = self
            .button(target.key("mute-button"), target.clone(), colors, window, cx)
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_app_mute(&click_key, cx)))
            .child(
                div()
                    .relative()
                    .size(px(APP_ICON))
                    .child(div().size_full().opacity(1.0 - 0.6 * mute).child(icon))
                    .when(mute > 0.01, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(-5. + (14.0 - badge) / 2.0))
                                .bottom(px(-5. + (14.0 - badge) / 2.0))
                                .size(px(badge))
                                .opacity(mute)
                                .rounded_full()
                                .bg(badge_bg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family(super::volume::fluent_font())
                                .text_size(px(9. * mute))
                                .line_height(px(9. * mute))
                                .text_color(colors.text)
                                .child(char::from_u32(0xE74F).unwrap_or(' ').to_string()),
                        )
                    }),
            );
        let slider = self.slider(
            target.key("slider"),
            target.clone(),
            percent,
            muted,
            colors,
            window,
            cx,
        )
        // Wheel on the slider sets the app's volume; anywhere else in the
        // list it scrolls.
        .on_scroll_wheel(cx.listener(move |view, event: &ScrollWheelEvent, _, cx| {
            view.on_app_wheel(&key, event, cx)
        }));
        if !self.layout.labeled {
            button = button.tooltip(name_tooltip(row.app.name.clone(), self.system.light));
        }
        Self::row(
            target.key("row"),
            self.layout.labeled,
            button,
            div()
                .text_size(px(12.))
                .text_color(if muted { colors.secondary } else { colors.text })
                .child(row.app.name.clone()),
            slider,
            percent,
            muted,
            false,
            colors,
        )
    }
}

/// Tooltip with an app or device name, for rows that show no name.
fn name_tooltip(name: String, light: bool) -> impl Fn(&mut Window, &mut App) -> gpui::AnyView + 'static {
    move |_, cx| {
        let name = name.clone();
        cx.new(|_| NameTooltip { name, light }).into()
    }
}

struct NameTooltip {
    name: String,
    light: bool,
}

impl Render for NameTooltip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (bg, border, text) = if self.light {
            (rgb(0xF9F9F9), rgba(0x0000_000F), rgb(0x1B1B1B))
        } else {
            (rgb(0x2C2C2C), rgba(0x0000_0033), rgb(0xFFFFFF))
        };
        div()
            .mt(px(6.))
            .px(px(8.))
            .py(px(5.))
            .rounded(px(4.))
            .bg(bg)
            .border_1()
            .border_color(border)
            .shadow(vec![BoxShadow {
                color: rgba(0x0000_0040).into(),
                offset: point(px(0.), px(4.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            }])
            .font_family("Segoe UI Variable Text")
            .text_size(px(12.))
            .text_color(text)
            .child(self.name.clone())
    }
}

fn popup_divider(colors: &Colors) -> gpui::Div {
    div()
        .h(px(DIVIDER_H))
        .flex_shrink_0()
        .flex()
        .items_center()
        .child(div().w_full().h(px(1.)).bg(colors.divider))
}

/// A Fluent glyph centered in a `box_size` square.
fn glyph_box(glyph: char, glyph_size: f32, box_size: f32, color: Rgba) -> gpui::Div {
    div()
        .size(px(box_size))
        .flex()
        .items_center()
        .justify_center()
        .font_family(super::volume::fluent_font())
        .text_size(px(glyph_size))
        .line_height(px(glyph_size))
        .text_color(color)
        .child(glyph.to_string())
}

impl Render for PopupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let colors = Colors::load(self.system);
        let t = self.appear.value(now).clamp(0.0, 1.0);
        let height = self.card_h.value(now);
        self.apps
            .retain(|row| !(row.leaving && !row.presence.active(now)));
        let rows_moving = self.apps.iter().any(|row| row.presence.active(now));
        if self.appear.active(now) || self.card_h.active(now) || rows_moving {
            window.request_animation_frame();
        }

        // Real Mica: the glass follows the card's bounds and fade.
        let glass = match &mut self.backdrop {
            Some(backdrop) if backdrop.usable() => {
                let scale = window.scale_factor();
                let viewport_h = window.viewport_size().height.as_f32();
                let top = viewport_h - GUTTER - height + 8.0 * (1.0 - t);
                backdrop.set_luminosity(luminosity(self.system.light));
                backdrop.update(
                    [GUTTER * scale, top * scale, CARD_W * scale, height * scale],
                    RADIUS * scale,
                    t,
                );
                backdrop.usable()
            }
            _ => false,
        };
        // Fallback: the blurred wallpaper under where the card settles.
        let mica = if glass {
            None
        } else {
            let (monitor, card) =
                crate::volume_popup::card_screen_rect(crate::volume_popup::last_anchor(), self.card_h.to);
            super::mica::crop(monitor, card)
        };
        if !glass && mica.is_none() && super::mica::pending() {
            window.request_animation_frame();
        }

        let list_h = height - self.layout.fixed_h();
        let mut list = div()
            .id("popup-apps")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px(list_h.max(0.)))
            .overflow_y_scroll();
        let row_h = self.layout.app_row_h();
        for index in 0..self.apps.len() {
            let row = &self.apps[index];
            let presence = row.presence.value(now).clamp(0.0, 1.0);
            let element = self.app_row(row, &colors, window, cx);
            list = list.child(
                // Grows/collapses so the rows below slide along with the card.
                div()
                    .flex_shrink_0()
                    .h(px(row_h * presence))
                    .overflow_hidden()
                    .opacity(presence * presence)
                    .child(element),
            );
        }

        let mut card = div()
            .id("popup-card")
            .w(px(CARD_W))
            .h(px(height))
            .relative()
            .top(px(8.0 * (1.0 - t)))
            .opacity(t)
            .rounded(px(RADIUS))
            .shadow(vec![BoxShadow {
                color: colors.shadow.into(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(16.),
                spread_radius: px(0.),
                inset: false,
            }])
            .font_family("Segoe UI Variable Text");
        card = match &mica {
            Some(wallpaper) => card
                .child(
                    img(wallpaper.clone())
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full()
                        .rounded(px(RADIUS))
                        .object_fit(ObjectFit::Fill),
                )
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .size_full()
                        .rounded(px(RADIUS))
                        .bg(colors.mica_tint),
                ),
            None if glass => card.bg(colors.glass_tint),
            None => card.bg(colors.surface),
        };
        let device_row = self.device_row(&colors, window, cx);
        let apps_first = self.layout.device_at_bottom && !self.apps.is_empty();
        let mut content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .pt(px(PAD_Y))
            .pb(px(self.layout.pad_bottom(self.layout.last_row_h(self.apps.len()))))
            .flex()
            .flex_col();
        if apps_first {
            content = content.child(list).child(popup_divider(&colors));
        }
        if self.layout.header {
            content = content.child(
                // Starts at the icons' left edge.
                div()
                    .h(px(HEADER_H))
                    .flex_shrink_0()
                    .pl(px(PAD_LEFT + (BUTTON - APP_ICON) / 2.0))
                    .pr(px(PAD_RIGHT))
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight(600.))
                    .text_color(colors.text)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(self.device_name()),
            );
        }
        content = content.child(device_row);
        if !self.apps.is_empty() && !apps_first {
            content = content.child(popup_divider(&colors)).child(list);
        }
        // Hairline drawn over the background, not around it: a real border
        // would leave the wallpaper inset by a pixel with seams at the corners.
        let card = card.child(content).child(
            div()
                .absolute()
                .left_0()
                .top_0()
                .size_full()
                .rounded(px(RADIUS))
                .border_1()
                .border_color(colors.border),
        );

        div()
            .id("popup-root")
            .track_focus(&self.focus)
            .size_full()
            .p(px(GUTTER))
            .flex()
            .flex_col()
            .justify_end()
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if let Some(target) = view.dragging.clone() {
                    if event.pressed_button == Some(MouseButton::Left) {
                        view.set_from_x(&target, event.position.x.as_f32(), cx);
                    } else {
                        view.dragging = None;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.dragging = None;
                    cx.notify();
                }),
            )
            .on_key_down(cx.listener(|view, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => view.close(cx),
                    "up" | "right" => {
                        view.local_percent = None;
                        crate::request_popup_step(1);
                    }
                    "down" | "left" => {
                        view.local_percent = None;
                        crate::request_popup_step(-1);
                    }
                    "space" | "m" => crate::request_popup_mute(),
                    _ => {}
                }
            }))
            .child(card)
    }
}
