//! Whether the desktop shows anything: the GNOME lock shield
//! (`org.gnome.ScreenSaver`) and monitor power saving (Mutter's
//! `PowerSaveMode`). Changes arrive as D-Bus signals.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

/// True while the screen is locked or the monitors are powered down.
#[must_use]
pub fn off() -> bool {
    locked() || power_saving()
}

fn locked() -> bool {
    busctl(&[
        "call",
        "org.gnome.ScreenSaver",
        "/org/gnome/ScreenSaver",
        "org.gnome.ScreenSaver",
        "GetActive",
    ])
    .is_some_and(|reply| reply.trim() == "b true")
}

fn power_saving() -> bool {
    busctl(&[
        "get-property",
        "org.gnome.Mutter.DisplayConfig",
        "/org/gnome/Mutter/DisplayConfig",
        "org.gnome.Mutter.DisplayConfig",
        "PowerSaveMode",
    ])
    .and_then(|reply| reply.trim().strip_prefix("i ")?.parse::<i32>().ok())
    .is_some_and(|mode| mode != 0)
}

fn busctl(args: &[&str]) -> Option<String> {
    let output = Command::new("busctl")
        .arg("--user")
        .args(args)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Runs `on_change` for every lock or display-power signal. Returns when
/// the monitor process ends.
pub fn watch(mut on_change: impl FnMut()) {
    let Ok(mut child) = Command::new("busctl")
        .args([
            "--user",
            "monitor",
            "--json=short",
            "--match",
            "type='signal',interface='org.gnome.ScreenSaver',member='ActiveChanged'",
            "--match",
            "type='signal',path='/org/gnome/Mutter/DisplayConfig',\
             interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",
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
