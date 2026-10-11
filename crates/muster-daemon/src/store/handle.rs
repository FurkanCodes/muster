//! Runs the store on its own thread so async code never blocks on SQLite.

use std::panic::{AssertUnwindSafe, catch_unwind};

use tokio::sync::{mpsc, oneshot};

use super::Store;

type Job = Box<dyn FnOnce(&mut Store) + Send>;

/// The store thread has stopped, or a call panicked on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the store is not available")]
pub struct StoreClosed;

/// A cheap, cloneable way to run work on the store thread. The thread stops
/// once every handle is dropped.
#[derive(Clone, Debug)]
pub struct StoreHandle {
    jobs: mpsc::Sender<Job>,
}

impl StoreHandle {
    /// Moves `store` onto a new thread.
    pub fn spawn(store: Store) -> std::io::Result<Self> {
        let (jobs, mut queue) = mpsc::channel::<Job>(256);
        std::thread::Builder::new()
            .name("muster-store".to_owned())
            .spawn(move || {
                let mut store = store;
                while let Some(job) = queue.blocking_recv() {
                    // A panicking call loses only its own reply.
                    if catch_unwind(AssertUnwindSafe(|| job(&mut store))).is_err() {
                        tracing::error!("a store call panicked");
                    }
                }
            })?;
        Ok(Self { jobs })
    }

    /// Runs `work` on the store thread and returns its result.
    pub async fn call<T, F>(&self, work: F) -> Result<T, StoreClosed>
    where
        F: FnOnce(&mut Store) -> T + Send + 'static,
        T: Send + 'static,
    {
        let (reply, answer) = oneshot::channel();
        let job: Job = Box::new(move |store| {
            let _ = reply.send(work(store));
        });
        self.jobs.send(job).await.map_err(|_| StoreClosed)?;
        answer.await.map_err(|_| StoreClosed)
    }
}
