//! The boundary between JRX and whatever the platform happens to offer.

use std::time::Duration;

use jrx_core::activity::{CounterSample, SocketObservation};

/// Why an activity source could not answer.
///
/// Kept separate from a generic error because the product treats these
/// differently: a missing tool is permanent for this run, a timeout is not,
/// and unreadable output means the format moved under us.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// The tool is not on this system.
    #[error("{0} is not available on this Mac")]
    Unavailable(String),
    /// It ran but produced nothing usable.
    #[error("could not read the output of {0}")]
    Unreadable(String),
    /// It did not finish in time.
    #[error("{0} did not respond within {1:?}")]
    TimedOut(String, Duration),
    /// It ran and failed.
    #[error("{0} failed: {1}")]
    Failed(String, String),
}

impl ProviderError {
    /// Whether retrying on the next tick could plausibly succeed.
    ///
    /// A missing tool will still be missing; a slow one may not be.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            ProviderError::TimedOut(..) | ProviderError::Failed(..)
        )
    }

    /// Wording for a person. The technical detail belongs in a diagnostic
    /// view, not in the headline.
    pub fn user_facing(&self) -> &'static str {
        match self {
            ProviderError::Unavailable(_) => {
                "This Mac does not provide the tool JRX uses to see per-program activity."
            }
            ProviderError::Unreadable(_) => {
                "JRX could not understand what macOS reported about program activity."
            }
            ProviderError::TimedOut(..) => "macOS did not answer in time about program activity.",
            ProviderError::Failed(..) => {
                "macOS reported an error when JRX asked about program activity."
            }
        }
    }
}

/// How much data an interface has carried.
///
/// The cheaper and more reliable of the two sources. It must keep working when
/// the per-program source does not.
pub trait InterfaceActivityProvider: Send + Sync {
    fn counters(&self, interface: &str) -> Result<CounterSample, ProviderError>;
}

/// Which sockets exist, who owns them, and how much they have carried.
pub trait ProcessConnectionProvider: Send + Sync {
    fn observe(&self) -> Result<Vec<SocketObservation>, ProviderError>;

    /// Prepare the source ahead of first use.
    ///
    /// The macOS implementation costs seconds on its first call after boot.
    /// Warming happens off the critical path so the first frame is not spent
    /// waiting for it.
    fn warm(&self) {}

    /// A name for diagnostics.
    fn describe(&self) -> &'static str;
}

/// Both halves together.
pub struct ActivityProvider {
    pub interface: Box<dyn InterfaceActivityProvider>,
    pub connections: Box<dyn ProcessConnectionProvider>,
}
