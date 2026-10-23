//! `muster daemon`: the daemon run in this process, its log, and the
//! signals it answers.

use std::process::ExitCode;

use muster_core::Paths;
use muster_daemon::DaemonConfig;

use crate::fail;

pub(crate) async fn daemon(paths: Paths, background: bool, adopt: Option<i32>) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MUSTER_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    // A panic anywhere, in a task or a thread, is logged with the rest, its
    // time and thread named (`ARC-13`).
    std::panic::set_hook(Box::new(|info| {
        let thread = std::thread::current();
        let backtrace = std::backtrace::Backtrace::capture();
        tracing::error!(
            thread = thread.name().unwrap_or("unnamed"),
            "panicked: {info}\n{backtrace}"
        );
    }));
    let mut config = DaemonConfig::new(paths);
    // Tests measure more often than people need.
    let millis = |key| std::env::var(key).ok()?.parse::<u64>().ok();
    if let Some(ms) = millis("MUSTER_SAMPLE_MS") {
        config.sample_interval = std::time::Duration::from_millis(ms.max(10));
    }

    if let Some(ms) = millis("MUSTER_BRANCH_MS") {
        config.branch_interval = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_WORKTREE_MS") {
        config.worktree_interval = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_START_GRACE_MS") {
        config.start_grace = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_PR_MS") {
        config.pr_interval = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_SPEND_MS") {
        config.spend_interval = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_LOG_CHECK_MS") {
        config.log_limits.every = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(bytes) = millis("MUSTER_LOG_MAX_BYTES") {
        config.log_limits.max_bytes = bytes.max(1);
    }
    // Listening from the start, so no signal arrives before it is caught:
    // uncaught, SIGUSR1 would end the daemon.
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: one stop per signal the daemon is sent"
    )]
    let (stops, stop_queue) = tokio::sync::mpsc::unbounded_channel();
    match Signals::listen() {
        Ok(signals) => {
            #[expect(
                clippy::disallowed_methods,
                reason = "ARC-13: listens for signals as long as the daemon runs"
            )]
            tokio::spawn(signals.forward(background, stops));
        }
        Err(error) => return fail(format!("cannot listen for signals: {error}")),
    }
    // Run again, as it is, by a restart in place.
    let mut relaunch = vec!["daemon".to_owned()];
    if background {
        relaunch.push("--background".to_owned());
    }
    match muster_daemon::run(config, stop_queue, relaunch, adopt).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// The signals the daemon answers, each caught by a handler, never ignored
/// outright: an ignored signal would be inherited by the agents the daemon
/// starts, which are stopped with a hang-up.
struct Signals {
    interrupt: tokio::signal::unix::Signal,
    terminate: tokio::signal::unix::Signal,
    hangup: tokio::signal::unix::Signal,
    reload: tokio::signal::unix::Signal,
}

impl Signals {
    fn listen() -> std::io::Result<Self> {
        use tokio::signal::unix::{SignalKind, signal};
        Ok(Self {
            interrupt: signal(SignalKind::interrupt())?,
            terminate: signal(SignalKind::terminate())?,
            hangup: signal(SignalKind::hangup())?,
            reload: signal(SignalKind::user_defined1())?,
        })
    }

    /// Tells the daemon what each signal asks, saying which in the log, so
    /// a stop is never a mystery: SIGINT and SIGTERM stop it, SIGUSR1 has it
    /// restart in place (`muster reload`). A hang-up stops a daemon run in a
    /// terminal, which has closed; one in the `background` has no terminal,
    /// so a hang-up there is someone else's and is logged and ignored.
    async fn forward(
        mut self,
        background: bool,
        stops: tokio::sync::mpsc::UnboundedSender<muster_daemon::Stop>,
    ) {
        use muster_daemon::Stop;
        loop {
            let stop = tokio::select! {
                _ = self.interrupt.recv() => {
                    tracing::info!(signal = "SIGINT", "stopping: interrupted");
                    Stop::Shutdown
                }
                _ = self.terminate.recv() => {
                    tracing::info!(signal = "SIGTERM", "stopping: asked to stop");
                    Stop::Shutdown
                }
                _ = self.hangup.recv() => {
                    if background {
                        tracing::info!(signal = "SIGHUP", "ignored a hang-up: the daemon has no terminal");
                        continue;
                    }
                    tracing::info!(signal = "SIGHUP", "stopping: its terminal closed");
                    Stop::Shutdown
                }
                _ = self.reload.recv() => {
                    tracing::info!(signal = "SIGUSR1", "asked to restart in place");
                    Stop::Reload
                }
            };
            if stops.send(stop).is_err() {
                return;
            }
        }
    }
}
