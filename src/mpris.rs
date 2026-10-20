//! Media players over MPRIS, through `busctl --json`. Players are queried
//! when `busctl monitor` reports a change, not on a fixed fast timer.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER_INTERFACE: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Playback {
    Playing,
    Paused,
}

/// The track shown in the middle of the bar.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub bus_name: String,
    pub title: String,
    pub artist: String,
    pub playback: Playback,
    track_id: Option<String>,
    length: Option<Duration>,
    position: Option<Duration>,
    sampled_at: Instant,
}

impl NowPlaying {
    /// "Title · Artist", or just the title.
    #[must_use]
    pub fn label(&self) -> String {
        if self.artist.is_empty() {
            self.title.clone()
        } else {
            format!("{} · {}", self.title, self.artist)
        }
    }

    /// Progress 0..=1 extrapolated from the last sample, when known.
    #[must_use]
    pub fn progress(&self, now: Instant) -> Option<f64> {
        let length = self.length.filter(|length| !length.is_zero())?;
        let mut position = self.position?;
        if self.playback == Playback::Playing {
            position += now.saturating_duration_since(self.sampled_at);
        }
        Some((position.as_secs_f64() / length.as_secs_f64()).clamp(0.0, 1.0))
    }

    #[must_use]
    pub fn seekable(&self) -> bool {
        self.track_id.is_some() && self.length.is_some_and(|length| !length.is_zero())
    }

    /// Whether the bar would draw `other` differently (a fresh position
    /// sample that matches the extrapolation does not count).
    #[must_use]
    pub fn looks_like(&self, other: &Self, now: Instant) -> bool {
        let drift = match (self.progress(now), other.progress(now)) {
            (Some(a), Some(b)) => (a - b).abs() > 0.01,
            (a, b) => a.is_some() != b.is_some(),
        };
        self.bus_name == other.bus_name
            && self.title == other.title
            && self.artist == other.artist
            && self.playback == other.playback
            && self.track_id == other.track_id
            && !drift
    }
}

/// Calls a no-argument player method such as `PlayPause` or `Next`.
pub fn call(bus_name: &str, method: &str) {
    call_with(bus_name, method, &[]);
}

/// Seeks to `fraction` of the track, when the player supports it.
pub fn seek(player: &NowPlaying, fraction: f64) {
    let (Some(track_id), Some(length)) = (player.track_id.as_deref(), player.length) else {
        return;
    };
    #[allow(clippy::cast_possible_truncation)]
    let micros = (length.as_secs_f64() * fraction.clamp(0.0, 1.0) * 1e6) as i64;
    call_with(
        &player.bus_name,
        "SetPosition",
        &["ox", track_id, &micros.to_string()],
    );
}

fn call_with(bus_name: &str, method: &str, args: &[&str]) {
    let _ = Command::new("busctl")
        .arg("--user")
        .args(["call", bus_name, MPRIS_PATH, PLAYER_INTERFACE, method])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Runs `on_change` for every MPRIS signal or player appearing/leaving.
/// Returns when the monitor process ends.
pub fn watch(mut on_change: impl FnMut()) {
    let Ok(mut child) = Command::new("busctl")
        .args([
            "--user",
            "monitor",
            "--json=short",
            "--match",
            &format!("type='signal',path='{MPRIS_PATH}'"),
            "--match",
            "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',\
             arg0namespace='org.mpris.MediaPlayer2'",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    if let Some(stdout) = child.stdout.take() {
        for _ in BufReader::new(stdout).lines().map_while(Result::ok) {
            on_change();
        }
    }
    let _ = child.wait();
}

/// Picks a playing player, else the previously shown one if paused, else
/// any paused one.
pub fn query(preferred: &mut Option<String>) -> Option<NowPlaying> {
    let names = busctl_json(&[
        "call",
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "ListNames",
    ])?;
    let players: Vec<String> = names["data"][0]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .filter(|name| name.starts_with(MPRIS_PREFIX))
        .map(str::to_owned)
        .collect();
    let mut paused = None;
    let mut first_paused = None;
    for name in players {
        let Some(player) = query_player(&name) else {
            continue;
        };
        match player.playback {
            Playback::Playing => {
                *preferred = Some(name);
                return Some(player);
            }
            Playback::Paused if preferred.as_deref() == Some(name.as_str()) => {
                paused = Some(player);
            }
            Playback::Paused => {
                first_paused.get_or_insert(player);
            }
        }
    }
    let shown = paused.or(first_paused);
    *preferred = shown.as_ref().map(|player| player.bus_name.clone());
    shown
}

fn busctl_json(args: &[&str]) -> Option<Value> {
    let output = Command::new("busctl")
        .arg("--user")
        .arg("--json=short")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    serde_json::from_slice(&output.stdout).ok()
}

fn query_player(bus_name: &str) -> Option<NowPlaying> {
    let sampled_at = Instant::now();
    let reply = busctl_json(&[
        "call",
        bus_name,
        MPRIS_PATH,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        "s",
        PLAYER_INTERFACE,
    ])?;
    parse_player(bus_name, &reply["data"][0], sampled_at)
}

/// Parses a `GetAll` reply as printed by `busctl --json`.
fn parse_player(bus_name: &str, properties: &Value, sampled_at: Instant) -> Option<NowPlaying> {
    let playback = match properties["PlaybackStatus"]["data"].as_str()? {
        "Playing" => Playback::Playing,
        "Paused" => Playback::Paused,
        _ => return None,
    };
    let metadata = &properties["Metadata"]["data"];
    let title = metadata["xesam:title"]["data"]
        .as_str()
        .map(str::trim)
        .filter(|title| !title.is_empty())?
        .to_owned();
    let artist = metadata["xesam:artist"]["data"]
        .as_array()
        .map(|artists| {
            artists
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let micros = |value: &Value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().and_then(|value| i64::try_from(value).ok()))
            .and_then(|value| u64::try_from(value).ok())
            .map(Duration::from_micros)
    };
    Some(NowPlaying {
        bus_name: bus_name.to_owned(),
        title,
        artist: artist.trim().to_owned(),
        playback,
        track_id: metadata["mpris:trackid"]["data"]
            .as_str()
            .filter(|id| id.starts_with('/') && !id.contains("NoTrack"))
            .map(str::to_owned),
        length: micros(&metadata["mpris:length"]["data"]),
        position: micros(&properties["Position"]["data"]),
        sampled_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply(status: &str) -> Value {
        json!({
            "PlaybackStatus": {"type": "s", "data": status},
            "Position": {"type": "x", "data": 30_000_000},
            "Metadata": {"type": "a{sv}", "data": {
                "xesam:title": {"type": "s", "data": "Synthetic Song"},
                "xesam:artist": {"type": "as", "data": ["Band", "Guest"]},
                "mpris:length": {"type": "x", "data": 120_000_000},
                "mpris:trackid": {"type": "o", "data": "/org/example/track/1"}
            }}
        })
    }

    #[test]
    fn mpris_reply_is_parsed() {
        let sampled = Instant::now();
        let player = parse_player("org.mpris.MediaPlayer2.example", &reply("Playing"), sampled)
            .unwrap();
        assert_eq!(player.label(), "Synthetic Song · Band, Guest");
        assert!(player.seekable());
        let progress = player.progress(sampled).unwrap();
        assert!((progress - 0.25).abs() < 1e-9);
        let later = player.progress(sampled + Duration::from_secs(30)).unwrap();
        assert!((later - 0.5).abs() < 1e-9);
    }

    #[test]
    fn paused_progress_does_not_advance_and_stopped_is_hidden() {
        let sampled = Instant::now();
        let player = parse_player("p", &reply("Paused"), sampled).unwrap();
        let later = player.progress(sampled + Duration::from_secs(30)).unwrap();
        assert!((later - 0.25).abs() < 1e-9);
        assert!(parse_player("p", &reply("Stopped"), sampled).is_none());
    }

    #[test]
    fn untitled_players_are_ignored() {
        let mut value = reply("Playing");
        value["Metadata"]["data"]["xesam:title"]["data"] = json!("  ");
        assert!(parse_player("p", &value, Instant::now()).is_none());
    }

    #[test]
    fn a_new_sample_on_the_same_track_looks_the_same() {
        let sampled = Instant::now();
        let a = parse_player("p", &reply("Playing"), sampled).unwrap();
        let mut later = reply("Playing");
        later["Position"]["data"] = json!(31_000_000);
        let b = parse_player("p", &later, sampled + Duration::from_secs(1)).unwrap();
        assert!(a.looks_like(&b, sampled + Duration::from_secs(1)));
        let paused = parse_player("p", &reply("Paused"), sampled).unwrap();
        assert!(!a.looks_like(&paused, sampled));
    }
}
