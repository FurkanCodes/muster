//! The `muster` command. With no arguments it opens the terminal UI, starting
//! the daemon if none is running; its subcommands run the daemon and drive it
//! from scripts.

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

mod auth;
mod commands;
mod daemon_run;
mod daemon_start;
mod explain;
mod json;
mod launchd;
#[cfg(target_os = "macos")]
mod mac_app;
mod output;
mod person;
mod plugins;
mod reload;
mod runs;
mod ssh;
mod upgrade;
mod verbs;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use muster_core::Paths;

#[derive(Debug, Parser)]
#[command(
    version,
    about = "A cockpit for coding agents, with an optional autopilot.",
    after_help = "Start an agent: muster <agent> [PROMPT], such as `muster claude \"fix the \
                  login redirect\"`: in a new worktree of the project you are in, in the \
                  background, and back at once (`muster claude --help`)."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    /// Ask Muster on MACHINE instead, through ssh (`muster machine add`):
    /// `muster --on vm list agents`, `muster --on vm attach fix-login/2`.
    /// `MUSTER_MACHINE` says it for a whole shell.
    #[arg(long, global = true, value_name = "MACHINE")]
    on: Option<String>,
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
        /// What the launchd job `--detach` has run on macOS does: start the
        /// daemon with the environment saved in FILE, then remove it.
        #[arg(long, hide = true, value_name = "FILE")]
        from_job: Option<PathBuf>,
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
    Status {
        /// Print it as one JSON document, for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Manage the projects Muster tracks: git repositories, or any folder.
    #[command(subcommand)]
    Project(ProjectCommand),
    /// Run the project's checks (`[checks]` in its `muster.toml`) in
    /// workspace WS (as `muster list workspaces` names it), showing their
    /// output; exits as they did. The run stays in the UI as a session;
    /// starting it again runs them again.
    Check {
        #[arg(value_name = "WS")]
        workspace: String,
    },
    /// Have Claude Code review the change in workspace WS against the
    /// branch it is to merge into, in plan mode (it changes nothing), as a
    /// session you read in `muster`.
    Review {
        #[arg(value_name = "WS")]
        workspace: String,
    },
    /// Push workspace WS's branch, as committed, with your own git, and
    /// open a draft pull request from it with your `gh` (or show the one
    /// open).
    Pr {
        #[arg(value_name = "WS")]
        workspace: String,
    },
    /// The autopilot, set aside: runs that take an issue to a reviewed pull
    /// request in a container. Hidden; its code is kept.
    #[command(subcommand, hide = true)]
    Run(RunCommand),
    /// Plugins: install one from a git repository or link a folder, turn
    /// one on or off, update or remove it. Each is shown before it is
    /// added, and asked about.
    #[command(subcommand)]
    Plugin(plugins::PluginCommand),
    /// Sign-ins and tokens Muster keeps in the macOS Keychain: for Jira, Linear,
    /// Bitbucket, plugins' secrets, and the autopilot's runs.
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
    #[command(flatten)]
    Verb(verbs::Verb),
    /// `muster <agent> [PROMPT]`: start a coding agent in the background,
    /// in a new worktree of the project you are in, on PROMPT, and return
    /// (`muster claude --help` says more).
    #[command(external_subcommand)]
    Agent(Vec<String>),
    /// Serve Muster's own ticket plugin ID (`github`, `gitlab`, `jira`,
    /// `linear`) on stdin and stdout: what the daemon runs to reach a
    /// project's tickets.
    #[command(hide = true)]
    PluginRun { id: String },
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
    /// Store secret NAME for plugin ID, as its manifest names it: pasted
    /// when asked, not shown, or piped in. Muster gives it to that plugin
    /// alone.
    Plugin { id: String, name: String },
}

#[derive(Debug, Subcommand)]
enum DaemonCommand {
    /// Stop the running daemon, even one from another build of muster, as
    /// after an upgrade. Shells in sessions end; Claude Code sessions are
    /// resumed when the daemon starts again.
    Stop,
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
        /// The space it goes in; without it, the only space, or a Home
        /// made for it where there is none.
        #[arg(long, value_name = "SPACE")]
        space: Option<String>,
    },
    /// Stop tracking PROJECT (its name, id or path). Its folder is not
    /// touched; its worktrees must be removed first.
    Remove { project: String },
    /// Show PROJECT's settings for runs: its `muster.toml` with your
    /// overrides, or what is wrong with them.
    Config { project: String },
}

/// Whether `command` asks a daemon, and so can ask another machine's
/// (`--on`): not the UI, the daemon's own commands, the notch, sign-ins,
/// nor the commands about this machine's shells and machines.
fn on_another_machine(command: Option<&Commands>) -> bool {
    match command {
        None => true,
        Some(
            Commands::Daemon { .. }
            | Commands::Reload
            | Commands::Upgrade
            | Commands::Notch { .. }
            | Commands::Auth(_)
            | Commands::Plugin(_)
            | Commands::PluginRun { .. },
        ) => false,
        Some(Commands::Verb(verb)) => verbs::on_another_machine(verb),
        Some(_) => true,
    }
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
    // launchd's job, whose environment is launchd's until the daemon is
    // given the person's.
    if let Some(Commands::Daemon {
        from_job: Some(file),
        ..
    }) = &cli.command
    {
        return launchd::run_job(file);
    }
    // A ticket plugin the daemon runs: it needs no paths, and its stdout
    // is the protocol's.
    if let Some(Commands::PluginRun { id }) = &cli.command {
        return plugin_run(id);
    }
    let paths = match Paths::resolve(|key| std::env::var_os(key), nix::unistd::getuid().as_raw()) {
        Ok(paths) => paths,
        Err(error) => return fail(error),
    };
    // `--on` for a command that runs here is refused; `MUSTER_MACHINE`, set
    // for a whole shell, is left to the commands that ask a daemon.
    let remote = on_another_machine(cli.command.as_ref());
    if let Some(machine) = cli.on.as_deref().filter(|_| !remote) {
        return fail(format!(
            "--on {machine} asks another machine's Muster; this command runs here"
        ));
    }
    let on = cli.on.clone().or_else(|| {
        std::env::var("MUSTER_MACHINE")
            .ok()
            .filter(|machine| !machine.is_empty())
    });
    if let Some(machine) = on.as_deref().filter(|_| remote)
        && let Err(error) = ssh::set_remote(&paths, machine)
    {
        return fail(error);
    }
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
                ..
            }) => {
                if let Some(format) = can_adopt {
                    return if format == muster_daemon::HANDOFF_FORMAT {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    };
                }
                if detach {
                    return start_in_background(&paths);
                }
                if background {
                    // A session of its own, with no terminal to hang it up.
                    // Fails only if already a session leader, which is fine.
                    let _ = nix::unistd::setsid();
                }
                return daemon_run::daemon(paths, background, adopt).await;
            }
            Some(Commands::Reload) => {
                return reload::reload_daemon(&socket, &paths.runtime_dir).await;
            }
            Some(Commands::Upgrade) => return upgrade::run(&socket, &paths.runtime_dir).await,
            Some(Commands::Status { json }) => return commands::status(&socket, json).await,
            Some(Commands::Project(ProjectCommand::Add { path, name, space })) => {
                commands::add_project(&socket, &path, name, space.as_deref()).await
            }
            Some(Commands::Project(ProjectCommand::Remove { project })) => {
                commands::remove_project(&socket, &project).await
            }
            Some(Commands::Project(ProjectCommand::Config { project })) => {
                commands::show_config(&socket, &paths, &project).await
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
            Some(Commands::Auth(AuthCommand::Plugin { id, name })) => {
                return auth::plugin(&id, &name).await;
            }
            // Run before the runtime was made.
            Some(Commands::Notch { .. } | Commands::PluginRun { .. }) => Ok(()),
            Some(Commands::Run(command)) => run_command(&socket, command).await,
            Some(Commands::Plugin(command)) => plugins::run(&socket, command).await,
            Some(Commands::Check { workspace }) => {
                return commands::check(&socket, &workspace).await;
            }
            Some(Commands::Review { workspace }) => commands::review(&socket, &workspace).await,
            Some(Commands::Pr { workspace }) => {
                commands::open_pull_request(&socket, &workspace).await
            }
            Some(Commands::Verb(verb)) => {
                return match verbs::run(&paths, verb).await {
                    verbs::Then::Exit(code) => code,
                    verbs::Then::Open(session) => ui(paths, Some(session)).await,
                };
            }
            Some(Commands::Agent(args)) => return verbs::start::run(&paths, args).await,
            None => return ui(paths, None).await,
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

/// Opens the terminal UI, on `open` if given.
async fn ui(paths: Paths, open: Option<muster_core::SessionId>) -> ExitCode {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let paths_socket = paths.socket();
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
    // Another machine's Muster: reached through ssh each time the link is
    // lost, nothing of this machine's offered for it.
    let remote = ssh::remote();
    let connect = remote.map(ssh::connector);
    let here = remote.is_none();
    let start_daemon: Option<Box<dyn Fn() -> std::io::Result<()> + Send + Sync>> =
        here.then(|| Box::new(move || daemon_start::start(&paths)) as _);
    let start_notch = start_notch.filter(|_| here);
    let cwd = if here { cwd } else { PathBuf::from("/") };
    let options = muster_tui::Options {
        socket: paths_socket,
        connect,
        machine: remote.map(|remote| remote.name.clone()),
        start_daemon,
        cwd,
        notify,
        motion,
        start_notch,
        // What macOS says the terminal is, to bring it to the front.
        terminal_app: std::env::var("__CFBundleIdentifier").ok(),
        open,
    };
    match muster_tui::run(options).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
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
/// time), and returns; otherwise shows it until it ends. On a Mac it is
/// Muster's helper, which shows notifications of the Mac's own too, so it
/// runs as Muster's app (`mac_app`), where it can.
/// Serves Muster's own ticket plugin `id` until the daemon closes its
/// stdin, on a runtime of one thread: it waits on `gh`, `glab` or the
/// network.
fn plugin_run(id: &str) -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => return fail(error),
    };
    match runtime.block_on(muster_daemon::plugin_run(id)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

fn notch(paths: &Paths, detach: bool, background: bool) -> ExitCode {
    use std::os::unix::process::CommandExt;
    #[cfg(target_os = "macos")]
    if detach && let Some(started) = open_app(paths) {
        return started;
    }
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
    // speaks its version's protocol and draws its version's notch. As
    // Muster's app, the `muster` that opened it makes the app again first.
    let relaunch = |version: &str| match std::env::var_os(OPENED_BY) {
        Some(muster) => std::process::Command::new(muster)
            .args(["notch", "--detach"])
            .env(muster_notch::MOVED_FOR, version)
            .exec(),
        None => match std::env::current_exe() {
            Ok(exe) => std::process::Command::new(exe)
                .args(["notch", "--background"])
                .env(muster_notch::MOVED_FOR, version)
                .exec(),
            Err(error) => error,
        },
    };
    match muster_notch::run(&paths.socket(), &paths.runtime_dir, relaunch) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => fail(error),
    }
}

/// Set on Muster's app as it is opened: the `muster` that opened it, which
/// makes the app again for another version.
const OPENED_BY: &str = "MUSTER_OPENED_BY";

/// Opens Muster's helper as Muster's app, made or made again from this
/// `muster`, with what it needs passed on; `None` where it cannot be,
/// saying why, for the helper to run as this program instead.
#[cfg(target_os = "macos")]
fn open_app(paths: &Paths) -> Option<ExitCode> {
    let exe = std::env::current_exe().ok()?;
    let app = match mac_app::ensure(&paths.data_dir, &exe) {
        Ok(app) => app,
        Err(error) => {
            eprintln!(
                "muster: no notifications of the Mac's own: could not make Muster's app: {error}"
            );
            return None;
        }
    };
    let mut open = std::process::Command::new("/usr/bin/open");
    open.args(["-g", "-n"])
        .arg("--env")
        .arg(format!(
            "MUSTER_RUNTIME_DIR={}",
            paths.runtime_dir.display()
        ))
        .arg("--env")
        .arg(format!("MUSTER_DATA_DIR={}", paths.data_dir.display()))
        .arg("--env")
        .arg(format!("{OPENED_BY}={}", exe.display()))
        .arg("--stderr")
        .arg(paths.runtime_dir.join("notch.log"));
    if let Some(version) = std::env::var_os(muster_notch::MOVED_FOR) {
        open.arg("--env").arg(format!(
            "{}={}",
            muster_notch::MOVED_FOR,
            version.to_string_lossy()
        ));
    }
    let opened = open
        .arg(&app)
        .args(["--args", "notch", "--background"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .status();
    match opened {
        Ok(status) if status.success() => Some(ExitCode::SUCCESS),
        _ => {
            eprintln!("muster: could not open Muster's app; the notch runs without it");
            None
        }
    }
}

/// `muster daemon --detach`: starts the daemon as a process of its own and
/// returns. Once this one exits the daemon is no longer anyone's child but
/// the system's, so nothing that walks this process's tree reaches it. On
/// macOS, at the Mac's own screen, launchd starts it, so it is in no
/// terminal app's job either and quitting the app leaves it running; its
/// log is then `daemon.log` in the runtime directory. Where launchd does
/// not, this process starts it, saying why, and its errors go where this
/// process's do (the UI's `daemon.log`).
fn start_in_background(paths: &Paths) -> ExitCode {
    if launchd::wanted(|key| std::env::var_os(key)) {
        let through_launchd = launchd::Job::for_this_process(&paths.runtime_dir)
            .map_err(|error| format!("could not tell which muster this is: {error}"))
            .and_then(|job| {
                launchd::start(&launchd::System, &job, std::env::vars_os())
                    .map_err(|error| error.to_string())
            });
        match through_launchd {
            Ok(()) => return ExitCode::SUCCESS,
            Err(error) => eprintln!(
                "muster: starting the daemon directly, not through launchd: {}",
                muster_core::text::multi_line(&error)
            ),
        }
    }
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

pub(crate) fn fail(error: impl std::fmt::Display) -> ExitCode {
    // A message can quote git, a path or an issue: no control characters.
    eprintln!(
        "muster: {}",
        muster_core::text::multi_line(&error.to_string())
    );
    ExitCode::FAILURE
}
