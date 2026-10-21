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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Device {
    pub vid: String,
    pub pid: String,
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
            devices: vec![Device { vid: "3554".into(), pid: "FC01".into() }], // the 2.4G receiver
            buttons: Buttons::default(),
            theme: THEMES[0].0.into(),
            start_minimized: true,
            low_battery_warning: true,
        }
    }
}

impl Config {
    /// A missing or unreadable file gives the defaults; missing keys get their defaults.
    pub fn load(path: &Path) -> Self {
        let mut c: Self = fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if !is_theme(&c.theme) {
            c.theme = Self::default().theme;
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
    fn config_round_trip_and_defaults() {
        let path = std::env::temp_dir().join(format!("n230-config-test-{}.json", std::process::id()));
        let c = Config {
            devices: vec![Device { vid: "3554".into(), pid: "FC01".into() }, Device { vid: "17EF".into(), pid: "6060".into() }],
            buttons: Buttons { x1: "Back".into(), x2: "Ctrl+Shift+T".into() },
            theme: "ember".into(),
            start_minimized: false,
            low_battery_warning: false,
        };
        c.save(&path).unwrap();
        assert_eq!(Config::load(&path), c);

        // The file format from STEPS.md.
        let json: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(json["buttons"]["X2"], "Ctrl+Shift+T");
        assert_eq!(json["devices"][0]["vid"], "3554");
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
