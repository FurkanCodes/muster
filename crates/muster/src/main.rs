//! The `muster` command. With no arguments it opens the terminal UI, starting
//! the daemon if none is running; its subcommands run the daemon and drive it
//! from scripts.

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

mod auth;
mod commands;
mod output;
mod reload;
mod runs;
mod upgrade;

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
        /// Start the daemon in the background, apart from this process and
        /// its terminal, and return: as the UI does when it starts one.
        /// Nothing that ends this process, or its terminal, stops it.
        #[arg(long)]
        detach: bool,
        /// The daemon `--detach` starts: in a session of its own, ignoring
        /// hang-ups.
        #[arg(long, hide = true)]
        background: bool,
        /// Take over from the daemon's previous version, whose handoff is
        /// at this descriptor: its restart in place (`muster reload`).
        #[arg(long, hide = true, value_name = "FD")]
        adopt: Option<i32>,
        /// Say (by the exit status) whether this program can take over a
        /// handoff in this format; asked before a restart in place.
        #[arg(long, hide = true, value_name = "FORMAT")]
        can_adopt: Option<u32>,
    },
    /// Move the running daemon onto the muster installed now, in place:
    /// every session keeps running, a shell's work and an agent's turn
    /// included. Run it after installing a new version; `muster upgrade`
    /// does both.
    Reload,
    /// Install the latest release of muster, checked against its published
    /// checksum, and move the daemon onto it with every session running.
    Upgrade,
    /// Show whether the daemon is running and what it is tracking.
    Status,
    /// Manage the projects Muster tracks: git repositories, or any folder.
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Manage git worktrees.
    #[command(subcommand)]
    Worktree(WorktreeCommand),
    /// Start and stop sessions.
    #[command(subcommand)]
    Session(SessionCommand),
    /// Tickets: a project's GitHub issues, through the GitHub CLI (`gh`),
    /// and agents on them.
    #[command(subcommand)]
    Issue(IssueCommand),
    /// Run the project's checks (`[checks]` in its `muster.toml`) in
    /// WORKTREE (its id or branch, or the project's name for its own
    /// checkout), showing their output; exits as they did. The run stays
    /// in the UI as a session; starting it again runs them again.
    Check { worktree: String },
    /// Have Claude Code review the change in WORKTREE against the branch it
    /// is to merge into, in plan mode (it changes nothing), as a session
    /// you read in `muster`.
    Review { worktree: String },
    /// Push WORKTREE's branch, as committed, with your own git, and open a
    /// draft pull request from it with your `gh` (or show the one open).
    Pr { worktree: String },
    /// The autopilot, set aside: runs that take an issue to a reviewed pull
    /// request in a container. Hidden; its code is kept.
    #[command(subcommand, hide = true)]
    Run(RunCommand),
    /// Sign-ins and tokens Muster keeps in the macOS Keychain: for Jira, Linear,
    /// Bitbucket, and the autopilot's runs.
    #[command(subcommand)]
    Auth(AuthCommand),
    /// Show the agents in the MacBook's notch while you look away from
    /// Muster (macOS). The UI starts it, unless Muster's own settings have
    /// it off; it ends when turned off, or when the daemon is gone.
    Notch {
        /// Start it apart from this process and its terminal, and return:
        /// as the UI does.
        #[arg(long)]
        detach: bool,
        /// The notch `--detach` starts, in a session of its own.
        #[arg(long, hide = true)]
        background: bool,
    },
}

#[derive(Debug, Subcommand)]
enum AuthCommand {
    /// Store the Claude Code token runs' agents use: make one with
    /// `claude setup-token`, then paste it when asked.
    #[command(hide = true)]
    Claude,
    /// Store an API token for Bitbucket, for pull requests: pasted when
    /// asked, not shown, or piped in.
    Bitbucket {
        /// The Bitbucket: `bitbucket.org`, or a Data Center's host.
        #[arg(long, default_value = "bitbucket.org")]
        host: String,
    },
    /// Sign in to Jira Cloud through Atlassian in your browser, for the
    /// tickets of projects whose `[tickets]` say `tracker = "jira"`; or,
    /// with --token, store a token for a site (Jira Data Center too).
    Jira {
        /// Store a token instead of signing in in the browser.
        #[arg(long)]
        token: bool,
        /// The site the token is for: `acme.atlassian.net`, or a Data
        /// Center's address.
        #[arg(long)]
        site: Option<String>,
    },
    /// Sign in to Linear in your browser; or, with --token, store a
    /// personal API key.
    Linear {
        /// Store a personal API key instead of signing in in the browser.
        #[arg(long)]
        token: bool,
    },
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
    /// List PROJECT's open tickets (its name, id or path) from its tracker,
    /// with the agents on each.
    List { project: String },
    /// Start an agent on ticket KEY of PROJECT (`42`, `#42` or `APP-7`): a
    /// worktree for it and a session with the ticket as its first prompt.
    Start {
        project: String,
        #[arg(value_parser = ticket_key)]
        key: muster_core::TicketKey,
        /// What to run: `claude`, `codex`, `gemini`, `opencode`, `copilot`
        /// or `shell`.
        #[arg(long, default_value = "claude")]
        harness: String,
        /// Another agent on the issue, in a worktree of its own, rather
        /// than in the issue's worktree as it is.
        #[arg(long)]
        another: bool,
    },
    /// Close ticket KEY of PROJECT in its tracker. Its agents and worktrees
    /// stay; close and remove them with `muster session` and `muster
    /// worktree`.
    Close {
        project: String,
        #[arg(value_parser = ticket_key)]
        key: muster_core::TicketKey,
    },
}

/// A ticket's key as written on the command line.
fn ticket_key(text: &str) -> Result<muster_core::TicketKey, String> {
    muster_core::TicketKey::parse(text)
        .ok_or_else(|| format!("{text:?} is not a ticket: write `42`, `#42` or `APP-7`"))
}

/// RUN is a run's id, or its issue's number (`42` or `#42`): the run going
/// on, else the latest.
#[derive(Debug, Subcommand)]
enum RunCommand {
    /// Hand issue NUMBER of PROJECT (its name, id or path) to the
    /// autopilot. The project's `muster.toml` must name its checks.
    Start {
        project: String,
        number: u64,
        /// Hand over the work already done in this worktree (its id or
        /// branch, or the project's name for its own checkout): the run
        /// takes its branch, as committed, and starts with the checks.
        #[arg(long)]
        from: Option<String>,
    },
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
    /// Start tracking the folder at PATH: a git repository, or any folder.
    /// A plain folder has sessions and checks in it, but no worktrees,
    /// tickets, reviews or pull requests until it is a repository.
    Add {
        path: PathBuf,
        /// The name shown in the UI; defaults to the directory's name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Stop tracking PROJECT (its name, id or path). Its folder is not
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
        /// What to run: `shell`, or a coding agent: `claude`, `codex`,
        /// `gemini`, `opencode` or `copilot`.
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

/// The time now, for a command's own deadlines. The daemon reads time
/// through its `Clock` (`ARC-03`); a command reads it here.
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: a command's one monotonic clock"
)]
pub(crate) fn instant_now() -> std::time::Instant {
    std::time::Instant::now()
}

#[expect(clippy::too_many_lines, reason = "one arm for each subcommand")]
fn main() -> ExitCode {
    let cli = Cli::parse();
    let paths = match Paths::resolve(|key| std::env::var_os(key), nix::unistd::getuid().as_raw()) {
        Ok(paths) => paths,
        Err(error) => return fail(error),
    };
    // The notch takes the main thread, which AppKit needs, before any
    // runtime is made on it.
    if let Some(Commands::Notch { detach, background }) = cli.command {
        return notch(&paths, detach, background);
    }
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
                background,
                adopt,
                can_adopt,
            }) => {
                if let Some(format) = can_adopt {
                    return if format == muster_daemon::HANDOFF_FORMAT {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                }
                if detach {
                    return start_in_background();
                }
                if background {
                    // A session of its own, with no terminal to hang it up.
                    // Fails only if already a session leader, which is fine.
                    let _ = nix::unistd::setsid();
                }
                return daemon(paths, background, adopt).await;
            }
            Some(Commands::Reload) => {
                return reload::reload_daemon(&socket, &paths.runtime_dir).await;
            }
            Some(Commands::Upgrade) => return upgrade::run(&socket, &paths.runtime_dir).await,
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
            Some(Commands::Auth(AuthCommand::Bitbucket { host })) => {
                return auth::bitbucket(&host).await;
            }
            Some(Commands::Auth(AuthCommand::Jira { token, site })) => {
                return auth::jira(token, site.as_deref()).await;
            }
            Some(Commands::Auth(AuthCommand::Linear { token })) => {
                return auth::linear(token).await;
            }
            // Shown before the runtime was made.
            Some(Commands::Notch { .. }) => Ok(()),
            Some(Commands::Run(command)) => run_command(&socket, command).await,
            Some(Commands::Check { worktree }) => return commands::check(&socket, &worktree).await,
            Some(Commands::Review { worktree }) => commands::review(&socket, &worktree).await,
            Some(Commands::Pr { worktree }) => {
                commands::open_pull_request(&socket, &worktree).await
            }
            Some(Commands::Issue(IssueCommand::List { project })) => {
                commands::list_issues(&socket, &project).await
            }
            Some(Commands::Issue(IssueCommand::Close { project, key })) => {
                commands::close_issue(&socket, &project, key).await
            }
            Some(Commands::Issue(IssueCommand::Start {
                project,
                key,
                harness,
                another,
            })) => commands::start_issue(&socket, &project, key, harness, another).await,
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
        RunCommand::Start {
            project,
            number,
            from,
        } => runs::start(socket, &project, number, from).await,
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
    // MUSTER_MOTION=off keeps every mark still.
    let motion = !matches!(
        std::env::var("MUSTER_MOTION").as_deref(),
        Ok("off" | "0" | "false" | "no")
    );
    // MUSTER_NOTCH=off keeps the UI from starting the notch, whatever
    // Muster's own settings say.
    let notch = cfg!(target_os = "macos")
        && !matches!(
            std::env::var("MUSTER_NOTCH").as_deref(),
            Ok("off" | "0" | "false" | "no")
        );
    let notch_paths = paths.clone();
    let start_notch: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>> =
        notch.then(|| {
            Box::new(move || start_notch(&notch_paths))
                as Box<dyn Fn() -> std::io::Result<()> + Send + Sync>
        });
    let options = muster_tui::Options {
        socket: paths.socket(),
        start_daemon: Some(Box::new(move || start_daemon(&paths))),
        cwd,
        notify,
        motion,
        start_notch,
        // What macOS says the terminal is, to bring it to the front.
        terminal_app: std::env::var("__CFBundleIdentifier").ok(),
    };
    match muster_tui::run(options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// Starts `muster daemon --detach`, logging to `daemon.log` in the runtime
/// directory. That starts the daemon itself apart from this process and
/// returns at once, so the daemon is never the UI's child: whatever ends
/// the UI, its whole process tree included, leaves the daemon and its
/// sessions running (`PRD-09`). A thread waits for the short-lived starter
/// so it never lingers as a zombie.
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

/// Starts `muster notch --detach`, which starts the notch apart from the
/// UI and returns at once; a notch already running keeps on, and the new
/// one ends. A thread waits for the short-lived starter.
fn start_notch(paths: &Paths) -> std::io::Result<()> {
    let mut starter = std::process::Command::new(std::env::current_exe()?)
        .args(["notch", "--detach"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .env("MUSTER_RUNTIME_DIR", &paths.runtime_dir)
        .spawn()?;
    std::thread::Builder::new()
        .name("muster-reap-notch".to_owned())
        .spawn(move || {
            let _ = starter.wait();
        })?;
    Ok(())
}

/// `muster notch`: with `detach`, starts the notch as a process of its own,
/// its errors in `notch.log` in the runtime directory (written afresh each
/// time), and returns; otherwise shows it until it ends.
fn notch(paths: &Paths, detach: bool, background: bool) -> ExitCode {
    use std::os::unix::process::CommandExt;
    if detach {
        let started = std::fs::File::create(paths.runtime_dir.join("notch.log")).and_then(|log| {
            std::process::Command::new(std::env::current_exe()?)
                .args(["notch", "--background"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(log)
                .spawn()
        });
        return match started {
            // Not waited for: it runs on once this process has gone.
            Ok(_notch) => ExitCode::SUCCESS,
            Err(error) => fail(format!("could not start the notch: {error}")),
        };
    }
    if background {
        // A session of its own, with no terminal to hang it up.
        let _ = nix::unistd::setsid();
    }
    // Moves onto the program installed now, as the daemon has: the notch
    // speaks its version's protocol and draws its version's notch.
    let relaunch = |version: &str| match std::env::current_exe() {
        Ok(exe) => std::process::Command::new(exe)
            .args(["notch", "--background"])
            .env(muster_notch::MOVED_FOR, version)
            .exec(),
        Err(error) => error,
    };
    match muster_notch::run(&paths.socket(), &paths.runtime_dir, relaunch) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// `muster daemon --detach`: starts the daemon as a process of its own and
/// returns. Once this one exits the daemon is no longer anyone's child but
/// the system's, so nothing that walks this process's tree reaches it. Its
/// errors go where this process's do (the UI's `daemon.log`).
fn start_in_background() -> ExitCode {
    let started = std::env::current_exe().and_then(|exe| {
        std::process::Command::new(exe)
            .args(["daemon", "--background"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::inherit())
            .spawn()
    });
    match started {
        // Not waited for: it runs on once this process has gone.
        Ok(_daemon) => ExitCode::SUCCESS,
        Err(error) => fail(format!("could not start the daemon: {error}")),
    }
}

async fn daemon(paths: Paths, background: bool, adopt: Option<i32>) -> ExitCode {
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

fn fail(error: impl std::fmt::Display) -> ExitCode {
    // A message can quote git, a path or an issue: no control characters.
    eprintln!(
        "muster: {}",
        muster_core::text::multi_line(&error.to_string())
    );
    ExitCode::FAILURE
}
