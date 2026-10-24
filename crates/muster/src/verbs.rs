//! The verbs that drive agents from a shell (`PRODUCT.md` §6b): start one
//! on a prompt and get the shell back (`muster claude "…"`), list them, send
//! one a prompt, wait for them, read what one shows, and open one; make
//! workspaces and go into them, and start terminals there.

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use muster_client::Connection;
use muster_core::protocol::{Command, Snapshot};
use muster_core::{Paths, SessionId, SessionStatus};

mod control;
mod init;
pub(crate) mod list;
mod logs;
mod send;
pub(crate) mod start;
mod tab;
pub(crate) mod targets;
mod terminals;
mod tickets;
pub(crate) mod wait;
mod workspaces;

use self::wait::{Until, Waited, Waiting};
use crate::commands::{self, shown};

/// How long an agent sent a prompt has to be seen at work on it before a
/// wait calls it stalled.
const BEGIN_WITHIN: Duration = Duration::from_secs(10);

#[derive(Debug, Subcommand)]
pub(crate) enum Verb {
    /// List what Muster has: `muster list workspaces`, `list agents`,
    /// `list terminals`, `list projects`, or every session. In a workspace,
    /// its agents and terminals; elsewhere in a project, the project's.
    List {
        #[arg(value_enum)]
        what: Option<list::Listing>,
        /// Every project's.
        #[arg(long)]
        all: bool,
        /// Print them as one JSON document, for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Show SESSION (its number in the workspace you are in, `WS/N`, its
    /// id, name or branch) straight in this terminal tab, your keys in it;
    /// Ctrl-Q (Settings › Keys) leaves it running. Inside a Muster
    /// terminal, it takes that terminal's place, and Ctrl-Q comes back.
    Attach {
        session: String,
        /// Open it in the terminal UI instead.
        #[arg(long)]
        ui: bool,
    },
    /// Workspaces: worktrees of a project (or a project's own checkout),
    /// each with its terminals and agents.
    #[command(subcommand, visible_alias = "ws")]
    Workspace(workspaces::WorkspaceCommand),
    /// Go into workspace WS (or the one you are in): this tab shows its
    /// terminal, the one used last or a new one. What you start there,
    /// `muster claude` or another terminal, belongs to it and shows in its
    /// place; Ctrl-Q goes back.
    Enter {
        #[arg(value_name = "WS")]
        workspace: Option<String>,
        /// A new terminal, even where one runs.
        #[arg(long)]
        new: bool,
    },
    /// A workspace's terminals.
    #[command(subcommand, visible_alias = "t")]
    Term(terminals::TermCommand),
    /// Ticket KEY (`42`, `#42`, `APP-7`) of the project you are in: read it;
    /// or `claude` (any agent) puts one on it, in the workspace you are in,
    /// which becomes the ticket's (else in the ticket's own); `assign
    /// [WS]` makes a workspace the ticket's, `unassign` no longer; `open`
    /// shows it in the browser; `close` closes it in its tracker, after
    /// asking.
    Ticket {
        #[arg(value_parser = tickets::ticket_key)]
        key: muster_core::TicketKey,
        /// open, close, assign [WS], unassign, or an agent's name.
        words: Vec<String>,
        /// The project (its name, id or folder), where you are not in one.
        #[arg(short, long)]
        project: Option<String>,
        /// Close it without asking.
        #[arg(short, long)]
        yes: bool,
        /// Another agent on it, in a worktree of its own.
        #[arg(long)]
        another: bool,
        /// In a workspace's terminal, start it and stay, rather than show
        /// it in the terminal's place.
        #[arg(long)]
        bg: bool,
    },
    /// Join standard input and output to this machine's daemon: what
    /// `--on` has ssh run on a machine.
    #[command(hide = true)]
    Bridge,
    /// Machines of your own, reached with ssh, where Muster runs too.
    #[command(subcommand)]
    Machine(crate::ssh::MachineCommand),
    /// Muster on MACHINE (saved with `muster machine add`, or any host your
    /// ssh knows): checks it, installs Muster there after asking, and opens
    /// its Muster, or runs one command there (`muster ssh vm list agents`).
    /// Its agents keep working while this computer sleeps.
    Ssh {
        machine: String,
        /// Install or update Muster there without asking.
        #[arg(long, conflicts_with = "no_install")]
        install: bool,
        /// Never install or update it there.
        #[arg(long)]
        no_install: bool,
        /// Say what agents need there (signed in, git, gh), and stop unless
        /// a command follows.
        #[arg(long)]
        check: bool,
        /// Install Claude Code there with its own installer, without asking.
        #[arg(long)]
        install_claude: bool,
        /// The folder there to run it in.
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
        /// A command to run there, as `ssh` runs one, with Muster's PATH
        /// (`muster ssh vm claude auth login`), rather than Muster's UI.
        /// Muster's own commands there are `muster --on MACHINE …`.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        words: Vec<String>,
    },
    /// Give SESSION a name.
    Rename {
        session: String,
        #[arg(required = true)]
        name: Vec<String>,
    },
    /// Stop SESSION, after asking where it runs, and take it off the list.
    Close {
        session: String,
        /// Stop it without asking.
        #[arg(short, long)]
        yes: bool,
    },
    /// Print functions for your shell, to add to its rc file as
    /// `eval "$(muster init zsh)"`: an agent typed by its name in a
    /// workspace's terminal (`claude "fix it"`) then starts as a session of
    /// that workspace. Nothing else changes.
    Init {
        #[arg(value_enum)]
        shell: init::Shell,
    },
    /// Type TEXT into SESSION and press Enter: a prompt for its agent, or a
    /// command for a shell. `-` reads it from standard input.
    Send {
        session: String,
        text: Vec<String>,
        /// Type it without pressing Enter.
        #[arg(long)]
        no_enter: bool,
        /// Send it even while the agent asks you for a permission, where it
        /// would answer that rather than prompt.
        #[arg(long)]
        force: bool,
    },
    /// Give SESSION's agent a prompt and wait for its answer: `send` and
    /// `wait` at once, with no gap between them for a turn to slip through.
    Prompt {
        session: String,
        text: Vec<String>,
        /// Give up after this long (30s, 5m, 2h).
        #[arg(long, value_parser = wait::duration)]
        timeout: Option<Duration>,
        /// Send it even while the agent asks you for a permission.
        #[arg(long)]
        force: bool,
    },
    /// Wait until SESSIONS finish their turn, need you, or end; exit 0
    /// finished, 3 needs you, 4 ended badly, 124 out of time. Prints each
    /// one's last answer.
    Wait {
        #[arg(required = true)]
        sessions: Vec<String>,
        /// Stop when the first of them does, rather than all.
        #[arg(long)]
        any: bool,
        /// What to stop for, of finished, needs-you and exited; all three
        /// if not given.
        #[arg(long, value_delimiter = ',')]
        until: Vec<Until>,
        /// Give up after this long (30s, 5m, 2h).
        #[arg(long, value_parser = wait::duration)]
        timeout: Option<Duration>,
        /// Wait until the session shows this text instead (exit 0; 4 if it
        /// ends first, 124 out of time): for a terminal's command or server.
        #[arg(long = "for", value_name = "TEXT", conflicts_with_all = ["any", "until"])]
        for_text: Option<String>,
    },
    /// Press keys in SESSION by name: `muster keys fix-login/2 down enter`,
    /// or esc, tab, backspace, ctrl+c, alt+x, f1… or one character each.
    Keys {
        session: String,
        #[arg(required = true, allow_hyphen_values = true)]
        keys: Vec<String>,
    },
    /// Take terminal SESSION to workspace WS: a new terminal there under its
    /// name, shown where it was shown, and the old one closed. An agent is
    /// never moved: its work is in its workspace.
    Move {
        session: String,
        #[arg(long, value_name = "WS")]
        to: String,
    },
    /// Print what SESSION shows now, as text; its agent's last answer with
    /// --reply; or, with -f, its terminal's output as it comes.
    Logs {
        session: String,
        /// Its agent's last answer, rather than its screen.
        #[arg(long, conflicts_with = "follow")]
        reply: bool,
        /// Write its terminal's output as it comes, until it ends.
        #[arg(short, long)]
        follow: bool,
        /// Only the last N lines of what it shows.
        #[arg(short = 'n', long, value_name = "N")]
        lines: Option<usize>,
    },
    /// Stop SESSION (its id, name or branch).
    Stop { session: String },
}

/// Whether `verb` can ask another machine's Muster (`--on`): not those
/// about this machine's machines, shells and bridge.
pub(crate) fn on_another_machine(verb: &Verb) -> bool {
    !matches!(
        verb,
        Verb::Ssh { .. } | Verb::Machine(_) | Verb::Init { .. } | Verb::Bridge
    )
}

/// What `muster` does once a verb has run.
pub(crate) enum Then {
    Exit(ExitCode),
    /// Open the terminal UI on this session.
    Open(SessionId),
}

pub(crate) async fn run(paths: &Paths, verb: Verb) -> Then {
    let ran = match verb {
        Verb::List { what, all, json } => list::run(paths, what, all, json).await,
        Verb::Attach { session, ui } => match attach(paths, &session).await {
            Ok(id) if ui => return Then::Open(id),
            Ok(id) => tab::show(paths, id).await,
            Err(error) => Err(error),
        },
        Verb::Workspace(command) => return workspaces::run(paths, command).await,
        Verb::Enter { workspace, new } => workspaces::enter(paths, workspace.as_deref(), new).await,
        Verb::Term(command) => return terminals::run(paths, command).await,
        Verb::Ticket {
            key,
            words,
            project,
            yes,
            another,
            bg,
        } => {
            let asked = tickets::Asked {
                key,
                words,
                project,
                yes,
                another,
                bg,
            };
            tickets::run(paths, asked).await
        }
        Verb::Machine(command) => crate::ssh::machine(paths, command),
        Verb::Bridge => crate::ssh::bridge(paths).await,
        Verb::Ssh {
            machine,
            install,
            no_install,
            check,
            install_claude,
            dir,
            words,
        } => {
            crate::ssh::run(
                paths,
                &crate::ssh::Asked {
                    machine,
                    install,
                    no_install,
                    check,
                    install_claude,
                    dir,
                    words,
                },
            )
            .await
        }
        Verb::Rename { session, name } => terminals::rename(paths, &session, &name).await,
        Verb::Close { session, yes } => terminals::close(paths, &session, yes).await,
        Verb::Init { shell } => return Then::Exit(init::run(paths, shell).await),
        Verb::Send {
            session,
            text,
            no_enter,
            force,
        } => send::send(paths, &session, &text, !no_enter, force).await,
        Verb::Prompt {
            session,
            text,
            timeout,
            force,
        } => send::prompt(paths, &session, &text, timeout, force).await,
        Verb::Wait {
            sessions,
            any,
            until,
            timeout,
            for_text,
        } => match (for_text, sessions.as_slice()) {
            (Some(text), [session]) => control::wait_for_text(paths, session, &text, timeout).await,
            (Some(_), _) => Err("--for waits on one session".into()),
            (None, _) => wait(paths, &sessions, any, until, timeout).await,
        },
        Verb::Keys { session, keys } => control::keys(paths, &session, &keys).await,
        Verb::Move { session, to } => control::move_terminal(paths, &session, &to).await,
        Verb::Logs {
            session,
            reply,
            follow,
            lines,
        } => logs::run(paths, &session, reply, follow, lines).await,
        Verb::Stop { session } => stop(paths, &session).await,
    };
    Then::Exit(ran.unwrap_or_else(crate::fail))
}

/// The session the person's command runs in, where it runs in one of
/// Muster's (`MUSTER_SESSION`): the daemon tells an agent's from a shell's
/// and holds an agent to the person's limits (`PRD-01`).
pub(crate) fn started_by() -> Option<SessionId> {
    // A session of this machine's is none of another's.
    if crate::ssh::remote().is_some() {
        return None;
    }
    std::env::var("MUSTER_SESSION").ok()?.parse().ok()
}

/// Each session's status, as `snapshot` says.
pub(crate) fn statuses(snapshot: &Snapshot) -> HashMap<SessionId, SessionStatus> {
    snapshot
        .sessions
        .iter()
        .map(|session| (session.id, session.status))
        .collect()
}

/// Says how a wait ended: how each session rested on standard error, and
/// the last answer of each that finished on standard output, so a script
/// can keep it.
pub(crate) async fn tell(
    connection: &mut Connection,
    snapshot: &Snapshot,
    waited: &Waited,
) -> Result<(), Box<dyn std::error::Error>> {
    let name = |id: SessionId| {
        snapshot
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map_or_else(|| targets::short(id), |session| shown(&session.title))
    };
    match waited {
        Waited::TimedOut => eprintln!("muster: out of time; they work on"),
        Waited::Stalled(id) => eprintln!(
            "muster: {} did not begin on what it was sent within {} seconds",
            name(*id),
            BEGIN_WITHIN.as_secs()
        ),
        Waited::Rested(rests) => {
            for &(id, rest) in rests {
                eprintln!("{} ({}) {}.", name(id), targets::short(id), rest.words());
                if rest == wait::Rest::Finished
                    && let Some(answer) = start::last_reply(connection, id).await?
                {
                    // An agent's words, but no control character reaches
                    // the terminal raw.
                    println!("{}", muster_core::text::multi_line(&answer));
                }
            }
        }
    }
    Ok(())
}

async fn attach(paths: &Paths, session: &str) -> Result<SessionId, Box<dyn std::error::Error>> {
    let snapshot = commands::connect(&paths.socket())
        .await?
        .subscribe()
        .await?;
    Ok(targets::find_session(&snapshot, session)?.id)
}

async fn wait(
    paths: &Paths,
    wanted: &[String],
    any: bool,
    until: Vec<Until>,
    timeout: Option<Duration>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let sessions = wanted
        .iter()
        .map(|wanted| targets::find_session(&snapshot, wanted).map(|session| session.id))
        .collect::<Result<Vec<_>, _>>()?;
    let waiting = Waiting {
        until: if until.is_empty() {
            wait::ANY_STOP.to_vec()
        } else {
            until
        },
        any,
        timeout,
        begin_within: None,
    };
    let waited = wait::follow(&mut connection, &sessions, statuses(&snapshot), &waiting).await?;
    tell(&mut connection, &snapshot, &waited).await?;
    Ok(waited.code())
}

async fn stop(paths: &Paths, session: &str) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let session = targets::find_session(&snapshot, session)?;
    connection
        .command(Command::StopSession {
            session: session.id,
        })
        .await?;
    println!(
        "Stopping {} ({}).",
        shown(&session.title),
        targets::short(session.id)
    );
    Ok(ExitCode::SUCCESS)
}
