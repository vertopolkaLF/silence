//! Volume slider popup above the tray icon (Windows 11 flyout look).
//!
//! Runs on the volume OSD's GPUI thread as a second, focusable window. Volume
//! writes go back to the tray thread as window messages; the tray then sends
//! the new state here through `PopupCommand::Update`.

use super::{motion::Motion, theme::System};
use crate::volume_popup::{CARD_H, CARD_W, GUTTER, PopupCommand, VolumePopupState};
use gpui::{
    App, Bounds, BoxShadow, Context, FocusHandle, FontWeight, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement, Render, Rgba,
    ScrollDelta, ScrollWheelEvent, StatefulInteractiveElement, Styled, Task, Window,
    WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind, WindowOptions, div, point,
    prelude::*, px, rgb, rgba, size,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos},
};

const PAD: f32 = 16.0;
const BUTTON: f32 = 36.0;
const GAP_AFTER_BUTTON: f32 = 10.0;
const PERCENT_W: f32 = 40.0;
const GAP_BEFORE_PERCENT: f32 = 12.0;
const TRACK_W: f32 = CARD_W - PAD * 2.0 - BUTTON - GAP_AFTER_BUTTON - GAP_BEFORE_PERCENT - PERCENT_W;
/// Window-relative x of the slider's left edge.
const TRACK_X: f32 = GUTTER + PAD + BUTTON + GAP_AFTER_BUTTON;
const THUMB: f32 = 20.0;
const APPEAR_MS: u64 = 160;
const CLOSE_MS: u64 = 130;
/// Closes on its own if Windows never let it take focus.
const UNFOCUSED_TIMEOUT: Duration = Duration::from_secs(6);

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
        }
    }
}

fn with_view(cx: &mut App, f: impl FnOnce(&mut PopupView, &mut Context<PopupView>)) {
    let Some((handle, _)) = POPUP.with(|popup| popup.borrow().clone()) else {
        return;
    };
    let _ = handle.update(cx, |view, _, cx| f(view, cx));
}

fn show(anchor: windows::Win32::Foundation::RECT, state: VolumePopupState, cx: &mut App) {
    let existing = POPUP.with(|popup| popup.borrow().clone());
    let Some((handle, hwnd)) = existing.or_else(|| create(cx)) else {
        return;
    };
    let alive = handle
        .update(cx, |view, window, cx| {
            view.set_state(state, cx);
            view.reopen(window, cx);
        })
        .is_ok();
    if alive {
        place(hwnd, anchor);
    } else {
        POPUP.with(|popup| popup.borrow_mut().take());
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
                size(px(CARD_W + GUTTER * 2.0), px(CARD_H + GUTTER * 2.0)),
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

fn place(hwnd: HWND, anchor: windows::Win32::Foundation::RECT) {
    let (x, y, width, height) = crate::volume_popup::placement(anchor);
    unsafe {
        let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, width, height, SWP_SHOWWINDOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

struct Colors {
    surface: Rgba,
    border: Rgba,
    text: Rgba,
    muted: Rgba,
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
                surface: rgb(0xF9F9F9),
                border: rgba(0x0000_0018),
                text: rgb(0x1B1B1B),
                muted: rgb(0x5F5F5F),
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
                border: rgba(0xFFFF_FF1A),
                text: rgb(0xFFFFFF),
                muted: rgb(0xC5C5C5),
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

struct PopupView {
    state: VolumePopupState,
    /// Shown level while dragging, ahead of the tray's confirmation.
    local_percent: Option<u8>,
    dragging: bool,
    thumb_hovered: bool,
    button_hovered: bool,
    wheel: f32,
    system: System,
    appear: Motion,
    closing: bool,
    was_active: bool,
    focus: FocusHandle,
    timeout: Option<Task<()>>,
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
        // Created hidden (prewarmed); `reopen` shows it.
        let appear = Motion::decelerate(0.0, APPEAR_MS);
        Self {
            state,
            local_percent: None,
            dragging: false,
            thumb_hovered: false,
            button_hovered: false,
            wheel: 0.0,
            system: System::load(),
            appear,
            closing: true,
            was_active: false,
            focus: cx.focus_handle(),
            timeout: None,
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
        crate::volume_popup::mark_open(true);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn set_state(&mut self, state: VolumePopupState, cx: &mut Context<Self>) {
        if !self.dragging {
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
        self.dragging = false;
        crate::volume_popup::mark_open(false);
        self.appear.retarget(0.0, Instant::now());
        cx.notify();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(CLOSE_MS + 20))
                .await;
            let _ = view.update(cx, |view, _| {
                if view.closing
                    && let Some((_, hwnd)) = POPUP.with(|popup| popup.borrow().clone())
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

    fn percent(&self) -> u8 {
        self.local_percent.unwrap_or(self.state.percent).min(100)
    }

    fn set_from_x(&mut self, x: f32, cx: &mut Context<Self>) {
        let usable = TRACK_W - THUMB;
        let fraction = ((x - TRACK_X - THUMB / 2.0) / usable).clamp(0.0, 1.0);
        let percent = (fraction * 100.0).round() as u8;
        if self.local_percent != Some(percent) {
            self.local_percent = Some(percent);
            crate::request_popup_volume(percent);
            cx.notify();
        }
    }

    fn on_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        self.wheel += match event.delta {
            // One wheel notch is reported as a few lines.
            ScrollDelta::Lines(delta) => delta.y / 3.0,
            ScrollDelta::Pixels(delta) => delta.y.as_f32() / 60.0,
        };
        let steps = self.wheel.trunc();
        if steps != 0.0 {
            self.wheel -= steps;
            self.local_percent = None;
            crate::request_popup_step(steps as i32);
        }
        cx.stop_propagation();
    }

    /// Segoe Fluent Icons volume glyph, like the system flyout.
    fn glyph(&self) -> char {
        let percent = self.percent();
        let code = if self.state.muted {
            0xE74F
        } else {
            match percent {
                0 => 0xE992,
                1..=33 => 0xE993,
                34..=66 => 0xE994,
                _ => 0xE995,
            }
        };
        char::from_u32(code).unwrap_or(' ')
    }
}

impl Render for PopupView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let colors = Colors::load(self.system);
        let t = self.appear.value(now).clamp(0.0, 1.0);
        if self.appear.active(now) {
            window.request_animation_frame();
        }
        let percent = self.percent();
        let level = f32::from(percent) / 100.0;
        let usable = TRACK_W - THUMB;
        let thumb_x = usable * level;
        let fill = if self.state.muted {
            colors.muted
        } else {
            colors.fill
        };
        let inner = if self.dragging {
            8.0
        } else if self.thumb_hovered {
            14.0
        } else {
            12.0
        };
        let name = if self.state.device_name.is_empty() {
            "Output".to_string()
        } else {
            self.state.device_name.clone()
        };

        let button = div()
            .id("popup-mute")
            .size(px(BUTTON))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .when(self.button_hovered, |d| d.bg(colors.hover))
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                view.button_hovered = *hovered;
                cx.notify();
            }))
            .on_click(cx.listener(|_, _, _, _| crate::request_popup_mute()))
            .child(
                div()
                    .font_family(super::volume::fluent_font())
                    .text_size(px(16.))
                    .line_height(px(16.))
                    .text_color(colors.text)
                    .child(self.glyph().to_string()),
            );

        let slider = div()
            .id("popup-slider")
            .relative()
            .w(px(TRACK_W))
            .h(px(BUTTON))
            .flex_shrink_0()
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                view.thumb_hovered = *hovered;
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, _, cx| {
                    view.dragging = true;
                    view.set_from_x(event.position.x.as_f32(), cx);
                }),
            )
            // Rail
            .child(
                div()
                    .absolute()
                    .left(px(THUMB / 2.0))
                    .top(px(BUTTON / 2.0 - 2.0))
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
                    .top(px(BUTTON / 2.0 - 2.0))
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
                    .top(px(BUTTON / 2.0 - THUMB / 2.0))
                    .size(px(THUMB))
                    .rounded_full()
                    .bg(colors.thumb_outer)
                    .border_1()
                    .border_color(colors.thumb_border)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(div().size(px(inner)).rounded_full().bg(fill)),
            );

        let card = div()
            .id("popup-card")
            .w(px(CARD_W))
            .h(px(CARD_H))
            .relative()
            .top(px(8.0 * (1.0 - t)))
            .opacity(t)
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(PAD))
            .rounded(px(8.))
            .bg(colors.surface)
            .border_1()
            .border_color(colors.border)
            .shadow(vec![BoxShadow {
                color: colors.shadow.into(),
                offset: point(px(0.), px(8.)),
                blur_radius: px(16.),
                spread_radius: px(0.),
                inset: false,
            }])
            .font_family("Segoe UI Variable Text")
            .on_scroll_wheel(cx.listener(|view, event: &ScrollWheelEvent, _, cx| {
                view.on_wheel(event, cx)
            }))
            .child(
                div()
                    .h(px(18.))
                    .flex()
                    .items_center()
                    .text_size(px(12.5))
                    .text_color(colors.muted)
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(name),
            )
            .child(
                div()
                    .h(px(BUTTON))
                    .flex()
                    .items_center()
                    .child(button)
                    .child(div().w(px(GAP_AFTER_BUTTON)).flex_shrink_0())
                    .child(slider)
                    .child(div().w(px(GAP_BEFORE_PERCENT)).flex_shrink_0())
                    .child(
                        div()
                            .w(px(PERCENT_W))
                            .flex_shrink_0()
                            .flex()
                            .justify_end()
                            .text_size(px(15.))
                            .font_weight(FontWeight(600.))
                            .text_color(if self.state.muted {
                                colors.muted
                            } else {
                                colors.text
                            })
                            .child(percent.to_string()),
                    ),
            );

        div()
            .id("popup-root")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if view.dragging {
                    if event.pressed_button == Some(MouseButton::Left) {
                        view.set_from_x(event.position.x.as_f32(), cx);
                    } else {
                        view.dragging = false;
                        cx.notify();
                    }
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _, _, cx| {
                    view.dragging = false;
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
