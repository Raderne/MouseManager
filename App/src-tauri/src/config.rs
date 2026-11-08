use crate::device::{Bus, DeviceId};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

/// Theme name and its `--bg`, used as the window color before the page paints.
/// The test below checks this list against `src/styles.css`, which owns the colors.
pub const THEMES: [(&str, &str); 5] = [
    ("pulse", "#0B0F0D"),
    ("ember", "#0A0A0C"),
    ("frost", "#0A0D11"),
    ("amber", "#0F0D09"),
    ("violet", "#0D0B12"),
];

pub fn is_theme(name: &str) -> bool {
    THEMES.iter().any(|(n, _)| *n == name)
}

pub fn theme_bg(name: &str) -> (u8, u8, u8) {
    let bg = THEMES.iter().find(|(n, _)| *n == name).unwrap_or(&THEMES[0]).1;
    hex_rgb(bg)
}

/// A device id (VID or PID) must be exactly 4 hex digits, case-insensitive.
/// Anything else must never match a real device path.
pub fn valid_id(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn hex_rgb(hex: &str) -> (u8, u8, u8) {
    let v = |i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
    (v(1), v(3), v(5))
}

/// `config.json` in Tauri's `app_config_dir()`. Only Rust reads and writes it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// Devices to remap, as USB VID/PID (hex), so one file works on Windows and Linux.
    pub devices: Vec<Device>,
    pub buttons: Buttons,
    pub theme: String,
    pub start_minimized: bool,
    pub low_battery_warning: bool,
    /// Last DPI measured on the Performance screen; `None` until measured.
    pub measured_dpi: Option<f64>,
    /// The chosen stage; `None` = Native (no scaling).
    pub target_dpi: Option<u32>,
}

/// A device to remap. Matching uses VID/PID and, if set, the bus; `address` and `name` never affect matching.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Device {
    pub vid: String,
    pub pid: String,
    /// `None` = any bus (configs from before Bluetooth support).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<Bus>,
    /// Bluetooth address, only for reconnecting or forgetting this exact mouse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// Display only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Device {
    pub fn new(vid: &str, pid: &str, bus: Option<Bus>) -> Self {
        Self { vid: vid.into(), pid: pid.into(), bus, ..Default::default() }
    }

    /// A bad VID/PID never matches anything (so a blank entry can't remap every mouse).
    pub fn matches(&self, id: &DeviceId) -> bool {
        let parse = |s: &str| valid_id(s).then(|| u16::from_str_radix(s, 16).ok()).flatten();
        parse(&self.vid) == Some(id.vid) && parse(&self.pid) == Some(id.pid) && self.bus.is_none_or(|b| b == id.bus)
    }
}

/// The N230's IDs: the 2.4G receiver, and the mouse itself over Bluetooth LE (both measured, see the wiki's N230 Hardware).
fn default_devices() -> Vec<Device> {
    vec![
        Device { name: Some("N230 receiver".into()), ..Device::new("3554", "FC01", Some(Bus::Usb)) },
        Device { name: Some("N230 Bluetooth".into()), ..Device::new("25A7", "FAA2", Some(Bus::Bluetooth)) },
    ]
}

/// Mapping text per side button, parsed by `action::parse_buttons`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Buttons {
    #[serde(rename = "X1")]
    pub x1: String,
    #[serde(rename = "X2")]
    pub x2: String,
}

impl Default for Buttons {
    fn default() -> Self {
        Self { x1: "Default".into(), x2: "Default".into() }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            devices: default_devices(),
            buttons: Buttons::default(),
            theme: THEMES[0].0.into(),
            start_minimized: true,
            low_battery_warning: true,
            measured_dpi: None,
            target_dpi: None,
        }
    }
}

impl Config {
    /// After pairing in the app: record the Bluetooth address and name on the entry for this VID/PID
    /// (adding one if there's none). `id` is `None` when the PnP ID couldn't be read: nothing to match, no change.
    /// Returns whether anything changed.
    ///
    /// The N230 has two Bluetooth channels with the same VID/PID, so an entry is reused only if it is this
    /// address, has no address, or holds an address Windows no longer has paired (`still_paired`; a re-pair
    /// of the same channel gets a new address). Otherwise the other channel's entry is kept and one is added.
    pub fn remember_bluetooth(&mut self, id: Option<DeviceId>, address: &str, name: &str, still_paired: &[String]) -> bool {
        let Some(id) = id else { return false };
        let bt = DeviceId { bus: Bus::Bluetooth, ..id };
        let name = Some(name.to_string()).filter(|n| !n.is_empty());
        let before = self.devices.clone();
        let same = |a: &str| a.eq_ignore_ascii_case(address);
        let stale = |a: &str| !still_paired.iter().any(|p| p.eq_ignore_ascii_case(a));
        let slot = self
            .devices
            .iter()
            .position(|d| d.matches(&bt) && d.address.as_deref().is_some_and(same))
            .or_else(|| self.devices.iter().position(|d| d.matches(&bt) && d.address.as_deref().is_none_or(stale)));
        match slot.map(|i| &mut self.devices[i]) {
            Some(d) => {
                d.bus = Some(Bus::Bluetooth);
                if !d.address.as_deref().is_some_and(same) {
                    d.address = Some(address.to_string()); // same address in another case: keep the stored one
                }
                d.name = name.or(d.name.take());
            }
            None => self.devices.push(Device {
                address: Some(address.to_string()),
                name,
                ..Device::new(&format!("{:04X}", bt.vid), &format!("{:04X}", bt.pid), Some(Bus::Bluetooth))
            }),
        }
        self.devices != before
    }

    /// After forgetting in the app: drop the address, keep the VID/PID entry so a later pairing is
    /// still remapped. Returns whether anything changed.
    pub fn forget_bluetooth(&mut self, address: &str) -> bool {
        let mut changed = false;
        for d in self.devices.iter_mut().filter(|d| d.address.as_deref().is_some_and(|a| a.eq_ignore_ascii_case(address))) {
            d.address = None;
            changed = true;
        }
        changed
    }

    /// Movement scale for the backend: target ÷ measured, 1.0 when either is unknown.
    /// Clamped so a hand-edited config can't make the pointer unusable.
    pub fn scale(&self) -> f64 {
        match (self.measured_dpi, self.target_dpi) {
            (Some(m), Some(t)) if m.is_finite() && m > 0.0 => (t as f64 / m).clamp(0.1, 10.0),
            _ => 1.0,
        }
    }

    /// A missing or unreadable file gives the defaults; missing keys get their defaults.
    pub fn load(path: &Path) -> Self {
        let mut c: Self = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if !is_theme(&c.theme) {
            c.theme = Self::default().theme;
        }
        // A list saved before Bluetooth support is exactly the old default (the receiver, no bus).
        // Upgrade it to the new default; a list the user edited since then is left alone.
        if c.devices == [Device::new("3554", "FC01", None)] {
            c.devices = default_devices();
        }
        c
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS: &str = include_str!("../../src/styles.css");
    const VARS: [&str; 8] = ["bg", "panel", "panel2", "line", "text", "muted", "accent", "on-accent"];

    /// The `--name: #hex` pairs inside `[data-theme="theme"] { … }`.
    fn theme_vars(theme: &str) -> Vec<(String, String)> {
        let start = CSS.find(&format!("[data-theme=\"{theme}\"]")).expect(theme);
        let body = &CSS[start..];
        let body = &body[body.find('{').unwrap() + 1..body.find('}').unwrap()];
        body.split(';')
            .filter_map(|d| d.trim().strip_prefix("--")?.split_once(':'))
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_lowercase()))
            .collect()
    }

    fn var(theme: &str, name: &str) -> String {
        theme_vars(theme).into_iter().find(|(k, _)| k == name).unwrap().1
    }

    fn contrast(a: &str, b: &str) -> f64 {
        let lum = |h: &str| {
            let (r, g, b) = hex_rgb(h);
            let c = |v: u8| {
                let v = v as f64 / 255.0;
                if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
            };
            0.2126 * c(r) + 0.7152 * c(g) + 0.0722 * c(b)
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn css_themes_match_rust_list() {
        assert_eq!(CSS.matches("[data-theme=\"").count(), THEMES.len());
        for (name, bg) in THEMES {
            let keys: Vec<_> = theme_vars(name).into_iter().map(|(k, _)| k).collect();
            assert_eq!(keys, VARS, "{name} must define exactly the 8 tokens, in order");
            assert_eq!(var(name, "bg"), bg.to_lowercase(), "{name}: Rust bg out of sync with CSS");
        }
    }

    #[test]
    fn readable_contrast_in_every_theme() {
        for (name, _) in THEMES {
            for (fg, bg) in [("muted", "panel"), ("muted", "panel2"), ("text", "panel"), ("on-accent", "accent")] {
                let r = contrast(&var(name, fg), &var(name, bg));
                assert!(r >= 4.5, "{name}: --{fg} on --{bg} is {r:.2}:1, needs 4.5:1");
            }
        }
    }

    #[test]
    fn device_id_validation() {
        assert!(valid_id("3554"));
        assert!(valid_id("fc01"));
        assert!(valid_id("FC01"));
        assert!(!valid_id("355"), "too short");
        assert!(!valid_id("35545"), "too long");
        assert!(!valid_id(""), "empty");
        assert!(!valid_id("35g4"), "non-hex");
    }

    #[test]
    fn device_matching() {
        let usb = DeviceId { bus: Bus::Usb, vid: 0x3554, pid: 0xFC01 };
        let bt = DeviceId { bus: Bus::Bluetooth, vid: 0x25A7, pid: 0xFAA2 };
        assert!(Device::new("3554", "fc01", None).matches(&usb), "case-insensitive");
        assert!(Device::new("25A7", "FAA2", None).matches(&bt), "no bus = any bus");
        assert!(Device::new("25A7", "FAA2", Some(Bus::Bluetooth)).matches(&bt));
        assert!(!Device::new("25A7", "FAA2", Some(Bus::Usb)).matches(&bt), "USB-only entry must not match the BT mouse");
        assert!(!Device::new("25A7", "FAA3", None).matches(&bt), "other PID");
        let addr = Device { address: Some("00:00:00:00:00:00".into()), ..Device::new("25A7", "FAA2", None) };
        assert!(addr.matches(&bt), "the address never affects matching");
        for bad in [Device::new("", "", None), Device::new("25A", "FAA2", None), Device::new("25A7x", "FAA2", None)] {
            assert!(!bad.matches(&bt), "{bad:?} must match nothing");
        }
        assert!(Config::default().devices.iter().any(|d| d.matches(&usb)) && Config::default().devices.iter().any(|d| d.matches(&bt)));
    }

    #[test]
    fn remember_and_forget_bluetooth() {
        let n230 = Some(DeviceId { bus: Bus::Bluetooth, vid: 0x25A7, pid: 0xFAA2 });
        let mut c = Config::default();
        assert!(c.remember_bluetooth(n230, "FE:46:E9:24:04:91", "Lenovo Services N230 BT1", &[]));
        assert_eq!(c.devices.len(), 2, "the default Bluetooth entry is updated, not duplicated");
        let bt = &c.devices[1];
        assert_eq!((bt.address.as_deref(), bt.name.as_deref()), (Some("FE:46:E9:24:04:91"), Some("Lenovo Services N230 BT1")));
        assert!(!c.remember_bluetooth(n230, "FE:46:E9:24:04:91", "Lenovo Services N230 BT1", &[]), "same again: no change");
        assert!(!c.remember_bluetooth(None, "FE:46:E9:24:04:91", "x", &[]), "no PnP ID: nothing to match");

        // An entry without a bus (old config) is claimed for Bluetooth; an empty name keeps the old one.
        let mut old = Config { devices: vec![Device { name: Some("mine".into()), ..Device::new("25a7", "faa2", None) }], ..Config::default() };
        assert!(old.remember_bluetooth(n230, "AA:BB:CC:DD:EE:FF", "", &[]));
        assert_eq!((old.devices[0].bus, old.devices[0].name.as_deref()), (Some(Bus::Bluetooth), Some("mine")));

        // A mouse not in the list gets a new entry; a USB-only entry for the same IDs isn't reused.
        let mut usb = Config { devices: vec![Device::new("25A7", "FAA2", Some(Bus::Usb))], ..Config::default() };
        assert!(usb.remember_bluetooth(n230, "AA:BB:CC:DD:EE:FF", "N230", &[]));
        assert_eq!(usb.devices.len(), 2);
        assert_eq!((usb.devices[1].vid.as_str(), usb.devices[1].pid.as_str()), ("25A7", "FAA2"));

        // Two Bluetooth channels (BT1 and BT2) on one PC: same VID/PID, both paired → two entries.
        let mut two = Config::default();
        let bt1 = "FE:46:E9:24:04:91".to_string();
        assert!(two.remember_bluetooth(n230, &bt1, "Lenovo Services N230 BT1", &[]));
        assert!(two.remember_bluetooth(n230, "DB:13:6A:63:29:D1", "Lenovo Services N230 BT2", &[bt1.clone()]));
        assert_eq!(two.devices.len(), 3, "BT1 still paired: BT2 gets its own entry");
        assert_eq!(two.devices[1].address.as_deref(), Some(bt1.as_str()), "BT1's entry untouched");
        assert!(!two.remember_bluetooth(n230, "db:13:6a:63:29:d1", "Lenovo Services N230 BT2", &[bt1.clone()]), "BT2 again: its own entry, no change");
        // Re-pairing BT1 gives it a new address; the old one is no longer paired, so its entry is reused.
        assert!(two.remember_bluetooth(n230, "CC:CD:72:DF:59:E5", "Lenovo Services N230 BT1", &["DB:13:6A:63:29:D1".to_string()]));
        assert_eq!(two.devices.len(), 3, "a stale address is replaced, not piled up");
        assert_eq!(two.devices[1].address.as_deref(), Some("CC:CD:72:DF:59:E5"));

        assert!(c.forget_bluetooth("fe:46:e9:24:04:91"), "address match ignores case");
        assert_eq!(c.devices[1].address, None);
        assert!(c.devices[1].matches(&n230.unwrap()), "the VID/PID entry stays, so pairing again still remaps");
        assert!(!c.forget_bluetooth("FE:46:E9:24:04:91"), "already forgotten");
    }

    #[test]
    fn old_device_list_upgrades() {
        let path = std::env::temp_dir().join(format!("n230-config-upgrade-{}.json", std::process::id()));
        fs::write(&path, r#"{"devices":[{"vid":"3554","pid":"FC01"}]}"#).unwrap();
        assert_eq!(Config::load(&path).devices, Config::default().devices, "the old default gains the Bluetooth entry");
        fs::write(&path, r#"{"devices":[{"vid":"3554","pid":"FC01","bus":"usb"}]}"#).unwrap();
        assert_eq!(Config::load(&path).devices.len(), 1, "a list the user trimmed stays trimmed");
        fs::write(&path, r#"{"devices":[{"vid":"3554","pid":"FC01"},{"vid":"17EF","pid":"6060"}]}"#).unwrap();
        assert_eq!(Config::load(&path).devices.len(), 2, "an edited list is left alone");
        fs::write(&path, r#"{"devices":[]}"#).unwrap();
        assert!(Config::load(&path).devices.is_empty(), "an emptied list stays empty");
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn dpi_scale() {
        let c = |m, t| Config { measured_dpi: m, target_dpi: t, ..Config::default() }.scale();
        assert!((c(Some(1424.0), Some(800)) - 0.5618).abs() < 1e-3, "800 stage ≈ 56%");
        assert_eq!(c(None, Some(800)), 1.0, "not measured yet");
        assert_eq!(c(Some(1424.0), None), 1.0, "Native");
        assert_eq!(c(Some(0.0), Some(800)), 1.0);
        assert_eq!(c(Some(10.0), Some(1600)), 10.0, "clamped");
    }

    #[test]
    fn config_round_trip_and_defaults() {
        let path = std::env::temp_dir().join(format!("n230-config-test-{}.json", std::process::id()));
        let c = Config {
            devices: vec![
                Device::new("3554", "FC01", None),
                Device { address: Some("FE:46:E9:24:04:91".into()), name: Some("N230 BT1".into()), ..Device::new("25A7", "FAA2", Some(Bus::Bluetooth)) },
            ],
            buttons: Buttons { x1: "Back".into(), x2: "Ctrl+Shift+T".into() },
            theme: "ember".into(),
            start_minimized: false,
            low_battery_warning: false,
            measured_dpi: Some(1424.0),
            target_dpi: Some(800),
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);

        // The file format from STEPS.md.
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["buttons"]["X2"], "Ctrl+Shift+T");
        assert_eq!(json["devices"][0]["vid"], "3554");
        assert_eq!(json["devices"][0].as_object().unwrap().len(), 2, "unset bus/address/name are left out of the file");
        assert_eq!(json["devices"][1]["bus"], "bluetooth");
        assert_eq!(json["devices"][1]["address"], "FE:46:E9:24:04:91");
        assert_eq!(json["startMinimized"], false);
        assert_eq!(json["lowBatteryWarning"], false);

        fs::write(&path, "{}").unwrap();
        assert_eq!(Config::load(&path), Config::default(), "missing keys get defaults");
        fs::write(&path, r#"{"buttons":{"X2":"VolumeUp"},"startMinimized":false}"#).unwrap();
        let partial = Config::load(&path);
        assert_eq!((partial.buttons.x1.as_str(), partial.buttons.x2.as_str()), ("Default", "VolumeUp"));
        assert!(!partial.start_minimized && partial.low_battery_warning);
        fs::write(&path, r#"{"theme":"nope"}"#).unwrap();
        assert_eq!(Config::load(&path).theme, "pulse", "unknown theme falls back");
        fs::write(&path, "not json").unwrap();
        assert_eq!(Config::load(&path), Config::default());
        fs::remove_file(&path).unwrap();
        assert_eq!(Config::load(&path), Config::default(), "missing file");
    }
}
