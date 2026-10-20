//! Desktop state behind the bar, kept by one background thread: the media
//! player (refreshed on MPRIS signals), output volume (every 2 s, faster
//! while the volume slider is open) and a recording microphone (cheap
//! `/proc` check every second), and whether the screen is locked or off
//! (on D-Bus signals).
//!
//! Commands run on helper threads and ask the poller for a refresh, so the
//! render loop never waits on a subprocess.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::audio::{self, Audio, Microphone, Output};
use crate::mpris::{self, NowPlaying};
use crate::screen;

/// Players also re-read without signals, to correct position drift.
const MPRIS_FALLBACK: Duration = Duration::from_secs(10);
const AUDIO_INTERVAL: Duration = Duration::from_secs(2);
const AUDIO_FAST_INTERVAL: Duration = Duration::from_millis(300);
const MICROPHONE_INTERVAL: Duration = Duration::from_secs(1);
/// Output list refresh without a request (plugging a dock changes it).
const OUTPUTS_INTERVAL: Duration = Duration::from_secs(10);
/// Lock state also re-read without signals, in case one is missed.
const SCREEN_FALLBACK: Duration = Duration::from_secs(15);
/// Signals come in bursts (several properties at once); coalesce them.
const DEBOUNCE: Duration = Duration::from_millis(60);
const TICK_INTERVAL: Duration = Duration::from_millis(300);
const WATCH_RESTART: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub now_playing: Option<NowPlaying>,
    pub audio: Option<Audio>,
    /// `Some` only while a capture stream is running.
    pub microphone: Option<Microphone>,
    /// The screen is locked or the monitors are off.
    pub screen_off: bool,
    /// Audio outputs for the picker.
    pub outputs: Vec<Output>,
}

impl Snapshot {
    fn looks_like(&self, other: &Self) -> bool {
        let players = match (&self.now_playing, &other.now_playing) {
            (None, None) => true,
            (Some(a), Some(b)) => a.looks_like(b, Instant::now()),
            _ => false,
        };
        players
            && self.audio == other.audio
            && self.microphone == other.microphone
            && self.screen_off == other.screen_off
            && self.outputs == other.outputs
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Parts {
    players: bool,
    audio: bool,
    microphone: bool,
    screen: bool,
    outputs: bool,
}

const PLAYERS: Parts = Parts {
    players: true,
    audio: false,
    microphone: false,
    screen: false,
    outputs: false,
};
const AUDIO: Parts = Parts {
    players: false,
    audio: true,
    microphone: false,
    screen: false,
    outputs: false,
};
const MICROPHONE: Parts = Parts {
    players: false,
    audio: false,
    microphone: true,
    screen: false,
    outputs: false,
};
const OUTPUTS: Parts = Parts {
    players: false,
    audio: true,
    microphone: false,
    screen: false,
    outputs: true,
};
const SCREEN: Parts = Parts {
    players: false,
    audio: false,
    microphone: false,
    screen: true,
    outputs: false,
};

/// Handle to the poller; cheap to clone.
#[derive(Clone)]
pub struct Context {
    latest: Arc<Mutex<(u64, Snapshot)>>,
    refresh: Sender<Parts>,
    volume: Sender<u8>,
    fast_audio: Arc<AtomicBool>,
}

impl Context {
    #[must_use]
    pub fn start() -> Self {
        let latest = Arc::new(Mutex::new((0, Snapshot::default())));
        let fast_audio = Arc::new(AtomicBool::new(false));
        let (refresh, requests) = mpsc::channel();

        let shared = Arc::clone(&latest);
        let fast = Arc::clone(&fast_audio);
        thread::spawn(move || poll_forever(&shared, &requests, &fast));

        let watcher = refresh.clone();
        thread::spawn(move || {
            loop {
                mpris::watch(|| {
                    let _ = watcher.send(PLAYERS);
                });
                thread::sleep(WATCH_RESTART);
            }
        });
        let watcher = refresh.clone();
        thread::spawn(move || {
            loop {
                screen::watch(|| {
                    let _ = watcher.send(SCREEN);
                });
                thread::sleep(WATCH_RESTART);
            }
        });

        let (volume, volumes) = mpsc::channel();
        let after_volume = refresh.clone();
        thread::spawn(move || volume_worker(&volumes, &after_volume));

        Self {
            latest,
            refresh,
            volume,
            fast_audio,
        }
    }

    /// Returns the snapshot when it changed since `generation`.
    #[must_use]
    pub fn changed_since(&self, generation: u64) -> Option<(u64, Snapshot)> {
        let latest = self.latest.lock().ok()?;
        (latest.0 != generation).then(|| latest.clone())
    }

    /// Polls the volume faster while the volume slider is open.
    pub fn set_fast_audio(&self, fast: bool) {
        if self.fast_audio.swap(fast, Ordering::Relaxed) != fast && fast {
            let _ = self.refresh.send(AUDIO);
        }
    }

    pub fn set_volume(&self, percent: u8) {
        let _ = self.volume.send(percent);
    }

    /// Re-reads the output list now (the picker is opening).
    pub fn refresh_outputs(&self) {
        let _ = self.refresh.send(OUTPUTS);
    }

    pub fn set_output(&self, id: u32) {
        self.command(OUTPUTS, move || audio::set_output(id));
    }

    pub fn toggle_output_mute(&self) {
        self.command(AUDIO, audio::toggle_output_mute);
    }

    pub fn toggle_microphone(&self) {
        self.command(MICROPHONE, audio::toggle_microphone_mute);
    }

    /// Calls `PlayPause`, `Next` or `Previous` on a player.
    pub fn media(&self, bus_name: &str, method: &'static str) {
        let bus_name = bus_name.to_owned();
        self.command(PLAYERS, move || mpris::call(&bus_name, method));
    }

    pub fn seek(&self, player: &NowPlaying, fraction: f64) {
        let player = player.clone();
        self.command(PLAYERS, move || mpris::seek(&player, fraction));
    }

    fn command(&self, parts: Parts, run: impl FnOnce() + Send + 'static) {
        let refresh = self.refresh.clone();
        thread::spawn(move || {
            run();
            let _ = refresh.send(parts);
        });
    }
}

/// Applies only the newest queued volume, with a throttled feedback tick.
fn volume_worker(volumes: &Receiver<u8>, refresh: &Sender<Parts>) {
    let mut last_tick: Option<Instant> = None;
    while let Ok(mut percent) = volumes.recv() {
        while let Ok(newer) = volumes.try_recv() {
            percent = newer;
        }
        audio::set_volume(percent);
        if last_tick.is_none_or(|at| at.elapsed() >= TICK_INTERVAL) {
            last_tick = Some(Instant::now());
            audio::tick();
        }
        let _ = refresh.send(AUDIO);
    }
}

fn poll_forever(shared: &Mutex<(u64, Snapshot)>, requests: &Receiver<Parts>, fast: &AtomicBool) {
    let mut preferred: Option<String> = None;
    let mut snapshot = Snapshot::default();
    let start = Instant::now();
    let (mut next_players, mut next_audio, mut next_microphone, mut next_screen) =
        (start, start, start, start);
    let mut next_outputs = start;
    loop {
        let now = Instant::now();
        if now >= next_players {
            snapshot.now_playing = mpris::query(&mut preferred);
            next_players = now + MPRIS_FALLBACK;
        }
        if now >= next_audio {
            snapshot.audio = audio::output();
            next_audio = now
                + if fast.load(Ordering::Relaxed) {
                    AUDIO_FAST_INTERVAL
                } else {
                    AUDIO_INTERVAL
                };
        }
        if now >= next_microphone {
            snapshot.microphone = audio::microphone();
            next_microphone = now + MICROPHONE_INTERVAL;
        }
        if now >= next_outputs {
            snapshot.outputs = audio::outputs();
            next_outputs = now + OUTPUTS_INTERVAL;
        }
        if now >= next_screen {
            snapshot.screen_off = screen::off();
            next_screen = now + SCREEN_FALLBACK;
        }
        if let Ok(mut latest) = shared.lock()
            && !latest.1.looks_like(&snapshot)
        {
            latest.0 = latest.0.wrapping_add(1);
            latest.1 = snapshot.clone();
        }

        let deadline = next_players
            .min(next_audio)
            .min(next_microphone)
            .min(next_screen)
            .min(next_outputs);
        match requests.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            Ok(first) => {
                thread::sleep(DEBOUNCE);
                let mut parts = first;
                while let Ok(more) = requests.try_recv() {
                    parts.players |= more.players;
                    parts.audio |= more.audio;
                    parts.microphone |= more.microphone;
                    parts.screen |= more.screen;
                    parts.outputs |= more.outputs;
                }
                let now = Instant::now();
                if parts.players {
                    next_players = now;
                }
                if parts.audio {
                    next_audio = now;
                }
                if parts.microphone {
                    next_microphone = now;
                }
                if parts.screen {
                    next_screen = now;
                }
                if parts.outputs {
                    next_outputs = now;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
