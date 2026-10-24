//! Waiting in a test on what happens apart from it, the Supervisor's work
//! off its loop most often, never by a guess at how long that takes
//! (ARC-30): `until` and `found` wait for what must happen, up to [`WAIT`];
//! `holds` watches for a stated time that what must not happen does not;
//! `passes` lets a time go by where the time is what is tested. Only these
//! sleep.

#![expect(
    clippy::disallowed_methods,
    reason = "ARC-30: the one place tests sleep, each wait looking again and again, or for a stated time"
)]

use std::time::{Duration, Instant};

/// The longest anything a test waits for may take.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// How often a wait looks again.
const LOOK_EVERY: Duration = Duration::from_millis(10);

/// Waits until `done`, failing the test, which names `what`, after [`WAIT`].
pub(crate) async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    found(what, async || done().await.then_some(())).await;
}

/// The first of what `look` finds, looking again until it finds one,
/// failing the test, which names `what`, after [`WAIT`].
pub(crate) async fn found<T>(what: &str, mut look: impl AsyncFnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + WAIT;
    loop {
        if let Some(found) = look().await {
            return found;
        }
        assert!(Instant::now() < deadline, "waited {WAIT:?} for {what}");
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// Watches for `long` that `still` stays so, failing the test, which names
/// `what`, the moment it does not: what must not happen does not, within a
/// time it would have.
pub(crate) async fn holds(what: &str, long: Duration, mut still: impl AsyncFnMut() -> bool) {
    let until = Instant::now() + long;
    while Instant::now() < until {
        assert!(still().await, "{what} no longer held");
        tokio::time::sleep(LOOK_EVERY).await;
    }
    assert!(still().await, "{what} no longer held");
}

/// Lets `long` go by, where the time is what is tested: `why` says what
/// takes that time.
pub(crate) async fn passes(long: Duration, why: &str) {
    let _ = why;
    tokio::time::sleep(long).await;
}
