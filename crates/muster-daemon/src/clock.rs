//! Reading the clock, behind a trait so tests can fix it (`ARC-03`).

use std::fmt::Debug;
use std::time::{SystemTime, UNIX_EPOCH};

use muster_core::Timestamp;

/// Where the daemon gets wall-clock time.
pub trait Clock: Debug + Send + Sync + 'static {
    fn now(&self) -> Timestamp;
}

/// The system clock.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let since_epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        Timestamp::from_millis(i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX))
    }
}
