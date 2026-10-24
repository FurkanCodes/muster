//! Waiting in a test on what happens apart from it, the daemon's work off
//! its loop most often, never by a guess at how long that takes (ARC-30):
//! `until` waits for what must happen, up to [`WAIT`]; `holds` watches for
//! a stated time that what must not happen does not; `passes` lets a time
//! go by where the time is what is tested (a program's own timer). Only
//! these sleep.

#![expect(
    clippy::disallowed_methods,
    reason = "ARC-30: the one place tests sleep, each wait looking again and again, or for a stated time"
)]

use std::time::{Duration, Instant};

/// The longest anything a test waits for may take.
pub(crate) const WAIT: Duration = Duration::from_secs(20);

/// How often a wait looks again.
const LOOK_EVERY: Duration = Duration::from_millis(20);

/// Waits until `done`, failing the test, which names `what`, after [`WAIT`].
pub(crate) fn until(what: &str, mut done: impl FnMut() -> bool) {
    found(what, || done().then_some(()));
}

/// The first of what `look` finds, looking again until it finds one,
/// failing the test, which names `what`, after [`WAIT`].
pub(crate) fn found<T>(what: &str, look: impl FnMut() -> Option<T>) -> T {
    within(WAIT, look).unwrap_or_else(|| panic!("waited {WAIT:?} for {what}"))
}

/// The first of what `look` finds within `long`, looking again until it
/// finds one; `None` where it finds nothing, for a caller that says more.
pub(crate) fn within<T>(long: Duration, mut look: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + long;
    loop {
        if let Some(found) = look() {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(LOOK_EVERY);
    }
}

/// [`until`], for a test that looks through the daemon's own connection.
pub(crate) async fn until_async(what: &str, mut done: impl AsyncFnMut() -> bool) {
    found_async(what, async || done().await.then_some(())).await;
}

/// [`found`], for a test that looks through the daemon's own connection.
pub(crate) async fn found_async<T>(what: &str, look: impl AsyncFnMut() -> Option<T>) -> T {
    within_async(WAIT, look)
        .await
        .unwrap_or_else(|| panic!("waited {WAIT:?} for {what}"))
}

/// [`within`], for a test that looks through the daemon's own connection.
pub(crate) async fn within_async<T>(
    long: Duration,
    mut look: impl AsyncFnMut() -> Option<T>,
) -> Option<T> {
    let deadline = Instant::now() + long;
    loop {
        if let Some(found) = look().await {
            return Some(found);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(LOOK_EVERY).await;
    }
}

/// Watches for `long` that `still` stays so, failing the test, which names
/// `what`, the moment it does not: what must not happen does not, within
/// a time it would have.
pub(crate) fn holds(what: &str, long: Duration, mut still: impl FnMut() -> bool) {
    let until = Instant::now() + long;
    while Instant::now() < until {
        assert!(still(), "{what} no longer held");
        std::thread::sleep(LOOK_EVERY);
    }
    assert!(still(), "{what} no longer held");
}

/// [`holds`], for a test that looks through the daemon's own connection.
pub(crate) async fn holds_async(what: &str, long: Duration, mut still: impl AsyncFnMut() -> bool) {
    let until = Instant::now() + long;
    while Instant::now() < until {
        assert!(still().await, "{what} no longer held");
        tokio::time::sleep(LOOK_EVERY).await;
    }
    assert!(still().await, "{what} no longer held");
}

/// Lets `long` go by, where the time is what is tested: `why` says whose
/// timer it waits out.
pub(crate) fn passes(long: Duration, why: &str) {
    let _ = why;
    std::thread::sleep(long);
}

/// [`passes`], in a test on the runtime.
pub(crate) async fn passes_async(long: Duration, why: &str) {
    let _ = why;
    tokio::time::sleep(long).await;
}
