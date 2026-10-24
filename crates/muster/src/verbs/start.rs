//! `muster <agent> [PROMPT]`: a coding agent started in the background, in
//! a worktree of its own in the project the person stands in, and the
//! command back at once, so the next can be started straight away. Typed in
//! a workspace's terminal (`muster enter`), it starts in that workspace
//! instead, and takes the terminal's place where the person is.

use std::io::{IsTerminal, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use muster_client::Connection;

use muster_core::protocol::{Command, Reading, Reply, Snapshot, TermSize};
use muster_core::{Paths, SessionId, WorkspaceId};
use serde::Serialize;

use super::targets::{self, Here};
use super::wait::{self, ANY_STOP, Waiting};
use crate::commands::{self, shown, unexpected};

/// What `muster <agent>` takes.
#[derive(Debug, Parser)]
#[command(
    name = "muster <agent>",
    about = "Start a coding agent in the background on PROMPT, in a new worktree of the \
             project you are in, and return: start another straight away, and see them with \
             `muster list`, `muster wait`, `muster logs` or `muster attach`."
)]
struct Args {
    /// The agent: `claude`, `codex`, `gemini`, `opencode`, `copilot`, or
    /// one you added as `[agents.<name>]` in Muster's own settings.
    agent: String,
    /// Its first prompt, in as many words as you like; `-` reads it from
    /// standard input. Without one, the agent waits for you.
    prompt: Vec<String>,
    /// Work in the checkout or worktree you are in, rather than a new
    /// worktree.
    #[arg(long, conflicts_with = "in_worktree")]
    here: bool,
    /// Work in WORKTREE (its id or branch), rather than a new worktree.
    #[arg(long = "in", value_name = "WORKTREE")]
    in_worktree: Option<String>,
    /// The project (its name, id or folder), where you are not in one.
    #[arg(short, long)]
    project: Option<String>,
    /// The new worktree's branch; named from the prompt if not given.
    #[arg(long, conflicts_with_all = ["here", "in_worktree"])]
    branch: Option<String>,
    /// The session's name; its prompt's first line if not given.
    #[arg(long)]
    title: Option<String>,
    /// Wait for it to finish its turn, need you or end, then print its
    /// answer, exiting as `muster wait` does.
    #[arg(long)]
    wait: bool,
    /// In a workspace's terminal, start it and stay, rather than show it
    /// in the terminal's place.
    #[arg(long)]
    bg: bool,
    /// With --wait, give up after this long (30s, 5m, 2h).
    #[arg(long, requires = "wait", value_parser = wait::duration)]
    timeout: Option<std::time::Duration>,
    /// Add the folder you are in as a project without asking, where it is
    /// not one yet.
    #[arg(long)]
    add: bool,
    /// Print what was started as one JSON document, for scripts.
    #[arg(long)]
    json: bool,
}

/// `muster <agent> …`, from the words after `muster`.
pub(crate) async fn run(paths: &Paths, words: Vec<String>) -> ExitCode {
    let args = match Args::try_parse_from(std::iter::once("muster".to_owned()).chain(words)) {
        Ok(args) => args,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(2));
        }
    };
    match start(paths, args).await {
        Ok(code) => code,
        Err(error) => crate::fail(error),
    }
}

/// What was started, for `--json`.
#[derive(Debug, Serialize)]
struct Started {
    format: u32,
    session: String,
    short: String,
    workspace: String,
    project: String,
    branch: Option<String>,
}

async fn start(paths: &Paths, args: Args) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let prompt = prompt(&args.prompt)?;
    let mut connection = crate::daemon_start::connect(paths).await?;
    let snapshot = connection.subscribe().await?;
    let terminal = in_terminal(&snapshot, &args);
    let snapshot = match terminal {
        Some(_) => snapshot,
        None => with_project(&mut connection, snapshot, &args).await?,
    };
    let (workspace, made) = match terminal {
        Some((_, workspace)) => (workspace, false),
        None => workspace_for(&mut connection, &snapshot, &args, prompt.as_deref()).await?,
    };
    let started = connection
        .command(Command::StartSession {
            workspace,
            harness: args.agent.clone(),
            title: args.title.clone(),
            size: TermSize::default(),
            prompt,
            started_by: super::started_by(),
        })
        .await;
    let session = match started {
        Ok(Reply::SessionStarted { id }) => id,
        Ok(reply) => return Err(unexpected(&reply)),
        Err(error) => {
            // A worktree made for this agent alone goes with it.
            if made {
                let _ = connection
                    .command(Command::RemoveWorktree {
                        workspace,
                        force: false,
                    })
                    .await;
            }
            return Err(error.into());
        }
    };
    let snapshot = connection.subscribe().await?;
    if terminal.is_some() && !args.bg && !args.wait && !args.json {
        drop(connection);
        return super::tab::show(paths, session).await;
    }
    report(&snapshot, session, workspace, args.json)?;
    if !args.wait {
        return Ok(ExitCode::SUCCESS);
    }
    let statuses = super::statuses(&snapshot);
    let waiting = Waiting {
        until: ANY_STOP.to_vec(),
        any: false,
        timeout: args.timeout,
        begin_within: None,
    };
    let waited = wait::follow(&mut connection, &[session], statuses, &waiting).await?;
    super::tell(&mut connection, &snapshot, &waited).await?;
    Ok(waited.code())
}

/// The terminal the person typed this in and its workspace, where they
/// typed it in a workspace's terminal (`MUSTER_TERMINAL`) and named no
/// other place: what they start there belongs to that workspace.
fn in_terminal(snapshot: &Snapshot, args: &Args) -> Option<(SessionId, WorkspaceId)> {
    if args.here
        || args.in_worktree.is_some()
        || args.project.is_some()
        || args.branch.is_some()
        || std::env::var_os("MUSTER_TERMINAL").is_none()
    {
        return None;
    }
    let inside = super::started_by()?;
    snapshot
        .sessions
        .iter()
        .find(|session| session.id == inside && session.kind().is_shell())
        .map(|session| (session.id, session.workspace))
}

/// The prompt the words make, read from standard input for `-`; `None`
/// for none.
fn prompt(words: &[String]) -> Result<Option<String>, Box<dyn std::error::Error>> {
    if words == ["-"] {
        let mut text = String::new();
        std::io::stdin().read_to_string(&mut text)?;
        return Ok(Some(text).filter(|text| !text.trim().is_empty()));
    }
    let joined = words.join(" ");
    Ok(Some(joined).filter(|text| !text.trim().is_empty()))
}

/// `snapshot`, with the folder the person stands in added as a project
/// first where it is none yet and they agree (or said `--add`).
async fn with_project(
    connection: &mut Connection,
    snapshot: Snapshot,
    args: &Args,
) -> Result<Snapshot, Box<dyn std::error::Error>> {
    if args.project.is_some() || args.in_worktree.is_some() {
        return Ok(snapshot);
    }
    if crate::ssh::remote().is_some() {
        return Err(
            "on another machine, name where it works: `--in WORKSPACE` or `-p PROJECT` \
                    (`muster --on MACHINE list workspaces`)"
                .into(),
        );
    }
    let cwd = std::env::current_dir()?;
    if targets::here(&snapshot, &cwd).is_some() {
        return Ok(snapshot);
    }
    let root = top_level(&cwd);
    if !args.add && !agrees(&root)? {
        return Err(format!(
            "{} is not one of Muster's projects: add it with `--add` or `muster project add`, \
             or name one with `--project`",
            root.display()
        )
        .into());
    }
    let reply = connection
        .command(Command::AddProject { root, name: None })
        .await?;
    let Reply::ProjectAdded { .. } = reply else {
        return Err(unexpected(&reply));
    };
    Ok(connection.subscribe().await?)
}

/// The repository `folder` is in, at its top, or the folder itself where
/// it is in none.
fn top_level(folder: &Path) -> PathBuf {
    std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(folder)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(|| folder.to_path_buf(), |top| PathBuf::from(top.trim_end()))
}

/// Whether the person, asked on their terminal, adds `root` as a project;
/// never, without a terminal to ask on.
fn agrees(root: &Path) -> std::io::Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("Add {} as a project in Muster? [y/N] ", root.display());
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

/// The workspace the agent works in, and whether it was made for it.
async fn workspace_for(
    connection: &mut Connection,
    snapshot: &Snapshot,
    args: &Args,
    prompt: Option<&str>,
) -> Result<(WorkspaceId, bool), Box<dyn std::error::Error>> {
    if let Some(worktree) = &args.in_worktree {
        return Ok((commands::find_worktree(snapshot, worktree)?.id, false));
    }
    let cwd = std::env::current_dir()?;
    let here = match &args.project {
        Some(project) => {
            let project = commands::find_project(snapshot, project)?;
            Here {
                project,
                workspace: None,
            }
        }
        None => targets::here(snapshot, &cwd).ok_or("you are in none of Muster's projects")?,
    };
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == here.project.id && workspace.kind.is_checkout())
        .ok_or("the project has no checkout")?;
    if args.here {
        return Ok((here.workspace.unwrap_or(checkout).id, false));
    }
    if !checkout.kind.in_git() {
        // A plain folder has nothing to isolate with (`PRD-04`).
        return Ok((checkout.id, false));
    }
    let reply = connection
        .command(Command::CreateWorktree {
            project: here.project.id,
            branch: args.branch.clone(),
            on: None,
            named_for: prompt.map(str::to_owned),
        })
        .await?;
    let Reply::WorktreeCreated { id } = reply else {
        return Err(unexpected(&reply));
    };
    Ok((id, true))
}

/// Says what was started: a line for the person, or a JSON document.
fn report(
    snapshot: &Snapshot,
    session: SessionId,
    workspace: WorkspaceId,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let place = snapshot.workspaces.iter().find(|w| w.id == workspace);
    let branch = place
        .filter(|workspace| workspace.kind.in_git())
        .map(|workspace| workspace.kind.branch().to_owned());
    let short = targets::short(session);
    if json {
        return Ok(crate::json::print(&Started {
            format: crate::json::FORMAT,
            session: session.to_string(),
            short,
            workspace: workspace.to_string(),
            project: place.map(|w| w.project.to_string()).unwrap_or_default(),
            branch,
        })?);
    }
    let title = snapshot
        .sessions
        .iter()
        .find(|found| found.id == session)
        .map_or_else(String::new, |found| shown(&found.title));
    let on = branch.map_or_else(String::new, |branch| format!(" on {}", shown(&branch)));
    println!("Started {title} ({short}){on}. `muster attach {short}` opens it.");
    Ok(())
}

/// Its last answer, for `--wait` where it finished: printed whole on
/// standard output, so a script can keep it.
pub(crate) async fn last_reply(
    connection: &mut Connection,
    session: SessionId,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let reply = connection
        .command(Command::ReadSession {
            session,
            what: Reading::LastReply,
        })
        .await?;
    let Reply::SessionText { text } = reply else {
        return Err(unexpected(&reply));
    };
    Ok(text)
}
