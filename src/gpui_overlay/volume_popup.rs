//! Volume popup above the tray icon: the device slider plus one slider per
//! app, like EarTrumpet, drawn as a Windows 11 flyout on fake Mica.
//!
//! Runs on the volume OSD's GPUI thread as a second, focusable window. Device
//! volume writes go back to the tray thread as window messages; the tray then
//! sends the new state here through `PopupCommand::Update`. App volumes are
//! read and written here directly through [`crate::volume_sessions`], polled
//! while the popup is open.

use super::{motion::Motion, theme::System};
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
const PAD_LEFT: f32 = 10.0;
const PAD_RIGHT: f32 = 16.0;
const BUTTON: f32 = 40.0;
const GAP: f32 = 8.0;
const NUMBER_W: f32 = 34.0;
const TRACK_W: f32 = CARD_W - PAD_LEFT - BUTTON - GAP - GAP - NUMBER_W - PAD_RIGHT;
/// Window-relative x of every slider's left edge.
const TRACK_X: f32 = GUTTER + PAD_LEFT + BUTTON + GAP;
const SLIDER_H: f32 = 28.0;
const THUMB: f32 = 20.0;
const CAPTION_H: f32 = 20.0;
const DEVICE_ROW_H: f32 = 60.0;
const APP_ROW_H: f32 = 54.0;
const DIVIDER_H: f32 = 9.0;
const PAD_TOP: f32 = 6.0;
const PAD_BOTTOM: f32 = 6.0;
/// More apps than this scroll inside the card.
const MAX_VISIBLE_APPS: usize = 7;
const APP_ICON: f32 = 24.0;
const RADIUS: f32 = 8.0;
const APP_STEP: i32 = 2;

const APPEAR_MS: u64 = 160;
const CLOSE_MS: u64 = 130;
const RESIZE_MS: u64 = 200;
const POLL: Duration = Duration::from_millis(500);
/// Closes on its own if Windows never let it take focus.
const UNFOCUSED_TIMEOUT: Duration = Duration::from_secs(6);

fn card_height(apps: usize) -> f32 {
    let apps = apps.min(MAX_VISIBLE_APPS);
    let list = if apps == 0 {
        0.0
    } else {
        DIVIDER_H + apps as f32 * APP_ROW_H
    };
    PAD_TOP + DEVICE_ROW_H + list + PAD_BOTTOM
}

/// The window is created at the tallest card: GPUI never grows a window past
/// its first size, so it is shrunk to fit instead.
fn max_card_height() -> f32 {
    card_height(MAX_VISIBLE_APPS)
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
        Some(card_h) => {
            place(hwnd, anchor, card_h, true);
        }
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
    POPUP.with(|popup| *popup.borrow_mut() = Some((handle, hwnd)));
    Some((handle, hwnd))
}

/// Moves (and with `activate`, shows and focuses) the window for a card
/// `card_h` tall above `anchor`.
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

struct Colors {
    surface: Rgba,
    /// Over the blurred wallpaper.
    mica_tint: Rgba,
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
                border: rgba(0x0000_0018),
                divider: rgba(0x0000_0014),
                text: rgb(0x1B1B1B),
                secondary: rgb(0x5F5F5F),
                rail: rgba(0x0000_0072),
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
                border: rgba(0xFFFF_FF1A),
                divider: rgba(0xFFFF_FF14),
                text: rgb(0xFFFFFF),
                secondary: rgb(0xC5C5C5),
                rail: rgba(0xFFFF_FF8B),
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

struct AppRow {
    app: AppVolume,
    icon: Option<Arc<Image>>,
    /// Shown level while dragging, ahead of the session's own value.
    local_percent: Option<u8>,
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
        let height = card_height(0);
        Self {
            state,
            local_percent: None,
            apps: Vec::new(),
            icons: HashMap::new(),
            dragging: None,
            hovered_slider: None,
            hovered_button: None,
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

    /// Reads the app sessions (and new icons) off the UI thread.
    fn refresh_apps(&mut self, cx: &mut Context<Self>) {
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
        let mut rows = Vec::with_capacity(apps.len());
        for app in apps {
            let previous = self.apps.iter().find(|row| row.app.key == app.key);
            let dragging = self.dragging == Some(Target::App(app.key.clone()));
            let local_percent = previous.and_then(|row| row.local_percent);
            let icon = app
                .icon_path
                .as_ref()
                .and_then(|path| self.icons.get(path).cloned().flatten());
            rows.push(AppRow {
                app,
                icon,
                // Keep the dragged value until the session reports it.
                local_percent: if dragging { local_percent } else { None },
            });
        }
        let changed_count = rows.len() != self.apps.len();
        let changed = changed_count
            || rows
                .iter()
                .zip(&self.apps)
                .any(|(a, b)| a.app != b.app || a.icon.is_some() != b.icon.is_some());
        self.apps = rows;
        if changed_count {
            self.resize(cx);
        }
        if changed {
            cx.notify();
        }
    }

    /// Animates the card to fit the rows. The window grows at once and
    /// shrinks after the card has, so the animation is never clipped.
    fn resize(&mut self, cx: &mut Context<Self>) {
        let target = card_height(self.apps.len());
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
                place(hwnd, crate::volume_popup::last_anchor(), target, false);
            }
        } else {
            self.shrink = Some(cx.spawn(async move |view, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(RESIZE_MS + 20))
                    .await;
                let _ = view.update(cx, |view, _| {
                    view.window_card_h = target;
                    if !view.closing {
                        place(hwnd, crate::volume_popup::last_anchor(), target, false);
                    }
                });
            }));
        }
        cx.notify();
    }

    fn set_state(&mut self, state: VolumePopupState, cx: &mut Context<Self>) {
        if self.dragging != Some(Target::Device) {
            self.local_percent = None;
        }
        self.state = state;
        cx.notify();
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.dragging = None;
        self.poll = None;
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
        let usable = TRACK_W - THUMB;
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
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let level = f32::from(percent) / 100.0;
        let usable = TRACK_W - THUMB;
        let thumb_x = usable * level;
        let fill = if muted { colors.secondary } else { colors.fill };
        let inner = if self.dragging.as_ref() == Some(&target) {
            8.0
        } else if self.hovered_slider.as_ref() == Some(&target) {
            14.0
        } else {
            12.0
        };
        let hover_target = target.clone();
        div()
            .id(id)
            .relative()
            .w(px(TRACK_W))
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
                    .left(px(thumb_x))
                    .top(px(SLIDER_H / 2.0 - THUMB / 2.0))
                    .size(px(THUMB))
                    .rounded_full()
                    .bg(colors.thumb_outer)
                    .border_1()
                    .border_color(colors.thumb_border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(inner)).rounded_full().bg(fill)),
            )
    }

    fn button(
        &self,
        id: ElementId,
        target: Target,
        colors: &Colors,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let hover_target = target.clone();
        div()
            .id(id)
            .size(px(BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .when(self.hovered_button.as_ref() == Some(&target), |d| {
                d.bg(colors.hover)
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

    fn number(percent: u8, muted: bool, strong: bool, colors: &Colors) -> impl IntoElement {
        div()
            .w(px(NUMBER_W))
            .h(px(SLIDER_H))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_end()
            .text_size(px(if strong { 15. } else { 14. }))
            .font_weight(FontWeight(if strong { 600. } else { 400. }))
            .text_color(if muted { colors.secondary } else { colors.text })
            .child(percent.to_string())
    }

    /// Caption above the slider, then the slider and its number.
    fn controls(caption: gpui::Div, slider: impl IntoElement, number: impl IntoElement) -> gpui::Div {
        div()
            .flex()
            .flex_col()
            .child(
                caption
                    .h(px(CAPTION_H))
                    .w(px(TRACK_W + GAP + NUMBER_W))
                    .pl(px(THUMB / 2.0 - 1.0))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis(),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(GAP))
                    .child(slider)
                    .child(number),
            )
    }

    fn device_row(&self, colors: &Colors, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let percent = self.device_percent();
        let name = if self.state.device_name.is_empty() {
            "Output".to_string()
        } else {
            self.state.device_name.clone()
        };
        let button = self
            .button("popup-mute".into(), Target::Device, colors, cx)
            .on_click(cx.listener(|_, _, _, _| crate::request_popup_mute()))
            .child(
                div()
                    .font_family(super::volume::fluent_font())
                    .text_size(px(18.))
                    .line_height(px(18.))
                    .text_color(colors.text)
                    .child(self.device_glyph().to_string()),
            );
        let slider = self.slider(
            "popup-slider".into(),
            Target::Device,
            percent,
            self.state.muted,
            colors,
            cx,
        );
        div()
            .id("popup-device")
            .h(px(DEVICE_ROW_H))
            .flex_shrink_0()
            .pl(px(PAD_LEFT))
            .flex()
            .items_center()
            .gap(px(GAP))
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.on_device_wheel(event, cx)
            }))
            .child(button_slot(button))
            .child(Self::controls(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight(600.))
                    .text_color(colors.text)
                    .child(name),
                slider,
                Self::number(percent, self.state.muted, true, colors),
            ))
    }

    fn app_row(&self, index: usize, row: &AppRow, colors: &Colors, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let key = row.app.key.clone();
        let target = Target::App(key.clone());
        let percent = row.local_percent.unwrap_or(row.app.percent).min(100);
        let muted = row.app.muted;
        let icon: gpui::AnyElement = match &row.icon {
            Some(image) => img(image.clone())
                .size(px(APP_ICON))
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => div()
                .font_family(super::volume::fluent_font())
                .text_size(px(20.))
                .line_height(px(20.))
                .text_color(colors.text)
                .child(
                    char::from_u32(if row.app.system { 0xE7F5 } else { 0xECAA })
                        .unwrap_or(' ')
                        .to_string(),
                )
                .into_any_element(),
        };
        let badge_bg = if self.system.light {
            rgb(0xF3F3F3)
        } else {
            rgb(0x2C2C2C)
        };
        let click_key = key.clone();
        let button = self
            .button(ElementId::NamedInteger("popup-app-mute".into(), index as u64), target.clone(), colors, cx)
            .on_click(cx.listener(move |view, _, _, cx| view.toggle_app_mute(&click_key, cx)))
            .child(
                div()
                    .relative()
                    .size(px(APP_ICON))
                    .child(div().size_full().opacity(if muted { 0.4 } else { 1.0 }).child(icon))
                    .when(muted, |d| {
                        d.child(
                            div()
                                .absolute()
                                .right(px(-5.))
                                .bottom(px(-5.))
                                .size(px(15.))
                                .rounded_full()
                                .bg(badge_bg)
                                .flex()
                                .items_center()
                                .justify_center()
                                .font_family(super::volume::fluent_font())
                                .text_size(px(9.))
                                .line_height(px(9.))
                                .text_color(colors.text)
                                .child(char::from_u32(0xE74F).unwrap_or(' ').to_string()),
                        )
                    }),
            );
        let slider = self.slider(
            ElementId::NamedInteger("popup-app-slider".into(), index as u64),
            target,
            percent,
            muted,
            colors,
            cx,
        );
        div()
            .id(ElementId::NamedInteger("popup-app".into(), index as u64))
            .h(px(APP_ROW_H))
            .flex_shrink_0()
            .pl(px(PAD_LEFT))
            .flex()
            .items_center()
            .gap(px(GAP))
            .on_scroll_wheel(cx.listener(move |view, event: &ScrollWheelEvent, _, cx| {
                view.on_app_wheel(&key, event, cx)
            }))
            .child(button_slot(button))
            .child(Self::controls(
                div()
                    .text_size(px(12.))
                    .text_color(colors.secondary)
                    .child(row.app.name.clone()),
                slider,
                Self::number(percent, muted, false, colors),
            ))
    }
}

/// Centers a row's button on its caption and slider.
fn button_slot(button: impl IntoElement) -> gpui::Div {
    div()
        .h(px(CAPTION_H + SLIDER_H))
        .flex()
        .items_center()
        .child(button)
}

impl Render for PopupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let colors = Colors::load(self.system);
        let t = self.appear.value(now).clamp(0.0, 1.0);
        let height = self.card_h.value(now);
        if self.appear.active(now) || self.card_h.active(now) {
            window.request_animation_frame();
        }

        // Fake Mica: the blurred wallpaper under where the card settles.
        let mica = {
            let (monitor, card) =
                crate::volume_popup::card_screen_rect(crate::volume_popup::last_anchor(), self.card_h.to);
            super::mica::crop(monitor, card)
        };
        if mica.is_none() && super::mica::pending() {
            window.request_animation_frame();
        }

        let apps_h = height - PAD_TOP - DEVICE_ROW_H - PAD_BOTTOM;
        let mut list = div()
            .id("popup-apps")
            .flex()
            .flex_col()
            .flex_shrink_0()
            .h(px((apps_h - DIVIDER_H).max(0.)))
            .overflow_y_scroll();
        for index in 0..self.apps.len() {
            let row = &self.apps[index];
            let element = self.app_row(index, row, &colors, cx);
            list = list.child(element);
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
            .font_family("Segoe UI Variable Text")
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.on_device_wheel(event, cx)
            }));
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
            None => card.bg(colors.surface),
        };
        let content = div()
            .relative()
            .size_full()
            .overflow_hidden()
            .pt(px(PAD_TOP))
            .flex()
            .flex_col()
            .child(self.device_row(&colors, cx))
            .when(!self.apps.is_empty(), |d| {
                d.child(
                    div()
                        .h(px(DIVIDER_H))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .child(div().w_full().h(px(1.)).bg(colors.divider)),
                )
                .child(list)
            });
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
