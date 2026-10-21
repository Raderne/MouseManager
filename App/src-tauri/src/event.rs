//! Events from the platform backend to the UI. Sent on one Tauri channel as `{ "type": …, … }`.

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
    Device { connected: bool, permission: bool },
    Error { message: String },
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
    }
}
