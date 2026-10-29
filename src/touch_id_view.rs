//! Leo Camp Touch ID prompt for the authentication states: an Italian label,
//! a spinner while the sensor waits, a check on success and an arrow toward
//! the sensor. Enrollment keeps the stock overlay and its progress line.

use std::f64::consts::TAU;

use t1_touchbar::framebuffer::{FramebufferError, Rectangle, Rgb, Xrgb8888Frame};
use t1_touchbar::overlay_state::OverlayState;

use crate::art::stock_art::rasterize;

/// Layout in native pixels (60 px tall bar), anchored to the right edge.
const NATIVE_HEIGHT: u32 = 60;
const BLOCK: u32 = 440;
const CANCEL: u32 = 56;
const GLYPH: u32 = 30;
const GLYPH_GAP: u32 = 16;
const CHEVRON: u32 = 22;
const CHEVRON_GAP: u32 = 18;
const RIGHT_MARGIN: u32 = 22;
const TEXT_SIZE: f32 = 25.0;
/// One spinner turn.
const SPIN_PERIOD: f64 = 0.9;

const LABEL: Rgb = Rgb { red: 255, green: 255, blue: 255 };
const DIM: Rgb = Rgb { red: 150, green: 150, blue: 150 };
const SPINNER: Rgb = Rgb { red: 255, green: 138, blue: 61 };
const RETRY: Rgb = Rgb { red: 255, green: 79, blue: 109 };
const SUCCESS: Rgb = Rgb { red: 46, green: 232, blue: 111 };

/// Whether this view draws `state`; enrollment stays stock.
#[must_use]
pub const fn handles(state: OverlayState) -> bool {
    !matches!(state, OverlayState::Enrollment)
}

#[must_use]
pub const fn label(state: OverlayState) -> &'static str {
    match state {
        OverlayState::Authenticate | OverlayState::Approve | OverlayState::Enrollment => {
            "Appoggia il dito"
        }
        OverlayState::Retry => "Riprova",
        OverlayState::Success => "Riconosciuto",
    }
}

/// Whether `state` animates (the spinner turns).
#[must_use]
pub const fn spinning(state: OverlayState) -> bool {
    matches!(
        state,
        OverlayState::Authenticate | OverlayState::Approve | OverlayState::Retry
    )
}

const fn cancellable(state: OverlayState) -> bool {
    spinning(state)
}

fn scale(value: u32, height: u32) -> u32 {
    (value * height).div_ceil(NATIVE_HEIGHT)
}

/// Font size for the label at this bar height.
#[must_use]
pub fn text_size(height: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let ratio = height as f32 / NATIVE_HEIGHT as f32;
    TEXT_SIZE * ratio
}

/// Widest label that fits between the glyph and the arrow.
#[must_use]
pub fn text_width(height: u32) -> u32 {
    scale(
        BLOCK - CANCEL - GLYPH - GLYPH_GAP - CHEVRON_GAP - CHEVRON - RIGHT_MARGIN,
        height,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Layout {
    cancel: Rectangle,
    glyph: Rectangle,
    text_left: u32,
    chevron: Rectangle,
}

fn layout(width: u32, height: u32) -> Layout {
    let left = width.saturating_sub(scale(BLOCK, height));
    let square = |x: u32, size: u32| Rectangle {
        x,
        y: (height - size.min(height)) / 2,
        width: size,
        height: size.min(height),
    };
    let cancel = Rectangle {
        x: left,
        y: 0,
        width: scale(CANCEL, height),
        height,
    };
    let glyph = square(cancel.x + cancel.width, scale(GLYPH, height));
    let chevron = square(
        width.saturating_sub(scale(RIGHT_MARGIN + CHEVRON, height)),
        scale(CHEVRON, height),
    );
    Layout {
        cancel,
        glyph,
        text_left: glyph.x + glyph.width + scale(GLYPH_GAP, height),
        chevron,
    }
}

/// Whether a press at `x` cancels the request: anywhere on the prompt, as
/// with the stock overlay.
#[must_use]
pub fn cancels(width: u32, height: u32, state: Option<OverlayState>, x: f64) -> bool {
    state.is_some_and(|state| handles(state) && cancellable(state))
        && x >= f64::from(layout(width, height).cancel.x)
        && x < f64::from(width)
}

/// Draws the prompt for `state`; `seconds` drives the spinner.
pub fn draw(
    frame: &mut Xrgb8888Frame<'_>,
    state: OverlayState,
    text: Option<(&[u8], u32, u32)>,
    seconds: f64,
    opacity: u8,
) -> Result<(), FramebufferError> {
    let (width, height) = (frame.width(), frame.height());
    let layout = layout(width, height);
    if cancellable(state) {
        let size = scale(18, height);
        let area = centered(layout.cancel, size);
        let half = f64::from(scale(2, height).max(1)) / 2.0 + 0.4;
        let extent = f64::from(size);
        blend(frame, area, DIM, opacity, |x, y| {
            let main = (x - y).abs() / std::f64::consts::SQRT_2;
            let anti = (x + y - extent).abs() / std::f64::consts::SQRT_2;
            main <= half || anti <= half
        })?;
    }
    let extent = f64::from(layout.glyph.width);
    match state {
        OverlayState::Success => blend(frame, layout.glyph, SUCCESS, opacity, |x, y| {
            let (u, v) = (x / extent, y / extent);
            segment(u, v, (0.12, 0.52), (0.40, 0.80), 0.075)
                || segment(u, v, (0.40, 0.80), (0.90, 0.22), 0.075)
        })?,
        _ => {
            let start = (seconds / SPIN_PERIOD).fract() * TAU;
            let color = if state == OverlayState::Retry { RETRY } else { SPINNER };
            blend(frame, layout.glyph, color, opacity, |x, y| {
                let (dx, dy) = (x / extent - 0.5, y / extent - 0.5);
                let ring = (0.34..=0.48).contains(&dx.hypot(dy));
                // A 270° arc whose gap turns clockwise.
                let angle = (dy.atan2(dx) - start).rem_euclid(TAU);
                ring && angle < TAU * 0.75
            })?;
        }
    }
    if let Some((pixels, text_width, text_height)) = text {
        let color = match state {
            OverlayState::Retry => RETRY,
            _ => LABEL,
        };
        let area = Rectangle {
            x: layout.text_left,
            y: (height - text_height.min(height)) / 2,
            width: text_width,
            height: text_height.min(height),
        };
        if area.x + area.width <= layout.chevron.x {
            let mut mask = pixels.to_vec();
            fade(&mut mask, opacity);
            frame.blend_mask(area, &mask, color)?;
        }
    }
    let extent = f64::from(layout.chevron.width);
    blend(frame, layout.chevron, DIM, opacity, |x, y| {
        let (u, v) = (x / extent, y / extent);
        segment(u, v, (0.30, 0.14), (0.70, 0.50), 0.08)
            || segment(u, v, (0.70, 0.50), (0.30, 0.86), 0.08)
    })
}

fn centered(bounds: Rectangle, size: u32) -> Rectangle {
    let size = size.min(bounds.width).min(bounds.height);
    Rectangle {
        x: bounds.x + (bounds.width - size) / 2,
        y: bounds.y + (bounds.height - size) / 2,
        width: size,
        height: size,
    }
}

fn blend(
    frame: &mut Xrgb8888Frame<'_>,
    area: Rectangle,
    color: Rgb,
    opacity: u8,
    contains: impl Fn(f64, f64) -> bool,
) -> Result<(), FramebufferError> {
    let mut mask = rasterize(area.width, area.height, contains);
    fade(&mut mask, opacity);
    frame.blend_mask(area, &mask, color)
}

fn fade(mask: &mut [u8], opacity: u8) {
    if opacity != u8::MAX {
        for alpha in mask {
            *alpha = u8::try_from((u16::from(*alpha) * u16::from(opacity) + 127) / 255)
                .unwrap_or(u8::MAX);
        }
    }
}

fn segment(u: f64, v: f64, (ax, ay): (f64, f64), (bx, by): (f64, f64), half_width: f64) -> bool {
    let (dx, dy) = (bx - ax, by - ay);
    let t = (((u - ax) * dx + (v - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (u - (ax + t * dx)).hypot(v - (ay + t * dy)) <= half_width
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_fits_the_bar() {
        let layout = layout(2_170, 60);
        assert!(layout.cancel.x > 2_170 / 13, "clear of Esc");
        assert!(layout.text_left + text_width(60) <= layout.chevron.x);
        assert!(layout.chevron.x + layout.chevron.width <= 2_170);
    }

    #[test]
    fn only_waiting_states_cancel() {
        let x = 2_000.0;
        assert!(cancels(2_170, 60, Some(OverlayState::Authenticate), x));
        assert!(!cancels(2_170, 60, Some(OverlayState::Success), x));
        assert!(!cancels(2_170, 60, Some(OverlayState::Enrollment), x));
        assert!(!cancels(2_170, 60, Some(OverlayState::Authenticate), 100.0));
    }
}
