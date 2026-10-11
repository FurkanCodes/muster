//! The `muster` command. With no arguments it opens the terminal UI, starting
//! the daemon if none is running; its subcommands run the daemon and drive it
//! from scripts.

mod commands;
mod runs;

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
    /// Run the daemon in this terminal until interrupted; `muster daemon
    /// stop` stops a running one.
    Daemon {
        #[command(subcommand)]
        action: Option<DaemonCommand>,
        /// Leave the terminal's session, as the UI does when it starts the
        /// daemon, so closing the terminal does not stop it.
        #[arg(long)]
        detach: bool,
    },
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
    /// Work on GitHub issues, through the GitHub CLI (`gh`).
    #[command(subcommand)]
    Issue(IssueCommand),
    /// Hand issues to the autopilot and follow its runs: each builds in a
    /// container, passes the project's checks, opens a draft pull request
    /// and has it reviewed before it asks for you.
    #[command(subcommand)]
    Run(RunCommand),
    /// Credentials runs need, kept in the macOS Keychain.
    #[command(subcommand)]
    Auth(AuthCommand),
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Store the Claude Code token runs' agents use: make one with
    /// `claude setup-token`, then paste it when asked.
    Claude,
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Stop the running daemon, even one from another build of muster, as
    /// after an upgrade. Shells in sessions end; Claude Code sessions are
    /// resumed when the daemon starts again.
    Stop,
}

#[derive(Debug, Subcommand)]
enum IssueCommand {
    /// List PROJECT's open issues (its name, id or path).
    List { project: String },
    /// Start work on issue NUMBER of PROJECT: a worktree for it and a
    /// session with the issue as its first prompt.
    Start {
        project: String,
        number: u64,
        /// What to run: `claude` or `shell`.
        #[arg(long, default_value = "claude")]
        harness: String,
    },
}

/// RUN is a run's id, or its issue's number (`42` or `#42`): the run going
/// on, else the latest.
#[derive(Debug, Subcommand)]
enum RunCommand {
    /// Hand issue NUMBER of PROJECT (its name, id or path) to the
    /// autopilot. The project's `muster.toml` must name its checks.
    Start { project: String, number: u64 },
    /// The runs going on, those waiting on you first.
    List {
        /// Include the runs that have ended.
        #[arg(long)]
        all: bool,
    },
    /// Where RUN is, why, and its timeline.
    Show { run: String },
    /// Send RUN, which needs you, back to work with a fresh budget.
    Retry {
        run: String,
        /// Notes for its agent; `-` reads them from stdin.
        #[arg(long)]
        notes: Option<String>,
    },
    /// Send RUN, which is ready for you, back to its agent with what to
    /// change (`-` reads it from stdin); its pull request goes back to
    /// draft.
    Changes { run: String, comments: String },
    /// Drive RUN's building agent yourself: Muster stops its own work, and
    /// Claude Code carries on the agent's conversation in a session in the
    /// run's container, which `muster` opens.
    TakeOver { run: String },
    /// Give RUN back once you have driven it: the sessions in its
    /// container stop and your work goes to the checks.
    HandBack { run: String },
    /// Stop RUN for good: its container goes, its record stays.
    Cancel { run: String },
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
    /// Stop tracking PROJECT (its name, id or path). Its repository is not
    /// touched; its worktrees must be removed first.
    Remove { project: String },
    /// Show PROJECT's settings for runs: its `muster.toml` with your
    /// overrides, or what is wrong with them.
    Config { project: String },
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
    /// Start a session in WORKTREE (its id or branch, or a project's name
    /// for the project's own checkout).
    Start {
        worktree: String,
        /// What to run: `shell` or `claude`.
        #[arg(long, default_value = "shell")]
        harness: String,
        #[arg(long)]
        title: Option<String>,
        /// A first prompt, for harnesses that take one.
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Stop SESSION (its id).
    Stop { session: String },
    /// Start ended SESSION's program again: Claude Code resumes its
    /// conversation, a shell starts afresh.
    Restart { session: String },
    /// Take ended SESSION off the list; its history is kept.
    Remove { session: String },
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
            Some(Commands::Daemon {
                action: Some(DaemonCommand::Stop),
                ..
            }) => return commands::stop_daemon(&socket).await,
            Some(Commands::Daemon {
                action: None,
                detach,
            }) => {
                if detach {
                    // Fails only if already a session leader, which is fine.
                    let _ = nix::unistd::setsid();
                }
                return daemon(paths).await;
            }
            Some(Commands::Status) => return commands::status(&socket).await,
            Some(Commands::Project(ProjectCommand::Add { path, name })) => {
                commands::add_project(&socket, &path, name).await
            }
            Some(Commands::Project(ProjectCommand::Remove { project })) => {
                commands::remove_project(&socket, &project).await
            }
            Some(Commands::Project(ProjectCommand::Config { project })) => {
                commands::show_config(&socket, &paths, &project).await
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
                prompt,
            })) => commands::start_session(&socket, &worktree, harness, title, prompt).await,
            Some(Commands::Session(SessionCommand::Stop { session })) => {
                commands::stop_session(&socket, &session).await
            }
            Some(Commands::Session(SessionCommand::Restart { session })) => {
                commands::restart_session(&socket, &session).await
            }
            Some(Commands::Session(SessionCommand::Remove { session })) => {
                commands::remove_session(&socket, &session).await
            }
            Some(Commands::Auth(AuthCommand::Claude)) => return commands::store_claude_token(),
            Some(Commands::Run(command)) => run_command(&socket, command).await,
            Some(Commands::Issue(IssueCommand::List { project })) => {
                commands::list_issues(&socket, &project).await
            }
            Some(Commands::Issue(IssueCommand::Start {
                project,
                number,
                harness,
            })) => commands::start_issue(&socket, &project, number, harness).await,
            None => return ui(paths).await,
        };
        match outcome {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => fail(error),
        }
    })
}

async fn run_command(socket: &std::path::Path, command: RunCommand) -> commands::Outcome {
    match command {
        RunCommand::Start { project, number } => runs::start(socket, &project, number).await,
        RunCommand::List { all } => runs::list(socket, all).await,
        RunCommand::Show { run } => runs::show(socket, &run).await,
        RunCommand::Retry { run, notes } => runs::retry(socket, &run, notes).await,
        RunCommand::Changes { run, comments } => {
            runs::request_changes(socket, &run, comments).await
        }
        RunCommand::TakeOver { run } => runs::take_over(socket, &run).await,
        RunCommand::HandBack { run } => runs::hand_back(socket, &run).await,
        RunCommand::Cancel { run } => runs::cancel(socket, &run).await,
    }
}

/// Opens the terminal UI.
async fn ui(paths: Paths) -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    // MUSTER_NOTIFY=off keeps the UI from notifying.
    let notify = !matches!(
        std::env::var("MUSTER_NOTIFY").as_deref(),
        Ok("off" | "0" | "false" | "no")
    );
    let options = muster_tui::Options {
        socket: paths.socket(),
        start_daemon: Some(Box::new(move || start_daemon(&paths))),
        cwd,
        notify,
    };
    match muster_tui::run(options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// Starts `muster daemon --detach` in the background, logging to
/// `daemon.log` in the runtime directory. The daemon stays this process's
/// child until the UI exits, so a thread waits for it: a daemon that stops
/// while the UI runs must not linger as a zombie.
fn start_daemon(paths: &Paths) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&paths.runtime_dir)?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.runtime_dir.join("daemon.log"))?;
    let mut daemon = std::process::Command::new(std::env::current_exe()?)
        .args(["daemon", "--detach"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(log)
        .spawn()?;
    std::thread::Builder::new()
        .name("muster-reap-daemon".to_owned())
        .spawn(move || {
            let _ = daemon.wait();
        })?;
    Ok(())
}

async fn daemon(paths: Paths) -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("MUSTER_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let mut config = DaemonConfig::new(paths);
    // Tests measure more often than people need.
    let millis = |key| std::env::var(key).ok()?.parse::<u64>().ok();
    if let Some(ms) = millis("MUSTER_SAMPLE_MS") {
        config.sample_interval = std::time::Duration::from_millis(ms.max(10));
    }
    if let Some(ms) = millis("MUSTER_HISTORY_MS") {
        config.history_interval = std::time::Duration::from_millis(ms.max(10));
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
    match muster_daemon::run(config, shutdown_signal()).await {
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
    // A message can quote git, a path or an issue: no control characters.
    eprintln!(
        "muster: {}",
        muster_core::text::multi_line(&error.to_string())
    );
    ExitCode::FAILURE
}
