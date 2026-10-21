//! The input backend. Each OS module has the same functions, picked with `#[cfg]` (no trait):
//!
//! ```text
//! pub fn start(config: Config, events: Sender<Event>) -> Handle;
//! impl Handle {
//!     pub fn apply(&self, config: Config) -> Result<(), String>;  // Err names the button, nothing changes
//!     pub fn set_scale(&self, factor: f64);                        // 1.0 = native DPI
//!     pub fn set_all_devices(&self, on: bool);                     // monitor: report other devices too
//! }
//! ```
//!
//! Phase 9 replaces the stub with `linux.rs`.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::{start, Handle};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{start, Handle};
