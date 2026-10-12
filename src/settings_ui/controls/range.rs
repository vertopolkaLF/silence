//! Filled 48px range shell from `.ui-range-*` in `controls.css`.
use super::{SettingsApp, anim};
use crate::settings_ui::{assets::icon, color::mix_pct, style::palette};
use gpui::{prelude::*, *};
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct RangeOptions {
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub label: SharedString,
    pub value_label: SharedString,
    pub icon: Option<SharedString>,
}

struct State {
    bounds: Bounds<Pixels>,
    hovered_since: Option<Instant>,
    dragging: bool,
    value: f64,
    focus: FocusHandle,
}

fn snap(value: f64, options: &RangeOptions) -> f64 {
    let max = options.max.max(options.min);
    let step = if options.step.is_finite() && options.step > 0. {
        options.step
    } else {
        1.
    };
    (options.min + ((value - options.min) / step).round() * step).clamp(options.min, max)
}

fn pointer_value(x: Pixels, bounds: Bounds<Pixels>, options: &RangeOptions) -> Option<f64> {
    if bounds.size.width <= px(0.) {
        return None;
    }
    // The invisible HTML thumb is 18px wide; its travel starts 9px in.
    let fraction =
        ((x - bounds.left() - px(9.)) / (bounds.size.width - px(18.)).max(px(1.))).clamp(0., 1.);
    Some(snap(
        options.min + f64::from(fraction) * (options.max - options.min),
        options,
    ))
}

/// Click, drag (including outside the shell), and wheel after a 600ms hover.
/// Fill animates for 240ms except while dragging. Typography inherits the caller.
/// ```ignore
/// range("volume", volume, RangeOptions { min: 0., max: 100., step: 1.,
///     label: "Volume".into(), value_label: format!("{volume}%").into(),
///     icon: Some("volume-loud-linear".into()) }, window, cx,
///     |app, value, _, cx| { /* apply value immediately */ });
/// ```
pub(crate) fn range(
    id: impl Into<ElementId>,
    value: f64,
    options: RangeOptions,
    window: &mut Window,
    cx: &mut Context<SettingsApp>,
    on_change: impl Fn(&mut SettingsApp, f64, &mut Window, &mut Context<SettingsApp>) + 'static,
) -> Stateful<Div> {
    let id = id.into();
    let p = palette(cx);
    let state = window.use_keyed_state((id.clone(), "range-state"), cx, |_, cx| State {
        bounds: Bounds::default(),
        hovered_since: None,
        dragging: false,
        value: 0.,
        focus: cx.focus_handle(),
    });
    state.update(cx, |state, _| {
        if !state.dragging {
            state.value = snap(value, &options);
        }
    });
    let dragging = state.read(cx).dragging;
    let hovered = state.read(cx).hovered_since.is_some();
    let value = state.read(cx).value;
    let progress = if options.max > options.min {
        ((value - options.min) / (options.max - options.min)) as f32
    } else {
        0.
    };
    let progress = anim::tween(
        window,
        cx,
        (id.clone(), "fill"),
        progress,
        anim::Transition::new(if dragging { 0 } else { 240 }, anim::Easing::Ease),
    );
    let scale = anim::tween(
        window,
        cx,
        (id.clone(), "thumb-scale"),
        if dragging {
            0.92
        } else if hovered {
            1.1
        } else {
            1.
        },
        anim::UI,
    );
    let hover = anim::tween(
        window,
        cx,
        (id.clone(), "thumb-hover"),
        hovered as u8 as f32,
        anim::UI,
    );
    let callback = Rc::new(on_change);
    let weak_app = cx.weak_entity();
    let update: Rc<dyn Fn(Pixels, &mut Window, &mut App)> = {
        let state = state.clone();
        let options = options.clone();
        let callback = callback.clone();
        let weak_app = weak_app.clone();
        Rc::new(move |x, window, cx| {
            let bounds = state.read(cx).bounds;
            let Some(value) = pointer_value(x, bounds, &options) else {
                return;
            };
            state.update(cx, |state, cx| {
                state.value = value;
                cx.notify();
            });
            let _ = weak_app.update(cx, |app, cx| callback(app, value, window, cx));
        })
    };
    let down_state = state.clone();
    let down_options = options.clone();
    let down_callback = callback.clone();
    let hover_state = state.clone();
    let wheel_state = state.clone();
    let wheel_options = options.clone();
    let capture_state = state.clone();
    let release_state = state.clone();
    // Canvas installs window-level drag listeners during paint, the valid phase.
    let capture = canvas(
        move |bounds, window, cx| {
            if capture_state.read(cx).bounds != bounds {
                capture_state.update(cx, |state, _| state.bounds = bounds);
                window.request_animation_frame();
            }
        },
        move |_, _, window, _| {
            let state = state.clone();
            let update = update.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && state.read(cx).dragging {
                    if event.pressed_button == Some(MouseButton::Left) {
                        update(event.position.x, window, cx);
                    } else {
                        state.update(cx, |s, cx| {
                            s.dragging = false;
                            cx.notify();
                        });
                    }
                }
            });
            let state = release_state.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble
                    && event.button == MouseButton::Left
                    && state.read(cx).dragging
                {
                    state.update(cx, |s, cx| {
                        s.dragging = false;
                        cx.notify();
                    });
                }
            });
        },
    )
    .absolute()
    .size_full();
    let width = f32::from(down_state.read(cx).bounds.size.width);
    let left = (progress * width - 6.).clamp(3., (width - 3.).max(3.)) - 3.;
    let focus = down_state.read(cx).focus.clone();
    let key_state = down_state.clone();
    let key_options = options.clone();
    let key_callback = callback.clone();
    div()
        .id(id)
        .relative()
        .w_full()
        .h(px(48.))
        .cursor_pointer()
        .track_focus(&focus)
        .focus_visible(move |style| super::focus_outline(style, p.focus.hsla()))
        .on_key_down(cx.listener(move |app, event: &KeyDownEvent, window, cx| {
            let current = key_state.read(cx).value;
            let next = match event.keystroke.key.as_str() {
                "left" | "down" => current - key_options.step,
                "right" | "up" => current + key_options.step,
                "home" => key_options.min,
                "end" => key_options.max,
                _ => return,
            };
            let value = snap(next, &key_options);
            key_state.update(cx, |s, cx| {
                s.value = value;
                cx.notify();
            });
            key_callback(app, value, window, cx);
            window.prevent_default();
            cx.stop_propagation();
        }))
        .on_hover(move |hovered, _, cx| {
            hover_state.update(cx, |s, cx| {
                s.hovered_since = hovered.then(Instant::now);
                cx.notify();
            })
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |app, event: &MouseDownEvent, window, cx| {
                let focus = down_state.read(cx).focus.clone();
                focus.focus(window, cx);
                down_state.update(cx, |s, cx| {
                    s.dragging = true;
                    cx.notify();
                });
                if let Some(value) =
                    pointer_value(event.position.x, down_state.read(cx).bounds, &down_options)
                {
                    down_state.update(cx, |state, cx| {
                        state.value = value;
                        cx.notify();
                    });
                    down_callback(app, value, window, cx);
                }
                cx.stop_propagation();
            }),
        )
        .on_scroll_wheel(
            cx.listener(move |app, event: &ScrollWheelEvent, window, cx| {
                if !wheel_state
                    .read(cx)
                    .hovered_since
                    .is_some_and(|t| t.elapsed() >= Duration::from_millis(600))
                {
                    return;
                }
                let delta = event.delta.pixel_delta(px(20.)).y;
                if delta == px(0.) {
                    return;
                }
                let value = snap(
                    wheel_state.read(cx).value
                        + if delta > px(0.) {
                            wheel_options.step
                        } else {
                            -wheel_options.step
                        },
                    &wheel_options,
                );
                wheel_state.update(cx, |s, cx| {
                    s.value = value;
                    cx.notify();
                });
                callback(app, value, window, cx);
                cx.stop_propagation();
            }),
        )
        .child(
            div()
                .absolute()
                .top(px(4.))
                .bottom(px(4.))
                .w_full()
                .rounded(px(12.))
                .bg(p.surface_2.hsla()),
        )
        .child(
            div()
                .absolute()
                .top(px(4.))
                .bottom(px(4.))
                .w(relative(progress))
                .rounded(px(12.))
                .bg(mix_pct(p.accent, 12., p.surface_2).hsla()),
        )
        .child(
            div()
                .absolute()
                .top(px(9. + (30. - 30. * scale) / 2.))
                .left(px(left + (6. - 6. * scale) / 2.))
                .w(px(6. * scale))
                .h(px(30. * scale))
                .rounded(px(12.))
                .bg(mix_pct(p.accent, 86., p.text).hsla())
                .shadow(vec![BoxShadow {
                    color: mix_pct(p.accent, 20. + 8. * hover, p.shadow_medium).hsla(),
                    offset: point(px(0.), px(1. + hover)),
                    blur_radius: px(4. + 3. * hover),
                    spread_radius: px(0.),
                    inset: false,
                }]),
        )
        .child(
            div()
                .absolute()
                .top(px(4.))
                .bottom(px(4.))
                .left(px(18.))
                .right(px(18.))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.))
                .text_color(p.text.hsla())
                .font_weight(FontWeight(650.))
                .line_height(relative(1.2))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .when_some(options.icon, |this, name| {
                            this.child(
                                svg()
                                    .path(icon(&name))
                                    .size(px(18.))
                                    .flex_shrink_0()
                                    .text_color(p.text.hsla()),
                            )
                        })
                        .child(options.label),
                )
                .child(div().flex_shrink_0().child(options.value_label)),
        )
        .child(capture)
}
