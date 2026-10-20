//! Output volume and microphone state through PipeWire's `wpctl`, plus a
//! cheap recording check: ALSA capture devices are read from
//! `/proc/asound` first, and `pw-dump` only runs while one is open.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::Value;

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";
const ASOUND: &str = "/proc/asound";
const TICK_SOUND: &str = "/usr/share/sounds/freedesktop/stereo/audio-volume-change.oga";

/// Output volume and mute.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Audio {
    pub volume: u8,
    pub muted: bool,
}

/// Microphone state while some application records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Microphone {
    pub muted: bool,
}

#[must_use]
pub fn output() -> Option<Audio> {
    let (volume, muted) = get_volume(SINK)?;
    Some(Audio { volume, muted })
}

/// Sets the output volume, unmuting like GNOME's volume keys do.
pub fn set_volume(percent: u8) {
    wpctl(&["set-volume", SINK, &format!("{}%", percent.min(100))]);
    wpctl(&["set-mute", SINK, "0"]);
}

pub fn toggle_output_mute() {
    wpctl(&["set-mute", SINK, "toggle"]);
}

pub fn toggle_microphone_mute() {
    wpctl(&["set-mute", SOURCE, "toggle"]);
}

/// Plays GNOME's volume feedback sound without waiting for it.
pub fn tick() {
    let _ = Command::new("pw-play")
        .arg(TICK_SOUND)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|mut child| std::thread::spawn(move || child.wait()));
}

/// `Some` while an application records from a real input.
#[must_use]
pub fn microphone() -> Option<Microphone> {
    if !capture_open(Path::new(ASOUND)) {
        return None;
    }
    let output = Command::new("pw-dump")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let objects: Value = serde_json::from_slice(&output.stdout).ok()?;
    if !recording(&objects) {
        return None;
    }
    let (_, muted) = get_volume(SOURCE)?;
    Some(Microphone { muted })
}

fn wpctl(args: &[&str]) {
    let _ = Command::new("wpctl")
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn get_volume(node: &str) -> Option<(u8, bool)> {
    let output = Command::new("wpctl")
        .args(["get-volume", node])
        .env("LC_ALL", "C")
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_volume(&String::from_utf8_lossy(&output.stdout))
}

/// Parses `Volume: 0.40 [MUTED]`.
fn parse_volume(line: &str) -> Option<(u8, bool)> {
    let value: f64 = line.strip_prefix("Volume:")?.split_whitespace().next()?.parse().ok()?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let percent = (value * 100.0).round().clamp(0.0, 100.0) as u8;
    Some((percent, line.contains("[MUTED]")))
}

/// True when any ALSA capture substream is open (not `closed`).
fn capture_open(asound: &Path) -> bool {
    let Ok(cards) = fs::read_dir(asound) else {
        return false;
    };
    cards.flatten().any(|card| {
        let Ok(devices) = fs::read_dir(card.path()) else {
            return false;
        };
        devices.flatten().any(|device| {
            let name = device.file_name();
            let name = name.to_string_lossy();
            if !(name.starts_with("pcm") && name.ends_with('c')) {
                return false;
            }
            fs::read_dir(device.path()).is_ok_and(|subs| {
                subs.flatten().any(|sub| {
                    fs::read_to_string(sub.path().join("status"))
                        .is_ok_and(|status| status.trim() != "closed" && !status.is_empty())
                })
            })
        })
    })
}

/// True when a running capture stream records a real source (not a monitor
/// of an output, as level meters do).
fn recording(objects: &Value) -> bool {
    objects.as_array().is_some_and(|objects| {
        objects.iter().any(|object| {
            let info = &object["info"];
            let props = &info["props"];
            props["media.class"].as_str() == Some("Stream/Input/Audio")
                && info["state"].as_str() == Some("running")
                && props["stream.monitor"].as_bool() != Some(true)
                && props["stream.capture.sink"].as_bool() != Some(true)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn wpctl_volume_lines_are_parsed() {
        assert_eq!(parse_volume("Volume: 0.40\n"), Some((40, false)));
        assert_eq!(parse_volume("Volume: 0.76 [MUTED]\n"), Some((76, true)));
        assert_eq!(parse_volume("Volume: 1.50\n"), Some((100, false)));
        assert_eq!(parse_volume("garbage"), None);
    }

    #[test]
    fn only_running_real_capture_counts_as_recording() {
        let node = |class: &str, state: &str, monitor: bool| {
            json!({"type": "PipeWire:Interface:Node", "info": {
                "state": state,
                "props": {"media.class": class, "stream.monitor": monitor}
            }})
        };
        assert!(recording(&json!([node("Stream/Input/Audio", "running", false)])));
        assert!(!recording(&json!([node("Stream/Input/Audio", "idle", false)])));
        assert!(!recording(&json!([node("Stream/Input/Audio", "running", true)])));
        assert!(!recording(&json!([node("Stream/Output/Audio", "running", false)])));
    }

    #[test]
    fn capture_status_files_are_detected() {
        let root = std::env::temp_dir().join(format!("touchbar-asound-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let sub = root.join("card0/pcm0c/sub0");
        fs::create_dir_all(&sub).unwrap();
        fs::create_dir_all(root.join("card0/pcm0p/sub0")).unwrap();
        fs::write(root.join("card0/pcm0p/sub0/status"), "state: RUNNING\n").unwrap();
        fs::write(sub.join("status"), "closed\n").unwrap();
        assert!(!capture_open(&root), "playback devices do not count");
        fs::write(sub.join("status"), "state: RUNNING\n").unwrap();
        assert!(capture_open(&root));
        fs::remove_dir_all(root).unwrap();
    }
}
