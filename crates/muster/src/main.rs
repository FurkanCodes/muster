//! The `muster` command. With no arguments it will open the terminal UI; its
//! subcommands run the daemon and drive it from scripts.

mod commands;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use muster_core::Paths;
use muster_daemon::DaemonConfig;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "A cockpit for coding agents, with an optional autopilot."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the daemon in this terminal until interrupted.
    Daemon,
    /// Show whether the daemon is running and what it is tracking.
    Status,
    /// Manage the repositories Muster tracks.
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Manage git worktrees.
    #[command(subcommand)]
    Worktree(WorktreeCommand),
    /// Start and stop sessions.
    #[command(subcommand)]
    Session(SessionCommand),
}

#[derive(Debug, Subcommand)]
enum ProjectCommand {
    /// Start tracking the git repository at PATH.
    Add {
        path: PathBuf,
        /// The name shown in the UI; defaults to the directory's name.
        #[arg(long)]
        name: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum WorktreeCommand {
    /// Create a worktree for PROJECT (its name, id or path).
    Add {
        project: String,
        /// The branch to check out, created from the project's current
        /// commit if it does not exist. Defaults to `muster/<random>`.
        #[arg(long)]
        branch: Option<String>,
    },
    /// Remove WORKTREE (its id or branch). Its branch is kept.
    Remove {
        worktree: String,
        /// Remove it even if it has uncommitted changes, discarding them.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Debug, Subcommand)]
enum SessionCommand {
    /// Start a session in WORKTREE (its id or branch).
    Start {
        worktree: String,
        /// What to run.
        #[arg(long, default_value = "shell")]
        harness: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// Stop SESSION (its id).
    Stop { session: String },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = match Paths::resolve(|key| std::env::var_os(key), nix::unistd::getuid().as_raw()) {
        Ok(paths) => paths,
        Err(error) => return fail(error),
    };
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(error) => return fail(error),
    };
    runtime.block_on(async {
        let socket = paths.socket();
        let outcome = match cli.command {
            Some(Commands::Daemon) => return daemon(paths).await,
            Some(Commands::Status) => return commands::status(&socket).await,
            Some(Commands::Project(ProjectCommand::Add { path, name })) => {
                commands::add_project(&socket, &path, name).await
            }
            Some(Commands::Worktree(WorktreeCommand::Add { project, branch })) => {
                commands::add_worktree(&socket, &project, branch).await
            }
            Some(Commands::Worktree(WorktreeCommand::Remove { worktree, force })) => {
                commands::remove_worktree(&socket, &worktree, force).await
            }
            Some(Commands::Session(SessionCommand::Start {
                worktree,
                harness,
                title,
            })) => commands::start_session(&socket, &worktree, harness, title).await,
            Some(Commands::Session(SessionCommand::Stop { session })) => {
                commands::stop_session(&socket, &session).await
            }
            None => {
                eprintln!(
                    "The terminal UI is not built yet. Run `muster --help` for what works today."
                );
                return ExitCode::from(2);
            }
        };
        match outcome {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(error),
        }
    })
}

async fn daemon(paths: Paths) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MUSTER_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    match muster_daemon::run(DaemonConfig::new(paths), shutdown_signal()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// Completes on SIGINT or SIGTERM.
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};
    match (
        signal(SignalKind::interrupt()),
        signal(SignalKind::terminate()),
    ) {
        (Ok(mut interrupt), Ok(mut terminate)) => {
            tokio::select! {
                _ = interrupt.recv() => {}
                _ = terminate.recv() => {}
            }
        }
        _ => {
            let _ = tokio::signal::ctrl_c().await;
        }
    }
}

fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("muster: {error}");
    ExitCode::FAILURE
}
