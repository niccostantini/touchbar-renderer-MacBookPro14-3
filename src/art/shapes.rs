//! Antialiased shapes for the slider surface: close glyph, track and knob.

use t1_touchbar::framebuffer::{FramebufferError, Rectangle, Rgb, Xrgb8888Frame};

use super::stock_art::rasterize;

/// Draws a centred "x" whose arms span `size` pixels with stroke `stroke`.
pub(crate) fn draw_close(
    frame: &mut Xrgb8888Frame<'_>,
    bounds: Rectangle,
    size: u32,
    stroke: f64,
    color: Rgb,
) -> Result<(), FramebufferError> {
    let size = size.min(bounds.width).min(bounds.height).max(1);
    let area = Rectangle {
        x: bounds.x + (bounds.width - size) / 2,
        y: bounds.y + (bounds.height - size) / 2,
        width: size,
        height: size,
    };
    let extent = f64::from(size);
    let half = stroke / 2.0;
    let mask = rasterize(size, size, |x, y| {
        // Distance from each diagonal, inset so the stroke ends stay inside.
        let inside = x >= half && y >= half && x <= extent - half && y <= extent - half;
        let main = (x - y).abs() / std::f64::consts::SQRT_2;
        let anti = (x + y - extent).abs() / std::f64::consts::SQRT_2;
        inside && (main <= half || anti <= half)
    });
    frame.blend_mask(area, &mask, color)
}

/// Draws a horizontal pill-shaped track filled up to `fraction` (0..=1).
pub(crate) fn draw_track(
    frame: &mut Xrgb8888Frame<'_>,
    bounds: Rectangle,
    fraction: f64,
    filled: Rgb,
    empty: Rgb,
) -> Result<(), FramebufferError> {
    if bounds.width == 0 || bounds.height == 0 {
        return Ok(());
    }
    let width = f64::from(bounds.width);
    let height = f64::from(bounds.height);
    let radius = height / 2.0;
    let split = (width * fraction.clamp(0.0, 1.0)).round();
    let pill = |x: f64, y: f64| {
        let center_x = clamp_between(x, radius, width - radius);
        let dx = x - center_x;
        let dy = y - radius;
        dx * dx + dy * dy <= radius * radius
    };
    let empty_mask = rasterize(bounds.width, bounds.height, |x, y| x >= split && pill(x, y));
    frame.blend_mask(bounds, &empty_mask, empty)?;
    let filled_mask = rasterize(bounds.width, bounds.height, |x, y| x < split && pill(x, y));
    frame.blend_mask(bounds, &filled_mask, filled)
}

/// Draws a filled circle of `diameter` centred at (`center_x`, `center_y`),
/// clipped to the frame.
pub(crate) fn draw_knob(
    frame: &mut Xrgb8888Frame<'_>,
    center_x: u32,
    center_y: u32,
    diameter: u32,
    color: Rgb,
) -> Result<(), FramebufferError> {
    let diameter = diameter.max(2);
    let radius = diameter / 2;
    let x = center_x.saturating_sub(radius);
    let y = center_y.saturating_sub(radius);
    let width = diameter.min(frame.width().saturating_sub(x));
    let height = diameter.min(frame.height().saturating_sub(y));
    if width == 0 || height == 0 {
        return Ok(());
    }
    let offset_x = f64::from(center_x) - f64::from(x);
    let offset_y = f64::from(center_y) - f64::from(y);
    let r = f64::from(diameter) / 2.0;
    let mask = rasterize(width, height, |px, py| {
        let dx = px - offset_x;
        let dy = py - offset_y;
        dx * dx + dy * dy <= r * r
    });
    frame.blend_mask(
        Rectangle {
            x,
            y,
            width,
            height,
        },
        &mask,
        color,
    )
}

/// Procedural icons for the custom buttons.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Icon {
    Crop,
    Padlock,
    Moon,
    Microphone,
    MicrophoneOff,
    Speaker,
    Bluetooth,
}

/// Draws `icon` centred in `bounds`, `size` pixels square.
pub(crate) fn draw_icon(
    frame: &mut Xrgb8888Frame<'_>,
    bounds: Rectangle,
    size: u32,
    icon: Icon,
    color: Rgb,
) -> Result<(), FramebufferError> {
    let size = size.min(bounds.width).min(bounds.height).max(1);
    let area = Rectangle {
        x: bounds.x + (bounds.width - size) / 2,
        y: bounds.y + (bounds.height - size) / 2,
        width: size,
        height: size,
    };
    let extent = f64::from(size);
    let mask = rasterize(size, size, |x, y| icon_contains(icon, x / extent, y / extent));
    frame.blend_mask(area, &mask, color)
}

/// Icon shapes in unit coordinates (0..1 both ways, y down).
fn icon_contains(icon: Icon, u: f64, v: f64) -> bool {
    let distance = |cx: f64, cy: f64| (u - cx).hypot(v - cy);
    match icon {
        Icon::Crop => {
            // Two interlocking crop corners, as in the usual crop/screenshot glyph.
            let half = 0.055;
            let bar = |a: f64, b: f64, fixed: f64, along: f64, across: f64| {
                (a - half..=b + half).contains(&along) && (across - fixed).abs() <= half
            };
            let first = bar(0.06, 0.74, 0.28, v, u) || bar(0.28, 0.94, 0.74, u, v);
            let second = bar(0.06, 0.74, 0.28, u, v) || bar(0.28, 0.94, 0.74, v, u);
            first || second
        }
        Icon::Padlock => {
            let body = rounded_rect(u, v, (0.16, 0.44, 0.84, 0.96), 0.08);
            let shackle = distance(0.5, 0.42);
            let arc = v <= 0.46 && (0.18..=0.29).contains(&shackle);
            let keyhole = distance(0.5, 0.66) <= 0.07 || ((0.47..=0.53).contains(&u) && (0.66..=0.82).contains(&v));
            (body && !keyhole) || arc
        }
        Icon::Moon => distance(0.5, 0.52) <= 0.42 && distance(0.70, 0.36) > 0.34,
        Icon::Microphone => microphone(u, v),
        Icon::Speaker => {
            // A speaker box outline with woofer ring and tweeter dot.
            let outer = rounded_rect(u, v, (0.18, 0.04, 0.82, 0.96), 0.12);
            let inner = rounded_rect(u, v, (0.27, 0.13, 0.73, 0.87), 0.06);
            let woofer = distance(0.5, 0.62);
            let tweeter = distance(0.5, 0.28) <= 0.075;
            (outer && !inner) || (0.10..=0.18).contains(&woofer) || tweeter
        }
        Icon::Bluetooth => {
            // The Bluetooth rune: a spine and two folded arrows.
            let segment = |(ax, ay): (f64, f64), (bx, by): (f64, f64)| {
                let (dx, dy) = (bx - ax, by - ay);
                let t = (((u - ax) * dx + (v - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                (u - (ax + t * dx)).hypot(v - (ay + t * dy)) <= 0.055
            };
            let (top, bottom) = ((0.5, 0.06), (0.5, 0.94));
            segment(top, bottom)
                || segment(top, (0.76, 0.29))
                || segment((0.76, 0.29), (0.25, 0.71))
                || segment(bottom, (0.76, 0.71))
                || segment((0.76, 0.71), (0.25, 0.29))
        }
        Icon::MicrophoneOff => {
            // Slash from top-left to bottom-right with a background gap.
            let along = (u - v).abs() / std::f64::consts::SQRT_2;
            let in_box = (0.12..=0.88).contains(&u) && (0.08..=0.92).contains(&v);
            let slash = in_box && along <= 0.045;
            let gap = in_box && along <= 0.11;
            slash || (microphone(u, v) && !gap)
        }
    }
}

fn microphone(u: f64, v: f64) -> bool {
    let distance = |cx: f64, cy: f64| (u - cx).hypot(v - cy);
    let capsule = rounded_rect(u, v, (0.36, 0.04, 0.64, 0.62), 0.14);
    // U-shaped holder below the capsule.
    let holder_ring = (0.24..=0.31).contains(&distance(0.5, 0.44));
    let holder = holder_ring && v >= 0.44;
    let stem = (0.465..=0.535).contains(&u) && (0.74..=0.90).contains(&v);
    let base = rounded_rect(u, v, (0.30, 0.86, 0.70, 0.94), 0.04);
    capsule || holder || stem || base
}

fn rounded_rect(u: f64, v: f64, (left, top, right, bottom): (f64, f64, f64, f64), radius: f64) -> bool {
    if u < left || u > right || v < top || v > bottom {
        return false;
    }
    let radius = radius.min((right - left) / 2.0).min((bottom - top) / 2.0);
    let cx = clamp_between(u, left + radius, right - radius);
    let cy = clamp_between(v, top + radius, bottom - radius);
    (u - cx).hypot(v - cy) <= radius
}

/// `f64::clamp` that tolerates bounds crossed by rounding (it panics then).
fn clamp_between(value: f64, low: f64, high: f64) -> f64 {
    if low > high {
        f64::midpoint(low, high)
    } else {
        value.clamp(low, high)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_and_degenerate_shape_rasterizes() {
        for icon in [
            Icon::Crop,
            Icon::Padlock,
            Icon::Moon,
            Icon::Microphone,
            Icon::MicrophoneOff,
            Icon::Speaker,
            Icon::Bluetooth,
        ] {
            for size in [1, 7, 30, 64] {
                let extent = f64::from(size);
                let mask = rasterize(size, size, |x, y| icon_contains(icon, x / extent, y / extent));
                assert_eq!(mask.len(), (size * size) as usize);
            }
        }
        // Thinner than the radius, and bounds crossed by rounding.
        assert!(!rounded_rect(0.5, 0.0, (0.0, 0.86, 1.0, 0.94), 0.04));
        assert!(rounded_rect(0.5, 0.9, (0.30, 0.86, 0.70, 0.94), 0.04));
        assert!((clamp_between(5.0, 0.9, 0.899_999_999_999_999_9) - 0.9).abs() < 1e-9);
    }
}
