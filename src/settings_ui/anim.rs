//! CSS-like transitions for GPUI. A [`tween`] keeps per-element state across
//! frames, retargets from its current value like a CSS `transition`, and
//! requests frames only while it is moving.
use gpui::{App, Context, ElementId, Entity, Window};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Easing {
    Linear,
    /// CSS `ease`.
    Ease,
    EaseIn,
    EaseOut,
    EaseInOut,
    Bezier(f32, f32, f32, f32),
}

impl Easing {
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0., 1.);
        match self {
            Self::Linear => t,
            Self::Ease => cubic_bezier(0.25, 0.1, 0.25, 1., t),
            Self::EaseIn => cubic_bezier(0.42, 0., 1., 1., t),
            Self::EaseOut => cubic_bezier(0., 0., 0.58, 1., t),
            Self::EaseInOut => cubic_bezier(0.42, 0., 0.58, 1., t),
            Self::Bezier(x1, y1, x2, y2) => cubic_bezier(x1, y1, x2, y2, t),
        }
    }
}

/// Solves a CSS `cubic-bezier(x1, y1, x2, y2)` timing function at progress `x`.
pub(crate) fn cubic_bezier(x1: f32, y1: f32, x2: f32, y2: f32, x: f32) -> f32 {
    if x <= 0. {
        return 0.;
    }
    if x >= 1. {
        return 1.;
    }
    let curve = |a: f32, b: f32, t: f32| {
        let u = 1. - t;
        3. * u * u * t * a + 3. * u * t * t * b + t * t * t
    };
    let slope = |a: f32, b: f32, t: f32| {
        let u = 1. - t;
        3. * u * u * a + 6. * u * t * (b - a) + 3. * t * t * (1. - b)
    };
    let mut t = x;
    for _ in 0..8 {
        let error = curve(x1, x2, t) - x;
        if error.abs() < 1e-5 {
            return curve(y1, y2, t);
        }
        let d = slope(x1, x2, t);
        if d.abs() < 1e-6 {
            break;
        }
        t -= error / d;
    }
    // Fall back to bisection for flat slopes.
    let (mut lo, mut hi) = (0_f32, 1_f32);
    t = x;
    for _ in 0..30 {
        let value = curve(x1, x2, t);
        if (value - x).abs() < 1e-5 {
            break;
        }
        if value < x {
            lo = t;
        } else {
            hi = t;
        }
        t = (lo + hi) / 2.;
    }
    curve(y1, y2, t)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Transition {
    pub duration: Duration,
    pub delay: Duration,
    pub easing: Easing,
}

impl Transition {
    pub const fn new(duration_ms: u64, easing: Easing) -> Self {
        Self {
            duration: Duration::from_millis(duration_ms),
            delay: Duration::ZERO,
            easing,
        }
    }

    pub const fn delay(mut self, delay_ms: u64) -> Self {
        self.delay = Duration::from_millis(delay_ms);
        self
    }
}

/// `--ui-transition: 0.2s ease`.
pub(crate) const UI: Transition = Transition::new(200, Easing::Ease);

pub(crate) struct Tween {
    from: f32,
    to: f32,
    started: Instant,
    spec: Transition,
}

impl Tween {
    fn settled(value: f32) -> Self {
        Self {
            from: value,
            to: value,
            started: Instant::now() - Duration::from_secs(60),
            spec: UI,
        }
    }

    pub fn value(&self) -> f32 {
        let elapsed = self.started.elapsed().saturating_sub(self.spec.delay);
        if self.spec.duration.is_zero() {
            return self.to;
        }
        let t = elapsed.as_secs_f32() / self.spec.duration.as_secs_f32();
        if t >= 1. {
            return self.to;
        }
        self.from + (self.to - self.from) * self.spec.easing.apply(t)
    }

    pub fn is_moving(&self) -> bool {
        self.started.elapsed() < self.spec.delay + self.spec.duration && self.from != self.to
    }

    fn retarget(&mut self, target: f32, spec: Transition) {
        if self.to == target {
            return;
        }
        self.from = self.value();
        self.to = target;
        self.started = Instant::now();
        self.spec = spec;
    }
}

/// Animates toward `target`. The first render settles at `target` instantly,
/// matching CSS where transitions only run on later style changes.
pub(crate) fn tween(
    window: &mut Window,
    cx: &mut App,
    key: impl Into<ElementId>,
    target: f32,
    spec: Transition,
) -> f32 {
    tween_from(window, cx, key, target, target, spec)
}

/// Like [`tween`], but a newly mounted element starts at `initial` and
/// animates to `target` (CSS `@keyframes` entrance).
pub(crate) fn tween_from(
    window: &mut Window,
    cx: &mut App,
    key: impl Into<ElementId>,
    initial: f32,
    target: f32,
    spec: Transition,
) -> f32 {
    let state = window.use_keyed_state(key, cx, |_, _| {
        let mut tween = Tween::settled(initial);
        tween.retarget(target, spec);
        tween
    });
    let (value, moving) = state.update(cx, |tween, _| {
        tween.retarget(target, spec);
        (tween.value(), tween.is_moving())
    });
    if moving {
        window.request_animation_frame();
    }
    value
}

/// Pointer state for an interactive element, kept across frames.
#[derive(Default, Clone, Copy, PartialEq)]
pub(crate) struct Pointer {
    pub hovered: bool,
    pub pressed: bool,
}

pub(crate) fn pointer(window: &mut Window, cx: &mut App, key: impl Into<ElementId>) -> Entity<Pointer> {
    window.use_keyed_state(key, cx, |_, _| Pointer::default())
}

pub(crate) fn set_hovered(state: &Entity<Pointer>, hovered: bool, cx: &mut App) {
    state.update(cx, |pointer, cx| {
        if pointer.hovered != hovered {
            pointer.hovered = hovered;
            if !hovered {
                pointer.pressed = false;
            }
            cx.notify();
        }
    });
}

pub(crate) fn set_pressed(state: &Entity<Pointer>, pressed: bool, cx: &mut App) {
    state.update(cx, |pointer, cx| {
        if pointer.pressed != pressed {
            pointer.pressed = pressed;
            cx.notify();
        }
    });
}

/// Requests another frame from inside a view while `condition` holds.
pub(crate) fn keep_animating<V>(condition: bool, window: &mut Window, _cx: &mut Context<V>) {
    if condition {
        window.request_animation_frame();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_matches_css_midpoint() {
        // cubic-bezier(.25,.1,.25,1) at 0.5 is ~0.8024.
        assert!((Easing::Ease.apply(0.5) - 0.8024).abs() < 0.002);
    }

    #[test]
    fn linear_bezier_is_identity() {
        assert!((cubic_bezier(0., 0., 1., 1., 0.3) - 0.3).abs() < 1e-3);
    }
}
