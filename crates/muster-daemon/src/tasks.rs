//! Who owns the daemon's tasks (`ARC-13`).
//!
//! Short-lived work, such as a command's call to a ticket source or a run's
//! next step, runs under [`Tasks`], which counts it and gives it a moment
//! to finish when the daemon stops. Clients' connections are counted there
//! too. A task that lives as long as something else is held by that
//! something as an [`Owned`] handle, and ends when the handle is dropped.

use std::future::Future;
use std::time::Duration;

use tokio_util::task::{AbortOnDropHandle, TaskTracker};

/// How long work under way has to finish when the daemon stops, before the
/// runtime ends it. Every effect is idempotent and stored before it runs
/// (`ARC-10`), so ending it is safe; the wait spares a reply or a write
/// that was nearly done.
pub(crate) const STOP_GRACE: Duration = Duration::from_secs(2);

/// A task that runs until its handle is dropped.
pub(crate) type Owned<T = ()> = AbortOnDropHandle<T>;

/// Starts a task that runs until the returned handle is dropped.
pub(crate) fn owned<T: Send + 'static>(task: impl Future<Output = T> + Send + 'static) -> Owned<T> {
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-13: the handle returned owns it"
    )]
    AbortOnDropHandle::new(tokio::spawn(task))
}

/// The daemon's short-lived work and its clients' connections.
#[derive(Clone, Debug, Default)]
pub(crate) struct Tasks {
    work: TaskTracker,
    connections: TaskTracker,
}

/// How many tasks are alive, for `muster status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Counts {
    /// Short-lived work under way.
    pub(crate) work: usize,
    /// Clients connected.
    pub(crate) connections: usize,
    /// Every task on the runtime, these included.
    pub(crate) all: usize,
}

impl Tasks {
    /// Starts finite work. Every wait on something outside the daemon in
    /// it has a deadline, so it ends on its own.
    pub(crate) fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.work.spawn(work);
    }

    /// Serves a client until it leaves or the daemon stops.
    pub(crate) fn connection(&self, connection: impl Future<Output = ()> + Send + 'static) {
        self.connections.spawn(connection);
    }

    pub(crate) fn counts(&self) -> Counts {
        Counts {
            work: self.work.len(),
            connections: self.connections.len(),
            all: tokio::runtime::Handle::current()
                .metrics()
                .num_alive_tasks(),
        }
    }

    /// Takes no more work and waits up to `grace` for what is under way to
    /// finish; what is left ends with the runtime. Returns how many tasks
    /// were left.
    pub(crate) async fn finish(&self, grace: Duration) -> usize {
        self.work.close();
        let _ = tokio::time::timeout(grace, self.work.wait()).await;
        self.work.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn work_is_counted_until_it_ends() {
        let tasks = Tasks::default();
        let (done, finished) = tokio::sync::oneshot::channel::<()>();
        tasks.spawn(async move {
            let _ = finished.await;
        });
        assert_eq!(tasks.counts().work, 1);
        let _ = done.send(());
        assert_eq!(tasks.finish(Duration::from_secs(5)).await, 0);
        assert_eq!(tasks.counts().work, 0);
    }

    #[tokio::test(start_paused = true)]
    async fn stopping_waits_no_longer_than_its_grace() {
        let tasks = Tasks::default();
        tasks.spawn(std::future::pending());
        let started = tokio::time::Instant::now();
        assert_eq!(tasks.finish(Duration::from_secs(2)).await, 1);
        assert_eq!(started.elapsed(), Duration::from_secs(2));
    }

    #[tokio::test]
    async fn connections_are_counted_apart_from_work() {
        let tasks = Tasks::default();
        tasks.connection(std::future::pending());
        let counts = tasks.counts();
        assert_eq!((counts.work, counts.connections), (0, 1));
        assert!(counts.all >= 1);
        // A connection is no work to wait for.
        assert_eq!(tasks.finish(Duration::from_secs(5)).await, 0);
    }

    #[tokio::test]
    async fn an_owned_task_ends_when_its_handle_is_dropped() {
        let (_alive, gone) = tokio::sync::oneshot::channel::<()>();
        let (dropped, told) = tokio::sync::oneshot::channel::<()>();
        let task = owned(async move {
            // Tells when the future is dropped, as an aborted task's is.
            let _guard = Signal(Some(dropped));
            let _ = gone.await;
        });
        tokio::task::yield_now().await;
        drop(task);
        assert!(told.await.is_ok());
    }

    struct Signal(Option<tokio::sync::oneshot::Sender<()>>);

    impl Drop for Signal {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }
}
