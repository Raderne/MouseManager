//! The Bluetooth connection layer: adapter state, searching (B3), pairing and forgetting (B4).
//! Separate from `platform/`: once paired, the OS HID stack owns the mouse and `platform/` remaps it as usual.
//! Each OS module has the same functions, picked with `#[cfg]`:
//!
//! ```text
//! pub fn start(events: Sender<Event>) -> BtHandle;
//! impl BtHandle {
//!     pub fn adapter(&self) -> AdapterState;
//!     pub fn scan(&self, on: bool) -> Result<(), String>;   // emits btScan / btFound, stops by itself after SCAN_SECS
//!     pub fn open_settings(&self) -> Result<(), String>;    // the OS Bluetooth page; we never switch the radio ourselves
//!     pub fn pair(&self, address: &str) -> Result<Paired, String>;   // B4: blocks up to ~40 s; call off the UI thread
//!     pub fn forget(&self, address: &str) -> Result<(), String>;     // B4: unpair in the OS
//!     pub fn paired(&self) -> Vec<PairedMouse>;                      // B4: mice the OS has paired
//! }
//! ```
//!
//! Pairing, forgetting and trusting only ever start from a click in the app, never by themselves.
//! What a found device *is* (mouse? N230?) is decided here in Rust, never in JS.

use crate::event::LinkState;
use serde::Serialize;
use std::collections::HashMap;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{start, BtHandle};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{start, BtHandle};

/// A search stops by itself after this long: scanning costs radio time and laptop battery.
pub const SCAN_SECS: u64 = 30;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AdapterState {
    On,
    Off,
    Missing,
}

/// GAP appearance "HID Mouse".
const APPEARANCE_MOUSE: u16 = 0x03C2;
/// The 16-bit UUID of the HID service (HID over GATT).
const UUID_HID: u16 = 0x1812;
/// Advertising data type "Appearance".
pub const AD_APPEARANCE: u8 = 0x19;

/// One received advertisement or scan response, already pulled out of the OS types.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Advert {
    pub address: u64,
    pub name: Option<String>,
    pub rssi: i16,
    pub appearance: Option<u16>,
    pub uuids16: Vec<u16>,
}

/// What the UI gets for one device (`btFound` event).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Found {
    pub address: String,
    /// Empty until the scan response with the name has arrived.
    pub name: String,
    pub rssi: i16,
    /// 0–4 signal bars.
    pub bars: u8,
    pub mouse: bool,
    pub n230: bool,
}

/// The appearance value from the payload of an `AD_APPEARANCE` data section (little-endian).
pub fn appearance(data: &[u8]) -> Option<u16> {
    (data.len() == 2).then(|| u16::from_le_bytes([data[0], data[1]]))
}

/// The result of a successful pairing (B4).
#[derive(Clone, Debug, PartialEq)]
pub struct Paired {
    pub address: String,
    pub name: String,
    /// From the PnP ID characteristic; `None` if it couldn't be read.
    pub id: Option<crate::device::DeviceId>,
}

/// A mouse paired with this PC, for the "Paired" list and Forget.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PairedMouse {
    pub address: String,
    pub name: String,
    pub n230: bool,
    pub connected: bool,
}

/// The link state of one paired mouse. The OS can't tell "asleep", "switched off" and "on another
/// channel" apart: all are "paired, not connected". A mouse seen connected this session reads as
/// `Sleeping` (the UI says "asleep or off: move it to wake it"); one never seen connected as `Disconnected`.
pub fn link_state(paired: bool, connected: bool, was_connected: bool) -> LinkState {
    if connected {
        LinkState::Connected
    } else if paired && was_connected {
        LinkState::Sleeping
    } else {
        LinkState::Disconnected
    }
}

/// Link state per watched address; reports only changes.
#[derive(Default)]
pub struct Links {
    seen: HashMap<u64, (bool, LinkState)>, // (was connected this session, last state sent)
}

impl Links {
    pub fn update(&mut self, address: u64, paired: bool, connected: bool) -> Option<LinkState> {
        let (was, last) = self.seen.entry(address).or_insert((false, LinkState::Searching)); // never sent: first update is news
        *was |= connected;
        let next = link_state(paired, connected, *was);
        (next != *last).then(|| {
            *last = next;
            next
        })
    }

    /// Drop addresses no longer watched (forgotten / removed from the config).
    pub fn keep(&mut self, addresses: &[u64]) {
        self.seen.retain(|a, _| addresses.contains(a));
    }
}

pub fn is_n230(name: &str) -> bool {
    name.to_ascii_uppercase().contains("N230")
}

/// `AA:BB:CC:DD:EE:FF` (or with `-`, any case) → the 48-bit address. Anything else is `None`.
pub fn parse_address(s: &str) -> Option<u64> {
    let parts: Vec<&str> = s.split([':', '-']).collect();
    if parts.len() != 6 || parts.iter().any(|p| p.len() != 2) {
        return None;
    }
    parts.iter().try_fold(0u64, |acc, p| Some(acc << 8 | u8::from_str_radix(p, 16).ok()? as u64))
}

/// `AA:BB:CC:DD:EE:FF` from the 48-bit address.
pub fn format_address(a: u64) -> String {
    (0..6).rev().map(|i| format!("{:02X}", (a >> (8 * i)) & 0xFF)).collect::<Vec<_>>().join(":")
}

pub fn bars(rssi: i16) -> u8 {
    match rssi {
        r if r >= -60 => 4,
        r if r >= -70 => 3,
        r if r >= -80 => 2,
        r if r >= -90 => 1,
        _ => 0,
    }
}

/// Everything seen during one search. Adverts for one device arrive in pieces (the name only in the
/// scan response), so what we learn is kept and merged.
#[derive(Default)]
pub struct Scan {
    seen: HashMap<u64, (Advert, Found)>,
}

impl Scan {
    /// Merges an advert. Returns the device when the UI should hear about it: first sighting,
    /// a new name or kind, or a different number of signal bars. RSSI jitter alone is not news.
    pub fn update(&mut self, adv: &Advert) -> Option<Found> {
        let (known, last) = self.seen.entry(adv.address).or_insert_with(|| (Advert { address: adv.address, ..Default::default() }, Found {
            address: format_address(adv.address),
            name: String::new(),
            rssi: 0,
            bars: u8::MAX, // never equal to a real value, so the first sighting is always news
            mouse: false,
            n230: false,
        }));
        if let Some(name) = adv.name.as_ref().filter(|n| !n.is_empty()) {
            known.name = Some(name.clone());
        }
        if adv.appearance.is_some() {
            known.appearance = adv.appearance;
        }
        for u in &adv.uuids16 {
            if !known.uuids16.contains(u) {
                known.uuids16.push(*u);
            }
        }
        let name = known.name.clone().unwrap_or_default();
        let next = Found {
            address: last.address.clone(),
            n230: is_n230(&name),
            name,
            rssi: adv.rssi,
            bars: bars(adv.rssi),
            mouse: match known.appearance {
                Some(a) => a == APPEARANCE_MOUSE,
                None => known.uuids16.contains(&UUID_HID), // HID, kind not said yet: show it rather than hide a mouse
            },
        };
        let news = next.name != last.name || next.mouse != last.mouse || next.bars != last.bars;
        *last = next;
        news.then(|| last.clone())
    }

    pub fn clear(&mut self) {
        self.seen.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The N230 (BT1 slot) as seen in B0: first the advert, then the scan response with the name.
    fn n230_advert(rssi: i16) -> Advert {
        Advert { address: 0xFE46_E924_0491, name: None, rssi, appearance: Some(0x03C2), uuids16: vec![0x1812] }
    }
    fn n230_scan_response(rssi: i16) -> Advert {
        Advert { address: 0xFE46_E924_0491, name: Some("Lenovo Services N230 BT1".into()), rssi, ..Default::default() }
    }

    #[test]
    fn helpers() {
        assert_eq!(format_address(0xFE46_E924_0491), "FE:46:E9:24:04:91");
        assert_eq!(format_address(0x0000_0000_0001), "00:00:00:00:00:01");
        assert_eq!(appearance(&[0xC2, 0x03]), Some(0x03C2), "the bytes B0 saw (C203)");
        assert_eq!(appearance(&[0xC2]), None);
        assert_eq!([bars(-50), bars(-65), bars(-75), bars(-85), bars(-95)], [4, 3, 2, 1, 0]);
        assert_eq!(parse_address("FE:46:E9:24:04:91"), Some(0xFE46_E924_0491));
        assert_eq!(parse_address("fe-46-e9-24-04-91"), Some(0xFE46_E924_0491));
        assert_eq!(parse_address(&format_address(0x0012_3456_789A)), Some(0x0012_3456_789A), "round trip");
        for bad in ["", "FE:46:E9:24:04", "FE:46:E9:24:04:91:00", "FE:46:E9:24:04:9", "FE:46:E9:24:04:GG", "FE46E9240491"] {
            assert_eq!(parse_address(bad), None, "{bad}");
        }
    }

    #[test]
    fn link_states() {
        use LinkState::*;
        let mut l = Links::default();
        let a = 0xFE46_E924_0491;
        assert_eq!(l.update(a, true, true), Some(Connected), "first report is always news");
        assert_eq!(l.update(a, true, true), None, "no change, no event");
        assert_eq!(l.update(a, true, false), Some(Sleeping), "paired, was connected: asleep or off");
        assert_eq!(l.update(a, true, true), Some(Connected), "woke up");
        assert_eq!(l.update(a, false, false), Some(Disconnected), "forgotten in Windows");

        let b = 0x0000_0000_0001;
        assert_eq!(l.update(b, true, false), Some(Disconnected), "paired but never seen connected this session");
        assert_eq!(l.update(b, true, true), Some(Connected));
        assert_eq!(l.update(b, true, false), Some(Sleeping));

        l.keep(&[a]);
        assert_eq!(l.update(b, true, false), Some(Disconnected), "unwatched address starts over");
    }

    #[test]
    fn the_n230_is_found_and_named() {
        let mut s = Scan::default();
        let first = s.update(&n230_advert(-71)).expect("first sighting is news");
        assert!(first.mouse && !first.n230 && first.name.is_empty(), "a mouse, name not known yet");
        let named = s.update(&n230_scan_response(-68)).expect("the name is news");
        assert!(named.mouse && named.n230, "the scan response keeps what the advert said");
        assert_eq!(named.name, "Lenovo Services N230 BT1");
        assert_eq!(named.address, "FE:46:E9:24:04:91");
    }

    #[test]
    fn rssi_jitter_is_not_news() {
        let mut s = Scan::default();
        s.update(&n230_advert(-61));
        assert_eq!(s.update(&n230_advert(-64)), None, "same 3 bars");
        assert_eq!(s.update(&n230_advert(-72)).map(|f| f.bars), Some(2));
        s.clear();
        assert!(s.update(&n230_advert(-72)).is_some(), "a new search starts fresh");
    }

    #[test]
    fn what_counts_as_a_mouse() {
        let mut s = Scan::default();
        let kb = Advert { address: 1, appearance: Some(0x03C1), uuids16: vec![0x1812], name: Some("Keyboard".into()), rssi: -50 };
        assert!(!s.update(&kb).unwrap().mouse, "a HID keyboard is not a mouse");
        let phone = Advert { address: 2, rssi: -50, ..Default::default() };
        assert!(!s.update(&phone).unwrap().mouse);
        let hid_unknown = Advert { address: 3, uuids16: vec![0x1812], rssi: -50, ..Default::default() };
        assert!(s.update(&hid_unknown).unwrap().mouse, "HID with no appearance yet is shown");
        let other_mouse = Advert { address: 4, appearance: Some(0x03C2), name: Some("MX Anywhere".into()), rssi: -50, ..Default::default() };
        let f = s.update(&other_mouse).unwrap();
        assert!(f.mouse && !f.n230, "another mouse is a mouse but not the N230");
        let lower = Advert { address: 5, name: Some("lenovo n230 bt2".into()), rssi: -50, ..Default::default() };
        assert!(s.update(&lower).unwrap().n230, "name match ignores case");
    }
}
