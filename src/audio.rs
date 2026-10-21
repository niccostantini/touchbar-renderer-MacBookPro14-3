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

/// One audio output (PipeWire sink) for the picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Output {
    pub id: u32,
    pub name: String,
    pub active: bool,
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

/// Lists audio outputs with the current default marked, from `pw-dump`.
#[must_use]
pub fn outputs() -> Vec<Output> {
    let Ok(output) = Command::new("pw-dump").stderr(Stdio::null()).output() else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&output.stdout)
        .map(|objects| parse_outputs(&objects))
        .unwrap_or_default()
}

/// Makes `id` the default output.
pub fn set_output(id: u32) {
    wpctl(&["set-default", &id.to_string()]);
}

fn parse_outputs(objects: &Value) -> Vec<Output> {
    let Some(objects) = objects.as_array() else {
        return Vec::new();
    };
    let default = objects
        .iter()
        .filter(|object| object["props"]["metadata.name"].as_str() == Some("default"))
        .filter_map(|object| object["metadata"].as_array())
        .flatten()
        .find(|entry| entry["key"].as_str() == Some("default.audio.sink"))
        .and_then(|entry| {
            entry["value"]["name"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| {
                    // Some versions print the value as a JSON string.
                    let text = entry["value"].as_str()?;
                    let parsed: Value = serde_json::from_str(text).ok()?;
                    parsed["name"].as_str().map(str::to_owned)
                })
        });
    let mut outputs: Vec<Output> = objects
        .iter()
        .filter_map(|object| {
            let props = &object["info"]["props"];
            if props["media.class"].as_str() != Some("Audio/Sink") {
                return None;
            }
            let id = u32::try_from(object["id"].as_u64()?).ok()?;
            let node = props["node.name"].as_str().unwrap_or_default();
            let description = props["node.description"]
                .as_str()
                .or_else(|| props["node.nick"].as_str())
                .unwrap_or(node);
            Some(Output {
                id,
                name: short_name(description),
                active: default.as_deref() == Some(node),
            })
        })
        .collect();
    outputs.sort_by_key(|output| output.id);
    outputs
}

/// Drops the channel/format tail PipeWire appends to device names
/// ("Audio interno Stereo analogico" becomes "Audio interno").
fn short_name(description: &str) -> String {
    const TAILS: [&str; 10] = [
        " Stereo analogico", " Stereo digitale", " Analog Stereo", " Digital Stereo",
        " Surround analogico", " Analog Surround", " Mono analogico", " Analog Mono",
        " Stereo", " (IEC958)",
    ];
    let mut name = description.trim().to_owned();
    for tail in TAILS {
        if let Some(index) = name.find(tail) {
            name.truncate(index);
        }
    }
    let name = name.trim();
    if name.is_empty() { description.trim().to_owned() } else { name.to_owned() }
}

pub fn toggle_output_mute() {
    wpctl(&["set-mute", SINK, "toggle"]);
}

pub fn toggle_microphone_mute() {
    wpctl(&["set-mute", SOURCE, "toggle"]);
}

/// `Some(on)` for the Bluetooth adapter (rfkill soft state), `None` without one.
#[must_use]
pub fn bluetooth() -> Option<bool> {
    let entries = fs::read_dir("/sys/class/rfkill").ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if fs::read_to_string(path.join("type")).ok()?.trim() != "bluetooth" {
            continue;
        }
        let soft = fs::read_to_string(path.join("soft")).ok()?;
        let hard = fs::read_to_string(path.join("hard")).ok()?;
        return Some(soft.trim() == "0" && hard.trim() == "0");
    }
    None
}

/// Turns Bluetooth on or off through GNOME (the same switch as its menu).
pub fn set_bluetooth(on: bool) {
    let _ = Command::new("busctl")
        .args([
            "--user",
            "set-property",
            "org.gnome.SettingsDaemon.Rfkill",
            "/org/gnome/SettingsDaemon/Rfkill",
            "org.gnome.SettingsDaemon.Rfkill",
            "BluetoothAirplaneMode",
            "b",
            if on { "false" } else { "true" },
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
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
    fn outputs_are_listed_with_the_default_marked() {
        let dump = json!([
            {"id": 44, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Audio/Sink", "node.name": "synthetic.speakers",
                "node.description": "Audio interno Stereo analogico"}}},
            {"id": 38, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Audio/Sink", "node.name": "synthetic.dock",
                "node.description": "Example Dock Stereo digitale (IEC958)"}}},
            {"id": 50, "type": "PipeWire:Interface:Node", "info": {"props": {
                "media.class": "Audio/Source", "node.name": "synthetic.mic"}}},
            {"id": 30, "type": "PipeWire:Interface:Metadata",
             "props": {"metadata.name": "default"},
             "metadata": [{"subject": 0, "key": "default.audio.sink",
                           "value": {"name": "synthetic.speakers"}}]}
        ]);
        let outputs = parse_outputs(&dump);
        assert_eq!(
            outputs,
            vec![
                Output { id: 38, name: "Example Dock".into(), active: false },
                Output { id: 44, name: "Audio interno".into(), active: true },
            ]
        );
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
