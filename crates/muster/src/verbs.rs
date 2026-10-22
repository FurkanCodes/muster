//! The verbs that drive agents from a shell (`PRODUCT.md` §6b): start one
//! on a prompt and get the shell back (`muster claude "…"`), list them, send
//! one a prompt, wait for them, read what one shows, and open one.

use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use muster_client::Connection;
use muster_core::protocol::{Command, Snapshot};
use muster_core::{Paths, SessionId, SessionStatus};

mod list;
mod logs;
mod send;
pub(crate) mod start;
pub(crate) mod targets;
pub(crate) mod wait;

use self::wait::{Until, Waited, Waiting};
use crate::commands::{self, shown};

/// How long an agent sent a prompt has to be seen at work on it before a
/// wait calls it stalled.
const BEGIN_WITHIN: Duration = Duration::from_secs(10);

#[derive(Debug, Subcommand)]
pub(crate) enum Verb {
    /// List the sessions of the project you are in (or every one, with
    /// --all, or outside a project): each one's id, name, branch and how it
    /// is doing.
    #[command(visible_alias = "ps")]
    Ls {
        /// Every project's.
        #[arg(long)]
        all: bool,
        /// Print them as one JSON document, for scripts.
        #[arg(long)]
        json: bool,
    },
    /// Open SESSION (its id, name or branch) in the terminal UI, with your
    /// keys in it. Leaving the UI leaves it running.
    Attach { session: String },
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

/// What `muster` does once a verb has run.
pub(crate) enum Then {
    Exit(ExitCode),
    /// Open the terminal UI on this session.
    Open(SessionId),
}

pub(crate) async fn run(paths: &Paths, verb: Verb) -> Then {
    let ran = match verb {
        Verb::Ls { all, json } => list::run(paths, all, json).await,
        Verb::Attach { session } => match attach(paths, &session).await {
            Ok(id) => return Then::Open(id),
            Err(error) => Err(error),
        },
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
        } => wait(paths, &sessions, any, until, timeout).await,
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
