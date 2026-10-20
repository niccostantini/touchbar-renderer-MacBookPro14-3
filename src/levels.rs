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
    fn ambiguous_sources_are_rejected() {
        let root = class_with(&[("a", 1, 2), ("b", 1, 2)]);
        assert!(LevelSource::discover(&root, |_| true).is_none());
        fs::remove_dir_all(root).unwrap();
    }
}
