//! jrx-collector — platform probes. The only OS-facing code in the system.
//!
//! No Tauri dependency, by design (ARCHITECTURE.md §4, §15).

pub mod probe;
pub mod registry;
