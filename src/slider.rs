//! Expanded-slider state and the per-control value writers. Everything here
//! is pure (time and readings are passed in) so it is unit tested directly.

use std::time::Duration;

use t1_touchbar::gesture::ContactId;
use t1_touchbar::stock_slice::StepDirection;

use crate::bar::SliderKind;

/// The slider closes this long after the last touch, like macOS.
pub const AUTO_CLOSE_AFTER: Duration = Duration::from_secs(4);

#[derive(Clone, Copy, Debug, PartialEq)]
enum DragMode {
    /// Started on the strip button: the value follows finger movement from
    /// where it was, so pressing does not jump the level.
    Relative { base: f64, start_x: f64 },
    /// Started on the track: the knob goes under the finger.
    Absolute,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Drag {
    contact: ContactId,
    mode: DragMode,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Open {
    kind: SliderKind,
    fraction: f64,
    drag: Option<Drag>,
    last_activity: Duration,
}

/// Which slider, if any, is expanded, and the knob position.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SliderState {
    open: Option<Open>,
}

impl SliderState {
    #[must_use]
    pub fn kind(&self) -> Option<SliderKind> {
        self.open.map(|open| open.kind)
    }

    #[must_use]
    pub fn fraction(&self) -> f64 {
        self.open.map_or(0.0, |open| open.fraction)
    }

    #[must_use]
    pub fn dragging(&self) -> bool {
        self.open.is_some_and(|open| open.drag.is_some())
    }

    pub fn open(&mut self, kind: SliderKind, current: f64, now: Duration) {
        self.open = Some(Open {
            kind,
            fraction: current.clamp(0.0, 1.0),
            drag: None,
            last_activity: now,
        });
    }

    pub fn close(&mut self) {
        self.open = None;
    }

    /// Starts tracking `contact` for a drag relative to the current value.
    pub fn begin_relative(&mut self, contact: ContactId, x: f64) {
        if let Some(open) = self.open.as_mut() {
            open.drag = Some(Drag {
                contact,
                mode: DragMode::Relative {
                    base: open.fraction,
                    start_x: x,
                },
            });
        }
    }

    /// Starts an absolute drag and moves the knob to `fraction`.
    pub fn begin_absolute(&mut self, contact: ContactId, fraction: f64, now: Duration) -> bool {
        let Some(open) = self.open.as_mut() else {
            return false;
        };
        open.drag = Some(Drag {
            contact,
            mode: DragMode::Absolute,
        });
        open.last_activity = now;
        open.fraction = fraction.clamp(0.0, 1.0);
        true
    }

    /// Moves the dragged knob. `absolute` is the track position under the
    /// finger and `track_width` converts relative movement. Returns the new
    /// value when `contact` is the one being tracked and the value changed.
    pub fn drag_to(
        &mut self,
        contact: ContactId,
        x: f64,
        absolute: f64,
        track_width: f64,
        now: Duration,
    ) -> Option<f64> {
        let open = self.open.as_mut()?;
        let drag = open.drag.filter(|drag| drag.contact == contact)?;
        let fraction = match drag.mode {
            DragMode::Relative { base, start_x } => base + (x - start_x) / track_width.max(1.0),
            DragMode::Absolute => absolute,
        }
        .clamp(0.0, 1.0);
        open.last_activity = now;
        if (fraction - open.fraction).abs() < f64::EPSILON {
            return None;
        }
        open.fraction = fraction;
        Some(fraction)
    }

    pub fn release(&mut self, contact: ContactId, now: Duration) {
        if let Some(open) = self.open.as_mut()
            && open.drag.is_some_and(|drag| drag.contact == contact)
        {
            open.drag = None;
            open.last_activity = now;
        }
    }

    /// Restarts the auto-close countdown without changing the value.
    pub fn touch(&mut self, now: Duration) {
        if let Some(open) = self.open.as_mut() {
            open.last_activity = now;
        }
    }

    /// Shows a level measured elsewhere while the user is not dragging.
    pub fn show_external(&mut self, kind: SliderKind, fraction: f64) -> bool {
        match self.open.as_mut() {
            Some(open) if open.kind == kind && open.drag.is_none() => {
                let fraction = fraction.clamp(0.0, 1.0);
                let changed = (open.fraction - fraction).abs() > f64::EPSILON;
                open.fraction = fraction;
                changed
            }
            _ => false,
        }
    }

    /// Closes the slider after the idle timeout; returns whether it closed.
    pub fn expire(&mut self, now: Duration) -> bool {
        let expired = self.open.is_some_and(|open| {
            open.drag.is_none() && now.saturating_sub(open.last_activity) >= AUTO_CLOSE_AFTER
        });
        if expired {
            self.open = None;
        }
        expired
    }
}

/// Sends at most one value per interval and always ends on the newest one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Throttle {
    pending: Option<u8>,
    last_sent: Option<u8>,
    last_at: Option<Duration>,
}

impl Throttle {
    pub fn set(&mut self, percent: u8) {
        if self.last_sent == Some(percent) && self.pending.is_none() {
            return;
        }
        self.pending = Some(percent);
    }

    /// Returns the value to send now, if any.
    pub fn take(&mut self, now: Duration, interval: Duration) -> Option<u8> {
        let percent = self.pending?;
        if self
            .last_at
            .is_some_and(|last| now.saturating_sub(last) < interval)
        {
            return None;
        }
        self.pending = None;
        if self.last_sent == Some(percent) {
            return None;
        }
        self.last_sent = Some(percent);
        self.last_at = Some(now);
        Some(percent)
    }
}

/// Initial guess for how much one brightness key moves the backlight.
const DEFAULT_STEP: f64 = 1.0 / 16.0;
/// A step is complete once the level stopped changing for this long...
const SETTLE_QUIET: Duration = Duration::from_millis(120);
/// ...or when nothing happened at all within this time.
const SETTLE_TIMEOUT: Duration = Duration::from_millis(800);
/// Wait per press while leaving a saturated limit.
const LIMIT_SETTLE: Duration = Duration::from_millis(250);
const CHANGE_EPSILON: f64 = 0.002;
const TARGET_TIMEOUT: Duration = Duration::from_secs(6);
const EXTREME_PRESSES: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Awaiting {
    before: f64,
    /// Sent while the backlight sat at its maximum, moving down.
    leaving_limit: bool,
    sent_at: Duration,
    last_value: f64,
    last_change_at: Option<Duration>,
}

/// Drives the display backlight toward a target with brightness-key steps.
///
/// GNOME owns the panel backlight, so only relative key presses work. Each
/// press is awaited until the level settles; the observed change teaches the
/// step size so the stepper stops within half a step of the target.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayStepper {
    target: Option<f64>,
    step: f64,
    awaiting: Option<Awaiting>,
    stalled: u8,
    /// When the target was set or last made progress: a target with no
    /// progress for TARGET_TIMEOUT is dropped and the slider shows the real level.
    target_since: Duration,
    extra_presses: u8,
}

impl Default for DisplayStepper {
    fn default() -> Self {
        Self {
            target: None,
            step: DEFAULT_STEP,
            awaiting: None,
            stalled: 0,
            target_since: Duration::ZERO,
            extra_presses: 0,
        }
    }
}

impl DisplayStepper {
    /// `extreme` asks for a couple of extra key presses once the hardware
    /// stops moving: GNOME's own 0%/100% can lie past the hardware's limits.
    #[cfg(test)]
    pub fn set_target(&mut self, fraction: f64, now: Duration) {
        self.set_target_with_extremes(fraction, now, false);
    }

    pub fn set_target_with_extremes(&mut self, fraction: f64, now: Duration, extreme: bool) {
        self.extra_presses = if extreme { EXTREME_PRESSES } else { 0 };
        self.target = Some(fraction.clamp(0.0, 1.0));
        self.target_since = now;
        self.stalled = 0;
    }

    #[must_use]
    pub fn target(&self) -> Option<f64> {
        self.target
    }

    #[must_use]
    pub fn busy(&self) -> bool {
        self.target.is_some() || self.awaiting.is_some()
    }

    /// Feeds the current level; returns a key to press now, if any.
    pub fn poll(&mut self, now: Duration, actual: f64) -> Option<StepDirection> {
        if self.target.is_some() && now.saturating_sub(self.target_since) >= TARGET_TIMEOUT {
            self.target = None;
            self.awaiting = None;
            return None;
        }
        if let Some(mut awaiting) = self.awaiting {
            if (actual - awaiting.last_value).abs() > CHANGE_EPSILON {
                awaiting.last_value = actual;
                awaiting.last_change_at = Some(now);
                self.awaiting = Some(awaiting);
                return None;
            }
            match awaiting.last_change_at {
                Some(changed) if now.saturating_sub(changed) >= SETTLE_QUIET => {
                    let moved = (actual - awaiting.before).abs();
                    self.step = (self.step + moved.clamp(0.01, 0.25)) / 2.0;
                    self.stalled = 0;
                    // Progress: the timeout counts time without progress, so a
                    // long change (many animated steps) can take as long as it needs.
                    self.target_since = now;
                    self.awaiting = None;
                }
                // Leaving a hardware limit: GNOME's level moves past the point
                // where the backlight saturates, so the first presses show no
                // change. That is expected, not a lost key: move on quickly.
                None if awaiting.leaving_limit
                    && now.saturating_sub(awaiting.sent_at) >= LIMIT_SETTLE =>
                {
                    self.awaiting = None;
                }
                None if now.saturating_sub(awaiting.sent_at) >= SETTLE_TIMEOUT => {
                    // No effect: at the limit, or the key was not applied.
                    self.stalled += 1;
                    self.awaiting = None;
                    if self.stalled >= 2 {
                        self.target = None;
                        self.stalled = 0;
                        return None;
                    }
                }
                _ => return None,
            }
        }
        let target = self.target?;
        let delta = target - actual;
        let direction = if delta > 0.0 {
            StepDirection::Up
        } else {
            StepDirection::Down
        };
        let at_limit = match direction {
            StepDirection::Up => actual >= 1.0 - CHANGE_EPSILON,
            StepDirection::Down => actual <= CHANGE_EPSILON,
        };
        if delta.abs() <= self.step / 2.0 || at_limit {
            if self.extra_presses > 0 {
                self.extra_presses -= 1;
                // Keep pushing towards the requested end.
                return Some(if target >= 0.5 {
                    StepDirection::Up
                } else {
                    StepDirection::Down
                });
            }
            self.target = None;
            return None;
        }
        // Only the top saturates (GNOME's 0% keeps the backlight on).
        let leaving_limit = direction == StepDirection::Down && actual >= 1.0 - CHANGE_EPSILON;
        self.awaiting = Some(Awaiting {
            before: actual,
            leaving_limit,
            sent_at: now,
            last_value: actual,
            last_change_at: None,
        });
        Some(direction)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    #[test]
    fn relative_drag_moves_from_the_current_level() {
        let mut slider = SliderState::default();
        slider.open(SliderKind::Volume, 0.4, ms(0));
        slider.begin_relative(7, 1_000.0);
        assert_eq!(slider.drag_to(7, 1_000.0, 0.9, 1_000.0, ms(10)), None);
        let value = slider.drag_to(7, 1_100.0, 0.9, 1_000.0, ms(20)).unwrap();
        assert!((value - 0.5).abs() < 1e-9);
        // Another finger does not move the knob.
        assert_eq!(slider.drag_to(8, 0.0, 0.0, 1_000.0, ms(30)), None);
        let value = slider.drag_to(7, -5_000.0, 0.0, 1_000.0, ms(40)).unwrap();
        assert!(value.abs() < f64::EPSILON);
    }

    #[test]
    fn absolute_drag_follows_the_finger() {
        let mut slider = SliderState::default();
        slider.open(SliderKind::Display, 0.1, ms(0));
        assert!(slider.begin_absolute(3, 0.7, ms(5)));
        assert!((slider.fraction() - 0.7).abs() < f64::EPSILON);
        assert_eq!(slider.drag_to(3, 0.0, 0.25, 1.0, ms(6)), Some(0.25));
    }

    #[test]
    fn auto_close_waits_for_release_and_idle_time() {
        let mut slider = SliderState::default();
        slider.open(SliderKind::Volume, 0.5, ms(0));
        slider.begin_relative(1, 0.0);
        assert!(!slider.expire(ms(10_000)), "never closes under a finger");
        slider.release(1, ms(10_000));
        assert!(!slider.expire(ms(13_999)));
        assert!(slider.expire(ms(14_000)));
        assert_eq!(slider.kind(), None);
    }

    #[test]
    fn external_levels_do_not_fight_a_drag() {
        let mut slider = SliderState::default();
        slider.open(SliderKind::Volume, 0.5, ms(0));
        assert!(slider.show_external(SliderKind::Volume, 0.6));
        assert!(!slider.show_external(SliderKind::Display, 0.1));
        slider.begin_relative(1, 0.0);
        assert!(!slider.show_external(SliderKind::Volume, 0.9));
        assert!((slider.fraction() - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn throttle_coalesces_to_the_newest_value() {
        let mut throttle = Throttle::default();
        let interval = ms(30);
        throttle.set(10);
        assert_eq!(throttle.take(ms(0), interval), Some(10));
        throttle.set(11);
        throttle.set(12);
        assert_eq!(throttle.take(ms(10), interval), None);
        assert_eq!(throttle.take(ms(30), interval), Some(12));
        throttle.set(12);
        assert_eq!(throttle.take(ms(100), interval), None);
    }

    /// Simulated GNOME: each key moves the level by `step` after `lag`.
    fn run_stepper(start: f64, target: f64, step: f64) -> (f64, usize) {
        let mut stepper = DisplayStepper::default();
        stepper.set_target(target, Duration::ZERO);
        let mut level = start;
        let mut pending: Vec<(Duration, f64)> = Vec::new();
        let mut presses = 0;
        for tick in 0..2_000 {
            let now = ms(tick * 10);
            pending.retain(|(at, delta)| {
                if *at <= now {
                    level = (level + delta).clamp(0.0, 1.0);
                    false
                } else {
                    true
                }
            });
            if let Some(direction) = stepper.poll(now, level) {
                presses += 1;
                let delta = match direction {
                    StepDirection::Up => step,
                    StepDirection::Down => -step,
                };
                pending.push((now + ms(20), delta));
            }
            if !stepper.busy() && pending.is_empty() {
                break;
            }
        }
        (level, presses)
    }

    #[test]
    fn stepper_reaches_the_target_without_oscillating() {
        for step in [0.05, 1.0 / 16.0, 0.1] {
            let (level, presses) = run_stepper(1.0, 0.3, step);
            assert!((level - 0.3).abs() <= step / 2.0 + 1e-9, "step {step}: {level}");
            assert!(presses <= (0.7 / step).ceil() as usize + 1, "step {step}: {presses}");
        }
    }

    #[test]
    fn a_long_change_is_not_cut_by_the_timeout() {
        // 0 -> 1 with small steps that each take 600 ms to settle: well over
        // TARGET_TIMEOUT in total, but always making progress.
        let mut stepper = DisplayStepper::default();
        stepper.set_target(1.0, Duration::ZERO);
        let mut level: f64 = 0.0;
        let mut pending: Option<Duration> = None;
        for tick in 0..10_000 {
            let now = ms(tick * 10);
            if pending.is_some_and(|at| at <= now) {
                level = (level + 0.04).min(1.0);
                pending = None;
            }
            if stepper.poll(now, level).is_some() {
                pending = Some(now + ms(600));
            }
            if !stepper.busy() && pending.is_none() {
                break;
            }
        }
        assert!(level > 0.97, "{level}");
    }

    #[test]
    fn leaving_the_top_is_not_slowed_by_silent_presses() {
        // GNOME saturates the backlight: the first two Down presses from the
        // top change nothing, then each press lowers it by 0.05.
        let mut stepper = DisplayStepper::default();
        stepper.set_target(0.8, Duration::ZERO);
        let mut level: f64 = 1.0;
        let mut silent = 2;
        let mut finished = None;
        for tick in 0..2_000 {
            let now = ms(tick * 10);
            if let Some(StepDirection::Down) = stepper.poll(now, level) {
                if silent > 0 {
                    silent -= 1;
                } else {
                    level -= 0.05;
                }
            }
            if !stepper.busy() {
                finished = Some(now);
                break;
            }
        }
        assert!((level - 0.8).abs() <= 0.05, "{level}");
        assert!(finished.unwrap() < ms(2_000), "{finished:?}");
    }

    #[test]
    fn stepper_stops_at_the_limit() {
        let (level, presses) = run_stepper(0.95, 1.0, 0.1);
        assert!((level - 1.0).abs() < 1e-9);
        assert!(presses <= 2);
    }
}
