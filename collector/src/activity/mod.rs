//! Getting activity observations out of the operating system.
//!
//! The application depends on the traits here, never on any particular tool.
//! On macOS the implementation shells out to `nettop`, which is a tool and not
//! an API: its output format is not contractual, so everything that reads it
//! is isolated behind this boundary and every failure mode degrades rather
//! than propagates (ARCHITECTURE.md, TECH_DECISIONS.md ADR-018).

pub mod macos;
pub mod monitor;
pub mod nettop;
pub mod provider;

pub use provider::{
    ActivityProvider, InterfaceActivityProvider, ProcessConnectionProvider, ProviderError,
};
