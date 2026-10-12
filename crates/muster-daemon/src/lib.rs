//! The Muster daemon: owns every session and run, and is the only process that
//! talks to agents, Docker, GitHub and ticket sources (`ARCHITECTURE.md` §4).

pub mod clock;
pub mod config;
pub mod store;

mod branches;
mod containers;
mod git;
mod githost;
mod github;
mod glab;
mod harness;
mod hooks;
mod pty;
mod resources;
mod runs;
mod secrets;
mod server;
mod service;
mod supervisor;
mod terminal;
mod trackers;
mod worktree_sync;

#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tests;

use std::fs::{DirBuilder, File, Permissions, TryLockError};
use std::future::Future;
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muster_core::Paths;
use tokio::net::UnixListener;

pub use crate::runs::credentials::{KEYCHAIN_ACCOUNT, KEYCHAIN_SERVICE};

/// Keeps `value` in the Keychain for `account`, as `muster auth` does:
/// handed to `security` on its standard input, never a command line.
pub async fn keep_secret(account: &str, value: &str) -> std::io::Result<()> {
    use crate::secrets::Secrets as _;
    secrets::Keychain
        .set(account, &secrets::Token::new(value))
        .await
}

/// The Keychain account for Bitbucket's token for `host`.
pub fn bitbucket_account(host: &str) -> String {
    githost::bitbucket::account(host)
}

use crate::clock::{Clock, SystemClock};
use crate::store::{Store, StoreError, StoreHandle};

/// How to run a daemon.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub paths: Paths,
    /// How many deltas a slow subscriber may fall behind before it is sent a
    /// fresh snapshot instead.
    pub delta_capacity: usize,
    /// How often sessions' resource use is measured.
    pub sample_interval: Duration,
    /// How often a measurement is kept in the store's history.
    pub history_interval: Duration,
    /// How often workspaces' checked-out branches are looked at.
    pub branch_interval: Duration,
    /// How often projects are looked at for worktrees made or removed
    /// outside Muster.
    pub worktree_interval: Duration,
    /// How long a harness that reports its start may take to before it is
    /// taken to be asking the person something first.
    pub start_grace: Duration,
    /// How often runs' pull requests are looked at (`ARCHITECTURE.md` §10:
    /// GitHub has no way to tell a local daemon, so it asks).
    pub pr_interval: Duration,
}

impl DaemonConfig {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            delta_capacity: 1024,
            sample_interval: Duration::from_secs(2),
            history_interval: Duration::from_secs(30),
            branch_interval: Duration::from_secs(2),
            worktree_interval: Duration::from_secs(3),
            start_grace: muster_state::session::Timing::default().start_grace,
            pr_interval: Duration::from_secs(60),
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
    let hook_socket = paths.hook_socket();
    // Holding the lock means any socket left here belongs to a daemon that died.
    for stale in [&socket, &hook_socket] {
        match std::fs::remove_file(stale) {
            Ok(()) => tracing::info!(socket = %stale.display(), "removed a stale socket"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("cannot remove", stale)(error)),
        }
    }
    let listener = listen(&socket)?;
    let hook_listener = listen(&hook_socket)?;

    let store = Store::open(&paths.database())?;
    let store = StoreHandle::spawn(store)
        .map_err(io_error("cannot start the store for", &paths.database()))?;
    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let (hook_deliveries, hook_queue) = tokio::sync::mpsc::channel(1024);
    let credentials: Arc<dyn runs::credentials::Credentials> =
        Arc::new(runs::credentials::PersonsCredentials);
    // Runs, set aside, open their pull requests on GitHub.
    let git_host: Arc<dyn githost::GitHost> = Arc::new(githost::GitHub);
    let supervisor = supervisor::spawn(supervisor::Settings {
        store,
        clock,
        worktrees_dir: paths.worktrees_dir.clone(),
        hook_socket: hook_socket.clone(),
        hook_queue,
        delta_capacity: config.delta_capacity,
        history_every: history_every(config.sample_interval, config.history_interval),
        timing: muster_state::session::Timing {
            start_grace: config.start_grace,
            ..muster_state::session::Timing::default()
        },
        work: Arc::new(run_work(paths, credentials.clone(), git_host.clone())),
        trackers: Arc::new(trackers::choose::ServiceTrackers),
        config_dir: paths.config_dir.clone(),
        credentials,
        docker: "docker".to_owned(),
        hosts: Arc::new(githost::choose::ServiceHosts {
            secrets: Arc::new(secrets::Keychain),
            http: service::http_client(),
        }),
    })
    .await?;
    let sampler = tokio::spawn(resources::run(supervisor.clone(), config.sample_interval));
    let branch_watcher = tokio::spawn(branches::run(supervisor.clone(), config.branch_interval));
    let worktree_sync = tokio::spawn(worktree_sync::run(
        supervisor.clone(),
        config.worktree_interval,
    ));
    let pr_watch = tokio::spawn(watch_pull_requests(supervisor.clone(), config.pr_interval));

    let (stop_hooks, hooks_stopped) = tokio::sync::oneshot::channel::<()>();
    let hook_server = tokio::spawn(hooks::serve(hook_listener, hook_deliveries, async {
        let _ = hooks_stopped.await;
    }));

    tracing::info!(socket = %socket.display(), pid = std::process::id(), "daemon ready");
    server::serve(listener, supervisor, shutdown).await;

    sampler.abort();
    branch_watcher.abort();
    worktree_sync.abort();
    pr_watch.abort();
    let _ = stop_hooks.send(());
    let _ = hook_server.await;
    let _ = std::fs::remove_file(&hook_socket);
    let _ = std::fs::remove_file(&socket);
    tracing::info!("daemon stopped");
    Ok(())
}

/// Runs' work with Docker, git and GitHub; `MUSTER_RUN_IMAGE` names an
/// image to use instead of building one.
fn run_work(
    paths: &Paths,
    credentials: Arc<dyn runs::credentials::Credentials>,
    git_host: Arc<dyn githost::GitHost>,
) -> runs::work::DockerWork {
    runs::work::DockerWork::new(
        paths.data_dir.clone(),
        git_host,
        credentials,
        Duration::from_millis(500),
        std::env::var("MUSTER_RUN_IMAGE")
            .ok()
            .filter(|image| !image.trim().is_empty()),
    )
}

/// Asks every `every` for runs' pull requests to be looked at, until the
/// Supervisor stops.
async fn watch_pull_requests(supervisor: supervisor::SupervisorHandle, every: Duration) {
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // The first tick is at once; the first look waits a whole interval.
    ticks.tick().await;
    loop {
        ticks.tick().await;
        if !supervisor.check_pull_requests().await {
            return;
        }
    }
}

/// How many measurements pass between those kept in history.
fn history_every(sample: Duration, history: Duration) -> u32 {
    let ratio = history.as_millis() / sample.as_millis().max(1);
    u32::try_from(ratio).unwrap_or(u32::MAX).max(1)
}

/// Binds a Unix socket only this user can use.
fn listen(socket: &Path) -> Result<UnixListener, DaemonError> {
    let listener = UnixListener::bind(socket).map_err(io_error("cannot listen on", socket))?;
    std::fs::set_permissions(socket, Permissions::from_mode(0o600))
        .map_err(io_error("cannot restrict", socket))?;
    Ok(listener)
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
