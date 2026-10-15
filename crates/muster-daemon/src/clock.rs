//! Reading the clock, behind a trait so tests can fix it (`ARC-03`).

use std::fmt::Debug;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use muster_core::Timestamp;

/// Where the daemon gets the time: wall-clock for what is stored and shown,
/// monotonic for deadlines and how long something took.
pub trait Clock: Debug + Send + Sync + 'static {
    fn now(&self) -> Timestamp;

    /// Monotonic time.
    fn instant(&self) -> Instant {
        monotonic()
    }
}

/// Monotonic time, from Tokio's clock: the system's, unless a test has
/// paused it to move it on by hand. Code without a [`Clock`] reads it here.
pub(crate) fn monotonic() -> Instant {
    tokio::time::Instant::now().into_std()
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[tokio::test(start_paused = true)]
    async fn monotonic_time_moves_with_a_paused_test_clock() {
        let before = monotonic();
        tokio::time::advance(Duration::from_secs(3600)).await;
        assert_eq!(monotonic() - before, Duration::from_secs(3600));
        assert_eq!(SystemClock.instant() - before, Duration::from_secs(3600));
    }
}
