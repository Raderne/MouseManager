//! No Bluetooth support on this OS yet (Linux arrives with BlueZ in B3's Linux half, after Phase 9).

use super::{AdapterState, Paired, PairedMouse};
use crate::event::Event;
use std::sync::mpsc::Sender;

#[derive(Clone)]
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

    pub fn pair(&self, _address: &str) -> Result<Paired, String> {
        Err("Bluetooth pairing isn't supported on this system yet".into())
    }

    pub fn forget(&self, _address: &str) -> Result<(), String> {
        Err("not supported on this system yet".into())
    }

    pub fn paired(&self) -> Vec<PairedMouse> {
        Vec::new()
    }

    pub fn watch(&self, _addresses: &[String]) {}
}
