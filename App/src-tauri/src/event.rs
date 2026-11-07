//! Events from the platform backend to the UI. Sent on one Tauri channel as `{ "type": …, … }`.

use crate::bt::{AdapterState, Found};
use crate::device::Bus;
use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Event {
    /// `target`: the click came from a configured device. `button`: Left, Right, Middle, X1 or X2.
    Button { device: String, target: bool, button: String, down: bool },
    /// Movement summed over 50 ms.
    Move { target: bool, dx: i32, dy: i32 },
    /// `None` (null in JSON) while unknown, asleep or disconnected: the UI shows "—".
    Battery { percent: Option<u8> },
    /// The raw settings feature report, as hex.
    Settings { hex: String },
    /// `bus`: which link this is about; `None` when it isn't known.
    Device { bus: Option<Bus>, connected: bool, permission: bool },
    /// Bluetooth connection state for the connect flow (phases B3–B5). `address` is `None` until known.
    Link { bus: Bus, state: LinkState, address: Option<String> },
    Error { message: String },
    /// Bluetooth adapter state, sent when a search starts (`on` | `off` | `missing`).
    BtAdapter { state: AdapterState },
    /// A Bluetooth search started or ended (stopped, timed out, or the radio went away).
    BtScan { active: bool },
    /// A device seen during a search: new, renamed, or a different number of signal bars.
    BtFound(Found),
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LinkState {
    Searching,
    Pairing,
    Connected,
    /// Paired, but the mouse dropped the link to save power; a click wakes it.
    Sleeping,
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_shape() {
        let b = Event::Button { device: "N230".into(), target: true, button: "X2".into(), down: true };
        assert_eq!(
            serde_json::to_value(b).unwrap(),
            json!({ "type": "button", "device": "N230", "target": true, "button": "X2", "down": true })
        );
        assert_eq!(
            serde_json::to_value(Event::Battery { percent: None }).unwrap(),
            json!({ "type": "battery", "percent": null })
        );
        assert_eq!(
            serde_json::to_value(Event::Device { bus: Some(Bus::Bluetooth), connected: true, permission: true }).unwrap(),
            json!({ "type": "device", "bus": "bluetooth", "connected": true, "permission": true })
        );
        assert_eq!(
            serde_json::to_value(Event::Link { bus: Bus::Bluetooth, state: LinkState::Sleeping, address: None }).unwrap(),
            json!({ "type": "link", "bus": "bluetooth", "state": "sleeping", "address": null })
        );
        let found = Found { address: "FE:46:E9:24:04:91".into(), name: "Lenovo Services N230 BT1".into(), rssi: -68, bars: 3, mouse: true, n230: true };
        assert_eq!(
            serde_json::to_value(Event::BtFound(found)).unwrap(),
            json!({ "type": "btFound", "address": "FE:46:E9:24:04:91", "name": "Lenovo Services N230 BT1", "rssi": -68, "bars": 3, "mouse": true, "n230": true })
        );
        assert_eq!(serde_json::to_value(Event::BtAdapter { state: AdapterState::Off }).unwrap(), json!({ "type": "btAdapter", "state": "off" }));
        assert_eq!(serde_json::to_value(Event::BtScan { active: false }).unwrap(), json!({ "type": "btScan", "active": false }));
    }
}
