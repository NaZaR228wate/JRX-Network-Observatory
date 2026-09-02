//! jrx-core — domain model and rules for JRX Network Observatory.
//!
//! Pure logic. No OS access, no network, no Tauri. See ARCHITECTURE.md §4.

pub mod activity;
pub mod capability;
pub mod data_class;
pub mod declaration;
pub mod device;
pub mod history;
pub mod network;
pub mod signal;
pub mod topology;
