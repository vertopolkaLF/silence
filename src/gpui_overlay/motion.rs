use std::time::{Duration, Instant};

/// Retarget from the value currently on screen, including interrupted transitions.
pub(super) struct Motion {
    pub from: f32,
    pub to: f32,
    started: Instant,
    duration: Duration,
    ease: fn(f32) -> f32,
}

impl Motion {
    pub fn new(value: f32, duration_ms: u64) -> Self {
        Self {
            from: value,
            to: value,
            started: Instant::now(),
            duration: Duration::from_millis(duration_ms),
            ease: |t| 1.0 - (1.0 - t).powi(3),
        }
    }

    /// Long, soft landing for travel across large distances.
    pub fn decelerate(value: f32, duration_ms: u64) -> Self {
        Self {
            ease: |t| 1.0 - (1.0 - t).powi(5),
            ..Self::new(value, duration_ms)
        }
    }

    pub fn progress(&self, now: Instant) -> f32 {
        (now.saturating_duration_since(self.started).as_secs_f32() / self.duration.as_secs_f32())
            .clamp(0.0, 1.0)
    }

    pub fn value(&self, now: Instant) -> f32 {
        let t = self.progress(now);
        self.from + (self.to - self.from) * (self.ease)(t)
    }

    pub fn retarget(&mut self, value: f32, now: Instant) {
        if (self.to - value).abs() < 0.001 {
            return;
        }
        self.from = self.value(now);
        self.to = value;
        self.started = now;
    }

    pub fn retarget_with_duration(&mut self, value: f32, now: Instant, duration_ms: u64) {
        if (self.to - value).abs() < 0.001 {
            return;
        }
        self.retarget(value, now);
        self.duration = Duration::from_millis(duration_ms);
    }

    pub fn active(&self, now: Instant) -> bool {
        (self.to - self.from).abs() > 0.001 && self.progress(now) < 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticker_reversal_keeps_position_when_duration_changes() {
        let mut motion = Motion::decelerate(1., 560);
        let start = Instant::now();
        motion.retarget_with_duration(0., start, 560);
        let reverse = start + Duration::from_millis(140);
        let before = motion.value(reverse);
        motion.retarget_with_duration(1., reverse, 440);
        assert!((before - motion.value(reverse)).abs() < 0.0001);
        assert!(motion.value(reverse + Duration::from_millis(60)) > before);
        assert_eq!(motion.value(reverse + Duration::from_millis(440)), 1.);
    }

    #[test]
    fn reversing_does_not_jump_or_wait_for_the_old_animation() {
        let mut motion = Motion::new(0.0, 200);
        let start = Instant::now();
        motion.retarget(1.0, start);
        let middle = start + Duration::from_millis(80);
        let before = motion.value(middle);
        motion.retarget(0.0, middle);
        assert!((before - motion.value(middle)).abs() < 0.0001);
        assert!(motion.value(middle + Duration::from_millis(50)) < before);
        assert_eq!(motion.value(middle + Duration::from_millis(200)), 0.0);
    }

    #[test]
    fn repeated_visibility_updates_do_not_restart_the_clock() {
        let mut motion = Motion::new(0.0, 200);
        let start = Instant::now();
        motion.retarget(1.0, start);
        motion.retarget(1.0, start + Duration::from_millis(150));
        assert_eq!(motion.value(start + Duration::from_millis(200)), 1.0);
        assert!(!motion.active(start + Duration::from_millis(200)));
    }

    #[test]
    fn reversing_crossfade_keeps_total_opacity_constant() {
        let start = Instant::now();
        let mut live = Motion::new(1.0, 220);
        let mut muted = Motion::new(0.0, 220);
        live.retarget(0.0, start);
        muted.retarget(1.0, start);
        let reverse = start + Duration::from_millis(70);
        live.retarget(1.0, reverse);
        muted.retarget(0.0, reverse);
        for ms in [0, 16, 50, 100, 220] {
            let now = reverse + Duration::from_millis(ms);
            assert!((live.value(now) + muted.value(now) - 1.0).abs() < 0.0001);
        }
    }
}
