//! Desktop actions that run as the logged-in user: screen lock and the
//! GNOME "Do Not Disturb" switch. Commands are spawned on helper threads so
//! the render loop never waits on them.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};
use std::thread;

const NOTIFICATIONS_SCHEMA: &str = "org.gnome.desktop.notifications";
const SHOW_BANNERS: &str = "show-banners";

const UNKNOWN: u8 = 0;
const ENABLED: u8 = 1;
const DISABLED: u8 = 2;

/// Locks the user's graphical session. `loginctl lock-session` without an
/// id needs a caller inside the session, which a user service is not, so
/// the session is resolved explicitly.
pub fn lock_screen() {
    thread::spawn(|| {
        let Ok(output) = Command::new("loginctl")
            .args(["show-user", "--property=Display", "--value"])
            .arg(user_name())
            .stderr(Stdio::null())
            .output()
        else {
            return;
        };
        let session = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        if session.is_empty() {
            return;
        }
        let _ = Command::new("loginctl")
            .args(["lock-session", &session])
            .stderr(Stdio::null())
            .status();
    });
}

fn user_name() -> String {
    std::env::var("USER").unwrap_or_default()
}

/// Mirrors GNOME's Do Not Disturb state (`show-banners` false) and flips it.
#[derive(Clone)]
pub struct DoNotDisturb {
    state: Arc<AtomicU8>,
}

impl DoNotDisturb {
    /// Starts a `gsettings monitor` child that keeps the state current, so
    /// changes made from the GNOME panel show up on the bar too.
    #[must_use]
    pub fn start() -> Self {
        let state = Arc::new(AtomicU8::new(UNKNOWN));
        let watcher = Arc::clone(&state);
        thread::spawn(move || {
            if let Some(banners) = read_banners() {
                watcher.store(encode(!banners), Ordering::Relaxed);
            }
            let Ok(mut child) = Command::new("gsettings")
                .args(["monitor", NOTIFICATIONS_SCHEMA, SHOW_BANNERS])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
            else {
                return;
            };
            if let Some(stdout) = child.stdout.take() {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(banners) = parse_monitor_line(&line) {
                        watcher.store(encode(!banners), Ordering::Relaxed);
                    }
                }
            }
            let _ = child.wait();
        });
        Self { state }
    }

    /// `Some(true)` while notifications are silenced.
    #[must_use]
    pub fn active(&self) -> Option<bool> {
        match self.state.load(Ordering::Relaxed) {
            ENABLED => Some(true),
            DISABLED => Some(false),
            _ => None,
        }
    }

    /// Flips the setting; the monitor confirms the new state.
    pub fn toggle(&self) {
        let silence = !self.active().unwrap_or(false);
        // Show the new state at once; the monitor corrects it on failure.
        self.state.store(encode(silence), Ordering::Relaxed);
        thread::spawn(move || {
            let _ = Command::new("gsettings")
                .args([
                    "set",
                    NOTIFICATIONS_SCHEMA,
                    SHOW_BANNERS,
                    if silence { "false" } else { "true" },
                ])
                .stderr(Stdio::null())
                .status();
        });
    }
}

const fn encode(active: bool) -> u8 {
    if active { ENABLED } else { DISABLED }
}

fn read_banners() -> Option<bool> {
    let output = Command::new("gsettings")
        .args(["get", NOTIFICATIONS_SCHEMA, SHOW_BANNERS])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    parse_bool(String::from_utf8_lossy(&output.stdout).trim())
}

/// Parses `show-banners: false` as printed by `gsettings monitor`.
fn parse_monitor_line(line: &str) -> Option<bool> {
    let (key, value) = line.split_once(':')?;
    (key.trim() == SHOW_BANNERS).then(|| parse_bool(value.trim()))?
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitor_lines_are_parsed() {
        assert_eq!(parse_monitor_line("show-banners: false"), Some(false));
        assert_eq!(parse_monitor_line("show-banners: true"), Some(true));
        assert_eq!(parse_monitor_line("other-key: true"), None);
        assert_eq!(parse_monitor_line("garbage"), None);
    }
}
