//! Read-only sysfs levels used to place the slider knob. Writes never happen
//! here: display steps go through brightness keys and keyboard levels through
//! the hardware service.

use std::fs;
use std::path::{Path, PathBuf};

const BACKLIGHT_CLASS: &str = "/sys/class/backlight";
const LED_CLASS: &str = "/sys/class/leds";
const KEYBOARD_FUNCTION: &str = "kbd_backlight";

/// One world-readable brightness attribute pair.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LevelSource {
    brightness: PathBuf,
    maximum: u32,
}

impl LevelSource {
    /// The single display backlight, or `None` when absent or ambiguous
    /// (the same rule t1bridge uses to advertise the control).
    #[must_use]
    pub fn display() -> Option<Self> {
        Self::discover(Path::new(BACKLIGHT_CLASS), |_| true)
    }

    /// The single keyboard backlight LED.
    #[must_use]
    pub fn keyboard() -> Option<Self> {
        Self::discover(Path::new(LED_CLASS), |name| {
            name == KEYBOARD_FUNCTION
                || name
                    .rsplit_once(':')
                    .is_some_and(|(_, function)| function == KEYBOARD_FUNCTION)
        })
    }

    fn discover(root: &Path, accepts: impl Fn(&str) -> bool) -> Option<Self> {
        let mut found = None;
        for entry in fs::read_dir(root).ok()?.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if name.starts_with('.') || !accepts(name) {
                continue;
            }
            if found.replace(entry.path()).is_some() {
                return None;
            }
        }
        let directory = found?;
        let maximum = read_number(&directory.join("max_brightness")).filter(|value| *value > 0)?;
        Some(Self {
            brightness: directory.join("brightness"),
            maximum,
        })
    }

    /// Current level as a fraction 0.0..=1.0.
    #[must_use]
    pub fn fraction(&self) -> Option<f64> {
        let value = read_number(&self.brightness)?;
        Some((f64::from(value) / f64::from(self.maximum)).clamp(0.0, 1.0))
    }
}

/// GNOME 50's brightness slider against the gmux backlight of this Mac,
/// measured at 0/25/50/75/100% (GNOME uses a perceptual scale and its 0%
/// keeps the backlight on).
const GNOME_TO_HARDWARE: [(f64, f64); 5] =
    [(0.0, 0.172), (0.25, 0.351), (0.5, 0.659), (0.75, 0.899), (1.0, 1.0)];

fn interpolate(value: f64, table: impl Iterator<Item = (f64, f64)> + Clone) -> f64 {
    let value = value.clamp(0.0, 1.0);
    let points: Vec<_> = table.collect();
    if let Some(&(x0, y0)) = points.first()
        && value <= x0
    {
        return y0;
    }
    for pair in points.windows(2) {
        let ((x0, y0), (x1, y1)) = (pair[0], pair[1]);
        if value <= x1 {
            return y0 + (y1 - y0) * (value - x0) / (x1 - x0);
        }
    }
    value
}

/// Display level as GNOME shows it, from the hardware fraction.
#[must_use]
pub fn display_to_gnome(hardware: f64) -> f64 {
    interpolate(hardware, GNOME_TO_HARDWARE.iter().map(|&(g, h)| (h, g)))
}

/// Hardware fraction for a level on GNOME's scale.
#[must_use]
pub fn display_from_gnome(gnome: f64) -> f64 {
    interpolate(gnome, GNOME_TO_HARDWARE.iter().copied())
}

fn read_number(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class_with(entries: &[(&str, u32, u32)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "touchbar-levels-{}-{}",
            std::process::id(),
            entries.len()
        ));
        let _ = fs::remove_dir_all(&root);
        for (name, value, maximum) in entries {
            let directory = root.join(name);
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("brightness"), format!("{value}\n")).unwrap();
            fs::write(directory.join("max_brightness"), format!("{maximum}\n")).unwrap();
        }
        root
    }

    #[test]
    fn unique_source_reports_its_fraction() {
        let root = class_with(&[("synthetic::kbd_backlight", 64, 256)]);
        let source = LevelSource::discover(&root, |name| name.ends_with("kbd_backlight")).unwrap();
        assert!((source.fraction().unwrap() - 0.25).abs() < 1e-9);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn gnome_scale_round_trips() {
        assert!((display_from_gnome(0.5) - 0.659).abs() < 1e-9);
        assert!((display_to_gnome(0.659) - 0.5).abs() < 1e-9);
        assert!(display_to_gnome(0.05).abs() < 1e-9, "below GNOME's 0% reads as 0");
        for value in [0.0, 0.1, 0.3, 0.62, 0.9, 1.0] {
            assert!((display_to_gnome(display_from_gnome(value)) - value).abs() < 1e-9);
        }
    }

    #[test]
    fn ambiguous_sources_are_rejected() {
        let root = class_with(&[("a", 1, 2), ("b", 1, 2)]);
        assert!(LevelSource::discover(&root, |_| true).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
