//! The Muster daemon: owns every session and run, and is the only process that
//! talks to agents, Docker, GitHub and ticket sources (`ARCHITECTURE.md` §4).

pub mod clock;
pub mod config;
pub mod store;

mod ask;
mod branches;
mod changes;
mod clashes;
mod containers;
mod git;
mod githost;
mod github;
mod glab;
mod handoff;
mod harness;
mod hooks;
mod log_files;
mod mcp;
mod oauth;
mod plugin_run;
mod plugins;
mod pty;
mod resources;
mod runs;
mod secrets;
mod server;
mod service;
mod spend;
mod supervisor;
mod tasks;
mod terminal;
mod tidy;
mod timeline;
mod trackers;
mod worktree_sync;

#[cfg(test)]
mod changes_tests;
#[cfg(test)]
mod clashes_supervisor_tests;
#[cfg(test)]
mod open_panel_tests;
#[cfg(test)]
mod panel_tests;
#[cfg(test)]
mod pulls_tests;
#[cfg(test)]
mod run_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod timeline_supervisor_tests;

use std::fs::{DirBuilder, File, Permissions, TryLockError};
use std::io;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use muster_core::Paths;
use tokio::net::UnixListener;

pub use crate::log_files::LogLimits;
pub use crate::plugin_run::{PluginRunError, plugin_run};
pub use crate::runs::credentials::{KEYCHAIN_ACCOUNT, KEYCHAIN_SERVICE};
pub use crate::service::ServiceError;

/// Keeps `value` in the Keychain for `account`, as `muster auth` does:
/// handed to `security` on its standard input, never a command line.
pub async fn keep_secret(account: &str, value: &str) -> std::io::Result<()> {
    use crate::secrets::Secrets as _;
    secrets::Keychain::default()
        .set(account, &secrets::Token::new(value))
        .await
}

/// The Keychain account for Bitbucket's token for `host`.
pub fn bitbucket_account(host: &str) -> String {
    githost::bitbucket::account(host)
}

/// A service Muster signs in to in the person's browser, through its MCP
/// server's OAuth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignIn {
    /// Atlassian, for Jira Cloud.
    Jira,
    Linear,
}

/// Signs the person in to `service` in their browser (`open` is given the
/// address to show them) and keeps the sign-in in the Keychain.
pub async fn sign_in(service: SignIn, open: impl FnOnce(&str)) -> Result<(), ServiceError> {
    let provider = match service {
        SignIn::Jira => oauth::Provider {
            name: "Atlassian",
            resource: trackers::jira::MCP_URL
                .split('?')
                .next()
                .unwrap_or(trackers::jira::MCP_URL)
                .to_owned(),
            account: trackers::jira::MCP_ACCOUNT,
            scopes: &[
                "read:me",
                "read:account",
                "offline_access",
                "read:jira:agent-interface",
                "write:jira:agent-interface",
                "search:jira:agent-interface",
            ],
        },
        SignIn::Linear => oauth::Provider {
            name: "Linear",
            resource: trackers::linear::MCP_URL.to_owned(),
            account: trackers::linear::MCP_ACCOUNT,
            scopes: &["read", "write"],
        },
    };
    oauth::sign_in(
        &service::http_client(),
        &secrets::Keychain::default(),
        &provider,
        open,
    )
    .await
}

/// A Jira site as written, read: its host, and whether it is Jira Cloud.
pub fn jira_site(written: &str) -> Option<(String, bool)> {
    trackers::jira::Site::parse(written).map(|site| {
        let cloud = site.is_cloud();
        (site.host, cloud)
    })
}

/// Keeps a Jira token for the site at `host`, with the email it belongs to
/// (Jira Cloud's) or without (a Data Center's).
pub async fn keep_jira_token(
    host: &str,
    email: Option<String>,
    token: String,
) -> std::io::Result<()> {
    let kept = trackers::jira::KeptToken { email, token };
    let text = serde_json::to_string(&kept).map_err(std::io::Error::other)?;
    keep_secret(&trackers::jira::token_account(host), &text).await
}

/// The Keychain account of a Linear API key.
pub const LINEAR_KEY_ACCOUNT: &str = trackers::linear::KEY_ACCOUNT;

/// The Keychain account of the plugin `id`'s secret `name`.
pub fn plugin_account(id: &str, name: &str) -> String {
    secrets::plugin_account(id, name)
}

use crate::clock::{Clock, SystemClock};
use crate::store::{Store, StoreError, StoreHandle};
use crate::tasks::Tasks;

/// How to run a daemon.
#[derive(Clone, Debug)]
pub struct DaemonConfig {
    pub paths: Paths,
    /// How many deltas a slow subscriber may fall behind before it is sent a
    /// fresh snapshot instead.
    pub delta_capacity: usize,
    /// How often sessions' resource use is measured.
    pub sample_interval: Duration,
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
    /// How often sessions' agents' records are read for what they used.
    pub spend_interval: Duration,
    /// When `daemon.log` moves on to a fresh file (`ARC-15`).
    pub log_limits: LogLimits,
}

impl DaemonConfig {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            delta_capacity: 1024,
            sample_interval: Duration::from_secs(2),
            branch_interval: Duration::from_secs(2),
            worktree_interval: Duration::from_secs(3),
            start_grace: muster_state::session::Timing::default().start_grace,
            pr_interval: Duration::from_secs(60),
            spend_interval: Duration::from_secs(5),
            log_limits: LogLimits::default(),
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

/// What stops the daemon, or has it restart in place as the program
/// installed now (`muster reload`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Shutdown,
    Reload,
}

/// How long plugins' hooks under way have to end as the daemon stops or
/// restarts in place, before they are cut.
const HOOK_GRACE: Duration = Duration::from_secs(2);

/// The handoff format this build takes over from (`--can-adopt`).
pub const HANDOFF_FORMAT: u32 = handoff::FORMAT;

/// Runs the daemon until told to stop, then removes its sockets.
///
/// Told to reload, it restarts in place as the program installed now
/// (`handoff`), running it with `relaunch`, its own arguments, and the
/// handoff; its sessions run on. If it cannot, it says why and carries on.
/// With `adopt`, it takes over from the daemon's previous version, whose
/// handoff is at that descriptor.
pub async fn run(
    config: DaemonConfig,
    mut stops: tokio::sync::mpsc::UnboundedReceiver<Stop>,
    relaunch: Vec<String>,
    adopt: Option<std::os::fd::RawFd>,
) -> Result<(), DaemonError> {
    let paths = &config.paths;
    private_dir(&paths.runtime_dir)?;
    DirBuilder::new()
        .recursive(true)
        .create(&paths.data_dir)
        .map_err(io_error("cannot create", &paths.data_dir))?;

    let socket = paths.socket();
    let hook_socket = paths.hook_socket();
    let Held {
        lock,
        socket: socket_listener,
        hook_socket: hook_listener,
        adopted,
    } = hold(paths, adopt)?;

    let clock: Arc<dyn Clock> = Arc::new(SystemClock);
    let store = Store::open(&paths.database(), clock.now())?;
    let store = StoreHandle::spawn(store)
        .map_err(io_error("cannot start the store for", &paths.database()))?;
    let (hook_deliveries, hook_queue) = tokio::sync::mpsc::channel(1024);
    let tasks = Tasks::default();
    // Muster's own ticket plugins are this program, run again.
    let plugins = trackers::plugin::Plugins::new(
        handoff::next_program().unwrap_or_else(|_| PathBuf::from("muster")),
    );
    // Plugins' hooks, which write to the plugins' logs.
    let hooks = plugins::hooks::Hooks::new(
        tasks.clone(),
        plugins.clone(),
        Arc::new(secrets::Keychain::default()),
    );
    let supervisor = start_supervisor(
        &config,
        Started {
            store,
            clock: clock.clone(),
            hook_queue,
            adopted,
            tasks: tasks.clone(),
            plugins: plugins.clone(),
            hooks: hooks.clone(),
        },
    )
    .await?;
    let watchers = start_watchers(&config, &supervisor, clock);

    let (stop_hooks, hooks_stopped) = tokio::sync::oneshot::channel::<()>();
    let hook_server = tasks::owned(hooks::serve(
        serving(&hook_listener, &hook_socket)?,
        hook_deliveries,
        async {
            let _ = hooks_stopped.await;
        },
    ));

    tracing::info!(socket = %socket.display(), pid = std::process::id(), "daemon ready");
    let mut listener = serving(&socket_listener, &socket)?;
    loop {
        let mut stop = None;
        server::serve(listener, supervisor.clone(), &tasks, async {
            stop = stops.recv().await;
        })
        .await;
        if stop != Some(Stop::Reload) {
            break;
        }
        // The program that follows starts its own; these would outlive the
        // exec unreaped. Hooks' runs under way have a grace first.
        hooks.cut(HOOK_GRACE).await;
        plugins.stop().await;
        let failed = match reload(
            paths,
            &supervisor,
            &lock,
            &socket_listener,
            &hook_listener,
            &relaunch,
        )
        .await
        {
            Ok(never) => match never {},
            Err(reason) => reason,
        };
        tracing::warn!(reason = %failed, "could not restart in place; carrying on as before");
        hooks.reopen();
        listener = serving(&socket_listener, &socket)?;
    }

    drop(watchers);
    let _ = stop_hooks.send(());
    let _ = hook_server.await;
    hooks.cut(HOOK_GRACE).await;
    let left = tasks.finish(tasks::STOP_GRACE).await;
    if left > 0 {
        tracing::info!(left, "work still under way when the daemon stopped");
    }
    plugins.stop().await;
    let _ = std::fs::remove_file(&hook_socket);
    let _ = std::fs::remove_file(&socket);
    drop(lock);
    tracing::info!("daemon stopped");
    Ok(())
}

/// What the daemon holds for its whole life, across a restart in place
/// too: the lock, which the operating system releases however the process
/// ends, and its sockets' listeners; with the sessions a previous version
/// handed over.
struct Held {
    lock: File,
    socket: std::os::unix::net::UnixListener,
    hook_socket: std::os::unix::net::UnixListener,
    adopted: Vec<handoff::HandedSession>,
}

/// Takes the lock and the sockets, or, with `adopt`, takes them over from
/// the daemon's previous version.
fn hold(paths: &Paths, adopt: Option<std::os::fd::RawFd>) -> Result<Held, DaemonError> {
    if let Some(state) = adopt {
        let inherited = handoff::inherit(state).map_err(io_error(
            "cannot take over the handoff in",
            &paths.runtime_dir,
        ))?;
        tracing::info!(
            from = %inherited.from,
            sessions = inherited.sessions.len(),
            "taking over from the daemon's previous version"
        );
        return Ok(Held {
            lock: inherited.lock,
            socket: inherited.socket,
            hook_socket: inherited.hook_socket,
            adopted: inherited.sessions,
        });
    }
    let (socket, hook_socket) = (paths.socket(), paths.hook_socket());
    let lock = lock(&paths.lock_file())?;
    // Holding the lock means any socket left here belongs to a daemon that
    // died.
    for stale in [&socket, &hook_socket] {
        match std::fs::remove_file(stale) {
            Ok(()) => tracing::info!(socket = %stale.display(), "removed a stale socket"),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(io_error("cannot remove", stale)(error)),
        }
    }
    Ok(Held {
        lock,
        socket: listen(&socket)?,
        hook_socket: listen(&hook_socket)?,
        adopted: Vec::new(),
    })
}

/// What the Supervisor starts with, made in `run`.
struct Started {
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    hook_queue: tokio::sync::mpsc::Receiver<hooks::HookDelivery>,
    adopted: Vec<handoff::HandedSession>,
    tasks: Tasks,
    plugins: trackers::plugin::Plugins,
    hooks: plugins::hooks::Hooks,
}

/// Starts the Supervisor with the real systems behind its traits (`ARC-03`).
async fn start_supervisor(
    config: &DaemonConfig,
    started: Started,
) -> Result<supervisor::SupervisorHandle, DaemonError> {
    let paths = &config.paths;
    let credentials: Arc<dyn runs::credentials::Credentials> =
        Arc::new(runs::credentials::PersonsCredentials);
    // Runs, set aside, open their pull requests on GitHub.
    let git_host: Arc<dyn githost::GitHost> = Arc::new(githost::GitHub);
    Ok(supervisor::spawn(supervisor::Settings {
        store: started.store,
        clock: started.clock,
        worktrees_dir: paths.worktrees_dir.clone(),
        data_dir: paths.data_dir.clone(),
        hook_socket: paths.hook_socket(),
        hook_queue: started.hook_queue,
        delta_capacity: config.delta_capacity,
        timing: muster_state::session::Timing {
            start_grace: config.start_grace,
            ..muster_state::session::Timing::default()
        },
        work: Arc::new(run_work(
            paths,
            credentials.clone(),
            git_host,
            started.tasks.clone(),
        )),
        trackers: Arc::new(trackers::choose::ServiceTrackers {
            secrets: Arc::new(secrets::Keychain::default()),
            plugins: started.plugins.clone(),
        }),
        config_dir: paths.config_dir.clone(),
        credentials,
        docker: "docker".to_owned(),
        hosts: Arc::new(githost::choose::ServiceHosts {
            secrets: Arc::new(secrets::Keychain::default()),
            http: service::http_client(),
        }),
        adopted: started.adopted,
        tasks: started.tasks,
        ticket_plugins: started.plugins,
        places: spend::Places::from_env(),
        secrets: Arc::new(secrets::Keychain::default()),
        hooks: started.hooks,
    })
    .await?)
}

/// The daemon's watchers, each running until the daemon stops and drops it
/// (`ARC-13`).
fn start_watchers(
    config: &DaemonConfig,
    supervisor: &supervisor::SupervisorHandle,
    clock: Arc<dyn Clock>,
) -> [tasks::Owned; 6] {
    [
        tasks::owned(resources::run(supervisor.clone(), config.sample_interval)),
        tasks::owned(branches::run(supervisor.clone(), config.branch_interval)),
        tasks::owned(worktree_sync::run(
            supervisor.clone(),
            config.worktree_interval,
        )),
        tasks::owned(watch_pull_requests(supervisor.clone(), config.pr_interval)),
        tasks::owned(spend::run(
            supervisor.clone(),
            config.spend_interval,
            spend::Places::from_env(),
        )),
        tasks::owned(log_files::run(
            config.paths.runtime_dir.join("daemon.log"),
            config.log_limits,
            clock,
        )),
    ]
}

/// Restarts in place as the program installed now, handing every running
/// session over; returns only if that could not be done, the sessions
/// taken back.
async fn reload(
    paths: &Paths,
    supervisor: &supervisor::SupervisorHandle,
    lock: &File,
    socket: &std::os::unix::net::UnixListener,
    hook_socket: &std::os::unix::net::UnixListener,
    relaunch: &[String],
) -> Result<std::convert::Infallible, handoff::ReloadError> {
    let program = handoff::next_program().map_err(handoff::ReloadError::NoProgram)?;
    // Before anything is handed over: an older Muster could not take it.
    handoff::can_take_over(&program).await?;
    let sessions = supervisor.hand_over().await;
    tracing::info!(
        program = %program.display(),
        sessions = sessions.len(),
        "restarting in place"
    );
    let (error, sessions) = handoff::exec_into(
        &program,
        relaunch,
        &paths.runtime_dir,
        env!("CARGO_PKG_VERSION"),
        lock,
        socket,
        hook_socket,
        sessions,
    );
    supervisor.adopt(sessions).await;
    Err(handoff::ReloadError::NotStarted { program, error })
}

/// A listener the servers can use, the original kept for a handoff.
fn serving(
    listener: &std::os::unix::net::UnixListener,
    path: &Path,
) -> Result<UnixListener, DaemonError> {
    listener
        .try_clone()
        .and_then(|clone| {
            clone.set_nonblocking(true)?;
            UnixListener::from_std(clone)
        })
        .map_err(io_error("cannot listen on", path))
}

/// Runs' work with Docker, git and GitHub; `MUSTER_RUN_IMAGE` names an
/// image to use instead of building one.
fn run_work(
    paths: &Paths,
    credentials: Arc<dyn runs::credentials::Credentials>,
    git_host: Arc<dyn githost::GitHost>,
    tasks: Tasks,
) -> runs::work::DockerWork {
    runs::work::DockerWork::new(
        paths.data_dir.clone(),
        git_host,
        credentials,
        Duration::from_millis(500),
        std::env::var("MUSTER_RUN_IMAGE")
            .ok()
            .filter(|image| !image.trim().is_empty()),
        tasks,
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

/// Binds a Unix socket only this user can use.
fn listen(socket: &Path) -> Result<std::os::unix::net::UnixListener, DaemonError> {
    let listener = std::os::unix::net::UnixListener::bind(socket)
        .map_err(io_error("cannot listen on", socket))?;
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
