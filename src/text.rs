//! Single-line text rendered with a system font into an alpha mask.

use std::fs;
use std::process::{Command, Stdio};

use fontdue::{Font, FontSettings};

const ELLIPSIS: char = '…';
const FALLBACK_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/ubuntu/UbuntuSans[wdth,wght].ttf",
    "/usr/share/fonts/truetype/ubuntu/Ubuntu[wdth,wght].ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
];

/// An 8-bit coverage mask, row-major.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextMask {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

pub struct TextRenderer {
    font: Font,
}

impl TextRenderer {
    /// Loads the desktop's sans-serif font, or a known fallback.
    #[must_use]
    pub fn load() -> Option<Self> {
        let preferred = Command::new("fc-match")
            .args(["-f", "%{file}", "sans-serif:medium"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
        preferred
            .iter()
            .map(String::as_str)
            .chain(FALLBACK_FONTS.iter().copied())
            .find_map(|path| {
                let bytes = fs::read(path).ok()?;
                Font::from_bytes(bytes, FontSettings::default()).ok()
            })
            .map(|font| Self { font })
    }

    /// Renders `text` at `size` pixels, vertically centred in `height` and
    /// cut with an ellipsis to fit `max_width`.
    #[must_use]
    pub fn render(&self, text: &str, size: f32, max_width: u32, height: u32) -> TextMask {
        let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
        let fitted = self.fit(&chars, size, max_width);
        let width = self.advance(&fitted, size).ceil().max(1.0);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let width = (width as u32).min(max_width.max(1));
        let mut mask = TextMask {
            width,
            height,
            pixels: vec![0; (width * height) as usize],
        };
        let Some(line) = self.font.horizontal_line_metrics(size) else {
            return mask;
        };
        // Centre the ascent..descent box; descent is negative.
        let baseline = (f32::from(u16::try_from(height).unwrap_or(u16::MAX)) + line.ascent
            + line.descent)
            / 2.0;
        let mut pen = 0.0_f32;
        let mut previous = None;
        for &c in &fitted {
            if let Some(previous) = previous {
                pen += self.font.horizontal_kern(previous, c, size).unwrap_or(0.0);
            }
            let (metrics, bitmap) = self.font.rasterize(c, size);
            #[allow(clippy::cast_possible_truncation)]
            let left = (pen.round() as i32) + metrics.xmin;
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let top = (baseline.round() as i32) - metrics.height as i32 - metrics.ymin;
            blit(&mut mask, left, top, metrics.width, metrics.height, &bitmap);
            pen += metrics.advance_width;
            previous = Some(c);
        }
        mask
    }

    fn fit(&self, chars: &[char], size: f32, max_width: u32) -> Vec<char> {
        #[allow(clippy::cast_precision_loss)]
        let limit = max_width as f32;
        if self.advance(chars, size) <= limit {
            return chars.to_vec();
        }
        let mut kept = chars.to_vec();
        while !kept.is_empty() {
            kept.pop();
            while kept.last().is_some_and(|c| c.is_whitespace()) {
                kept.pop();
            }
            let mut candidate = kept.clone();
            candidate.push(ELLIPSIS);
            if self.advance(&candidate, size) <= limit {
                return candidate;
            }
        }
        vec![ELLIPSIS]
    }

    fn advance(&self, chars: &[char], size: f32) -> f32 {
        let mut width = 0.0;
        let mut previous = None;
        for &c in chars {
            if let Some(previous) = previous {
                width += self.font.horizontal_kern(previous, c, size).unwrap_or(0.0);
            }
            width += self.font.metrics(c, size).advance_width;
            previous = Some(c);
        }
        width
    }
}

fn blit(mask: &mut TextMask, left: i32, top: i32, width: usize, height: usize, glyph: &[u8]) {
    for row in 0..height {
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let y = top + row as i32;
        if y < 0 || y >= i32::try_from(mask.height).unwrap_or(i32::MAX) {
            continue;
        }
        for column in 0..width {
            #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
            let x = left + column as i32;
            if x < 0 || x >= i32::try_from(mask.width).unwrap_or(i32::MAX) {
                continue;
            }
            #[allow(clippy::cast_sign_loss)]
            let index = y as usize * mask.width as usize + x as usize;
            mask.pixels[index] = mask.pixels[index].max(glyph[row * width + column]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_to_fit_and_short_text_is_kept() {
        let Some(text) = TextRenderer::load() else {
            return; // No fonts in this environment.
        };
        let short = text.render("Song", 22.0, 400, 40);
        assert!(short.width < 400);
        assert!(short.pixels.iter().any(|alpha| *alpha > 0));
        let long = text.render(&"Very long title ".repeat(20), 22.0, 300, 40);
        assert!(long.width <= 300);
        assert_eq!(long.pixels.len(), (long.width * long.height) as usize);
    }
}
