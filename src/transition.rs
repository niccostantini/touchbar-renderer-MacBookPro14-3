//! Cross-fade between two whole frames when the bar's layout changes
//! (slider, output picker, Fn row, contextual buttons appearing).

use std::time::Duration;

/// Default length of a layout change.
pub const LAYOUT_FADE: Duration = Duration::from_millis(240);
/// The Fn row appears while a key is held: keep it snappier.
pub const FN_FADE: Duration = Duration::from_millis(130);

pub struct Crossfade {
    from: Vec<u8>,
    to: Vec<u8>,
    start: Duration,
    duration: Duration,
}

impl Crossfade {
    /// Fades from the pixels currently shown to `to`.
    #[must_use]
    pub fn new(from: Vec<u8>, to: Vec<u8>, start: Duration, duration: Duration) -> Self {
        Self {
            from,
            to,
            start,
            duration,
        }
    }

    /// Retargets a running fade to a newer frame (the state changed again).
    pub fn retarget(&mut self, to: Vec<u8>) {
        self.to = to;
    }

    /// Writes the frame for `now` into `out`; returns true once finished.
    pub fn blend_into(&self, now: Duration, out: &mut [u8]) -> bool {
        let total = self.duration.as_secs_f64().max(1e-3);
        let t = (now.saturating_sub(self.start).as_secs_f64() / total).clamp(0.0, 1.0);
        if t >= 1.0 || self.from.len() != out.len() || self.to.len() != out.len() {
            if self.to.len() == out.len() {
                out.copy_from_slice(&self.to);
            }
            return true;
        }
        // Ease-out cubic: quick start, gentle landing.
        let eased = 1.0 - (1.0 - t).powi(3);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let alpha = (eased * 256.0) as u32;
        let inverse = 256 - alpha;
        for ((pixel, from), to) in out.iter_mut().zip(&self.from).zip(&self.to) {
            #[allow(clippy::cast_possible_truncation)]
            let mixed = ((u32::from(*from) * inverse + u32::from(*to) * alpha) >> 8) as u8;
            *pixel = mixed;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_runs_from_old_to_new_and_ends_on_new() {
        let fade = Crossfade::new(vec![0; 4], vec![200; 4], Duration::ZERO, LAYOUT_FADE);
        let mut out = vec![0; 4];
        assert!(!fade.blend_into(Duration::ZERO, &mut out));
        assert_eq!(out, vec![0; 4]);
        assert!(!fade.blend_into(Duration::from_millis(80), &mut out));
        assert!(out[0] > 100 && out[0] < 200, "{}", out[0]);
        assert!(fade.blend_into(Duration::from_millis(200), &mut out));
        assert_eq!(out, vec![200; 4]);
    }

    #[test]
    fn retarget_keeps_the_start_and_lands_on_the_newest_frame() {
        let mut fade = Crossfade::new(vec![0; 2], vec![100; 2], Duration::ZERO, LAYOUT_FADE);
        fade.retarget(vec![50; 2]);
        let mut out = vec![0; 2];
        assert!(fade.blend_into(Duration::from_secs(1), &mut out));
        assert_eq!(out, vec![50; 2]);
    }
}
