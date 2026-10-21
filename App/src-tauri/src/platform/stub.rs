//! Placeholder backend: validates mappings, touches no input.

use crate::{action, config::Config, event::Event};
use std::sync::mpsc::Sender;

pub struct Handle {
    _events: Sender<Event>,
}

pub fn start(_config: Config, events: Sender<Event>) -> Handle {
    Handle { _events: events }
}

impl Handle {
    pub fn apply(&self, config: Config) -> Result<(), String> {
        action::parse_buttons(&config.buttons).map(|_| ())
    }

    pub fn set_scale(&self, _factor: f64) {}

    pub fn set_all_devices(&self, _on: bool) {}
}
