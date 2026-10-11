//! The Muster daemon: owns every session and run, and is the only process that
//! talks to agents, Docker, GitHub and ticket sources (`ARCHITECTURE.md` §4).

pub mod clock;
pub mod store;

mod git;
mod harness;
mod pty;
mod server;
mod supervisor;
mod terminal;

#[cfg(test)]
mod tests;

use std::fs::{DirBuilder, File, Permissions, TryLockError};
use std::future::Future;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use muster_core::Paths;
use tokio::net::UnixListener;

use crate::clock::{Clock, SystemClock};
use crate::store::{Store, StoreError, StoreHandle};

/// How to run a daemon.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub paths: Paths,
    /// How many deltas a slow subscriber may fall behind before it is sent a
    /// fresh snapshot instead.
    pub delta_capacity: usize,
}

impl DaemonConfig {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            delta_capacity: 1024,
        }
    }
}

/// Why the daemon could not start or keep running.
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("{context} {}: {source}", .path.display())]
    Io {
        context: &'static str,
        path: PathBuf,
        source: io::Error,
    },
    #[error("a daemon is already running for {}", .0.display())]
    AlreadyRunning(PathBuf),
    #[error(transparent)]
    Store(#[from] StoreError),
}

fn io_error(context: &'static str, path: &Path) -> impl FnOnce(io::Error) -> DaemonError {
    let path = path.to_owned();
    move |source| DaemonError::Io {
        context,
        path,
        source,
    }
}

/// Runs the daemon until `shutdown` completes, then removes its socket.
pub async fn run(
    config: DaemonConfig,
    shutdown: impl Future<Output = ()>,
) -> Result<(), DaemonError> {
    let paths = &config.paths;
    private_dir(&paths.runtime_dir)?;
    DirBuilder::new()
        .recursive(true)
        .create(&paths.data_dir)
        .map_err(io_error("cannot create", &paths.data_dir))?;

    // Held for the daemon's whole life; the operating system releases it
    // however the process ends.
    let _lock = lock(&paths.lock_file())?;

    let socket = paths.socket();
    // Holding the lock means any socket left here belongs to a daemon that died.
    match std::fs::remove_file(&socket) {
        Ok(()) => tracing::info!("removed a stale socket"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(io_error("cannot remove", &socket)(error)),
    }
    let listener = UnixListener::bind(&socket).map_err(io_error("cannot listen on", &socket))?;
    std::fs::set_permissions(&socket, Permissions::from_mode(0o600))
        .map_err(io_error("cannot restrict", &socket))?;

    let store = Store::open(&paths.database())?;
    let store = StoreHandle::spawn(store)
        .map_err(io_error("cannot start the store for", &paths.database()))?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let supervisor = supervisor::spawn(supervisor::Settings {
        store,
        clock,
        worktrees_dir: paths.worktrees_dir.clone(),
        delta_capacity: config.delta_capacity,
    })
    .await?;

    tracing::info!(socket = %socket.display(), pid = std::process::id(), "daemon ready");
    server::serve(listener, supervisor, shutdown).await;

    let _ = std::fs::remove_file(&socket);
    tracing::info!("daemon stopped");
    Ok(())
}

/// Creates `dir` if needed and makes it private to this user: the socket's
/// only protection is that nobody else can reach it (`ARCHITECTURE.md` §6).
fn private_dir(dir: &Path) -> Result<(), DaemonError> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(io_error("cannot create", dir))?;
    std::fs::set_permissions(dir, Permissions::from_mode(0o700))
        .map_err(io_error("cannot restrict", dir))
}

fn lock(path: &Path) -> Result<File, DaemonError> {
    let file = File::create(path).map_err(io_error("cannot create", path))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(DaemonError::AlreadyRunning(path.to_owned())),
        Err(TryLockError::Error(error)) => Err(io_error("cannot lock", path)(error)),
    }
}
