//! Bluetooth devices for the picker: listed from BlueZ over `busctl --json`,
//! connected, disconnected and paired with `bluetoothctl`. Discovery runs in
//! a `bluetoothctl scan on` child, since BlueZ stops it when its caller exits.

use std::process::{Child, Command, Stdio};

use serde_json::Value;

/// Upper bound on one `bluetoothctl` command (pairing can wait on the device).
const COMMAND_TIMEOUT: &str = "30";
/// A scan child never outlives this many seconds, even if never stopped.
const SCAN_TIMEOUT: &str = "120";

/// One device for the picker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    rssi: Option<i64>,
}

/// Paired devices (connected first), then discovered ones by signal; devices
/// that never sent a name are left out.
#[must_use]
pub fn devices() -> Vec<Device> {
    let Ok(output) = Command::new("busctl")
        .args([
            "--json=short",
            "call",
            "org.bluez",
            "/",
            "org.freedesktop.DBus.ObjectManager",
            "GetManagedObjects",
        ])
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    serde_json::from_slice::<Value>(&output.stdout)
        .map(|reply| parse_devices(&reply))
        .unwrap_or_default()
}

fn parse_devices(reply: &Value) -> Vec<Device> {
    let Some(objects) = reply.pointer("/data/0").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut devices: Vec<Device> = objects
        .values()
        .filter_map(|interfaces| {
            let device = interfaces.get("org.bluez.Device1")?;
            let field = |name: &str| device.pointer(&format!("/{name}/data"));
            let flag = |name: &str| field(name).and_then(Value::as_bool).unwrap_or(false);
            let paired = flag("Paired");
            // Unnamed devices only have an address for a name; paired ones
            // keep whatever alias the user gave them.
            let name = if paired { field("Alias") } else { field("Name") }?
                .as_str()?
                .trim()
                .to_owned();
            Some(Device {
                address: field("Address")?.as_str()?.to_owned(),
                name,
                paired,
                connected: flag("Connected"),
                rssi: field("RSSI").and_then(Value::as_i64),
            })
        })
        .filter(|device| !device.name.is_empty())
        .collect();
    devices.sort_by(|a, b| {
        b.paired
            .cmp(&a.paired)
            .then(b.connected.cmp(&a.connected))
            .then(b.rssi.cmp(&a.rssi))
            .then_with(|| a.name.cmp(&b.name))
    });
    devices
}

/// Connects a paired device, disconnects a connected one, pairs (and trusts,
/// so it reconnects on its own) and connects a new one.
pub fn activate(device: &Device) {
    let address = device.address.as_str();
    if device.connected {
        bluetoothctl(&["disconnect", address]);
    } else if device.paired {
        bluetoothctl(&["connect", address]);
    } else if bluetoothctl(&["--agent", "NoInputNoOutput", "pair", address]) {
        bluetoothctl(&["trust", address]);
        bluetoothctl(&["connect", address]);
    }
}

fn bluetoothctl(args: &[&str]) -> bool {
    Command::new("bluetoothctl")
        .args(["--timeout", COMMAND_TIMEOUT])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Discovery for as long as the value lives.
pub struct Scan(Child);

impl Scan {
    #[must_use]
    pub fn start() -> Option<Self> {
        Command::new("bluetoothctl")
            .args(["--timeout", SCAN_TIMEOUT, "scan", "on"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()
            .map(Self)
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn device(address: &str, name: Option<&str>, paired: bool, connected: bool, rssi: Option<i64>) -> Value {
        let mut properties = json!({
            "Address": {"type": "s", "data": address},
            "Alias": {"type": "s", "data": name.unwrap_or(address)},
            "Paired": {"type": "b", "data": paired},
            "Connected": {"type": "b", "data": connected},
        });
        if let Some(name) = name {
            properties["Name"] = json!({"type": "s", "data": name});
        }
        if let Some(rssi) = rssi {
            properties["RSSI"] = json!({"type": "n", "data": rssi});
        }
        json!({"org.bluez.Device1": properties})
    }

    #[test]
    fn paired_come_first_then_discovered_by_signal() {
        let reply = json!({"type": "a{oa{sa{sv}}}", "data": [{
            "/org/bluez/hci0": {"org.bluez.Adapter1": {}},
            "/a": device("A", Some("Mouse"), true, false, None),
            "/b": device("B", Some("Buds"), true, true, None),
            "/c": device("C", Some("Far"), false, false, Some(-90)),
            "/d": device("D", None, false, false, Some(-40)),
            "/e": device("E", Some("Near"), false, false, Some(-50)),
        }]});
        let names: Vec<_> = parse_devices(&reply).into_iter().map(|device| device.name).collect();
        assert_eq!(names, ["Buds", "Mouse", "Near", "Far"]);
    }
}
