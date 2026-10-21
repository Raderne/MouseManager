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
//! Phase 4 adds `windows.rs` and Phase 9 `linux.rs`; until then every OS uses the stub.

mod stub;
pub use stub::{start, Handle};
