//! The probe contract. Every read of OS or network state goes through it.
//!
//! ARCHITECTURE.md §6.1. The trait deliberately exposes a single
//! `declaration()` rather than separate `id()`/`posture()`/`requires()`/
//! `reads()` accessors: one declaration means an implementation cannot report
//! an id that disagrees with the reads the registry audits. Drift is made
//! structurally impossible rather than merely discouraged.

use std::future::Future;
use std::pin::Pin;

use jrx_core::declaration::ProbeDeclaration;
use jrx_core::signal::Signal;

/// Everything a probe is given. Deliberately narrow: a probe cannot reach for
/// anything it was not handed.
#[derive(Debug, Clone, Default)]
pub struct ProbeCtx {
    /// Interface to operate on, where the probe is interface-scoped.
    pub interface: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// A required permission is not granted. Degrades one Visibility Panel
    /// row; never blocks the UI (ARCHITECTURE.md §6.6).
    #[error("permission denied: {0}")]
    PermissionDenied(&'static str),

    #[error("not supported on this platform")]
    Unsupported,

    #[error("probe timed out")]
    Timeout,

    #[error("probe failed: {0}")]
    Failed(String),
    /// The operating system rejected the request before it reached the
    /// network. Kept separate from `Failed` because it identifies a
    /// permission problem rather than a fault.
    #[error("{0}")]
    Refused(String),
}

pub type ProbeFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Signal>, ProbeError>> + Send + 'a>>;

/// A single source of observations.
pub trait Probe: Send + Sync {
    /// This probe's entry in the registry. Audited by the privacy invariants.
    fn declaration(&self) -> &'static ProbeDeclaration;

    fn run<'a>(&'a self, ctx: &'a ProbeCtx) -> ProbeFuture<'a>;
}
