//! macOS platform bindings. All `parse_*` functions are pure and fixture-
//! tested; `exec` is the thin, untestable shell that feeds them.

pub mod arp;
pub mod exec;
pub mod parse;
pub mod permissions;
