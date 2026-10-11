//! The `muster` command. With no arguments it will open the terminal UI; its
//! subcommands run the daemon and drive it from scripts.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use muster_client::{ClientError, Connection};
use muster_core::Paths;
use muster_core::protocol::{Command, Reply};
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

const CLIENT: &str = concat!("muster ", env!("CARGO_PKG_VERSION"));

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
        match cli.command {
            Some(Commands::Daemon) => daemon(paths).await,
            Some(Commands::Status) => status(paths).await,
            Some(Commands::Project(ProjectCommand::Add { path, name })) => {
                add_project(paths, path, name).await
            }
            None => {
                eprintln!(
                    "The terminal UI is not built yet. Run `muster --help` for what works today."
                );
                ExitCode::from(2)
            }
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

async fn status(paths: Paths) -> ExitCode {
    let mut connection = match Connection::connect(&paths.socket(), CLIENT).await {
        Ok(connection) => connection,
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running. Start it with `muster daemon`.");
            return ExitCode::FAILURE;
        }
        Err(error) => return fail(error),
    };
    let snapshot = match connection.subscribe().await {
        Ok(snapshot) => snapshot,
        Err(error) => return fail(error),
    };
    let welcome = connection.welcome();
    println!(
        "The daemon is running: muster {} (pid {}, protocol {}).",
        welcome.daemon_version, welcome.daemon_pid, welcome.version
    );
    println!(
        "{} projects, {} sessions.",
        snapshot.projects.len(),
        snapshot.sessions.len()
    );
    for project in &snapshot.projects {
        println!("  {}  {}", project.name, project.root.display());
    }
    ExitCode::SUCCESS
}

async fn add_project(paths: Paths, path: PathBuf, name: Option<String>) -> ExitCode {
    // The daemon runs elsewhere, so it needs a path that does not depend on
    // this shell's working directory.
    let root = match std::path::absolute(&path) {
        Ok(root) => root,
        Err(error) => return fail(error),
    };
    let reply = match Connection::connect(&paths.socket(), CLIENT).await {
        Ok(mut connection) => connection.command(Command::AddProject { root, name }).await,
        Err(error) => Err(error),
    };
    match reply {
        Ok(Reply::ProjectAdded { id }) => {
            println!("Added {} as {id}.", path.display());
            ExitCode::SUCCESS
        }
        Ok(other) => fail(format!("unexpected reply {other:?}")),
        Err(error) => fail(error),
    }
}

fn fail(error: impl std::fmt::Display) -> ExitCode {
    eprintln!("muster: {error}");
    ExitCode::FAILURE
}
