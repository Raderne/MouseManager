//! No Bluetooth support on this OS yet (Linux arrives with BlueZ in B3's Linux half, after Phase 9).

use super::AdapterState;
use crate::event::Event;
use std::sync::mpsc::Sender;

pub struct BtHandle;

pub fn start(_events: Sender<Event>) -> BtHandle {
    BtHandle
}

impl BtHandle {
    pub fn adapter(&self) -> AdapterState {
        AdapterState::Missing
    }

    pub fn scan(&self, _on: bool) -> Result<(), String> {
        Err("Bluetooth search isn't supported on this system yet".into())
    }

    pub fn open_settings(&self) -> Result<(), String> {
        Err("not supported on this system yet".into())
    }
}
