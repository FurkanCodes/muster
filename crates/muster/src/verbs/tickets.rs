//! Tickets from a shell (`PRODUCT.md` §6b), in the person's words:
//! `muster list tickets`, `muster ticket 42` reads one, `muster ticket 42
//! claude` puts an agent on it (in the workspace the person is in, which
//! becomes the ticket's), `muster ticket 42 assign [WS]` makes a workspace
//! the ticket's, `open` shows it in the browser and `close` closes it after
//! asking. Which tickets are worked on is the person's: run inside an
//! agent's session, these refuse (`PRD-01`, `PRD-07`).

use std::process::ExitCode;

use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{Paths, ProjectId, TicketKey, Workspace};

use super::targets::{self, workspace_name};
use crate::commands::{self, count, describe, shown, unexpected};

/// A ticket's key as written on the command line.
pub(crate) fn ticket_key(text: &str) -> Result<TicketKey, String> {
    TicketKey::parse(text)
        .ok_or_else(|| format!("{text:?} is not a ticket: write `42`, `#42` or `APP-7`"))
}

/// What `muster ticket KEY …` was asked.
#[derive(Debug)]
pub(crate) struct Asked {
    pub(crate) key: TicketKey,
    /// `open`, `close`, `assign [WS]`, `unassign`, an agent's name, or
    /// nothing to read it.
    pub(crate) words: Vec<String>,
    pub(crate) project: Option<String>,
    pub(crate) yes: bool,
    pub(crate) another: bool,
    pub(crate) bg: bool,
}

pub(crate) async fn run(
    paths: &Paths,
    asked: Asked,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let words: Vec<&str> = asked.words.iter().map(String::as_str).collect();
    match words.as_slice() {
        [] => read(paths, &asked).await,
        ["open"] => open(paths, &asked).await,
        ["close"] => close(paths, &asked).await,
        ["assign"] => assign(paths, &asked, None).await,
        ["assign", workspace] => assign(paths, &asked, Some(workspace)).await,
        ["unassign"] => unassign(paths, &asked).await,
        [harness] => start(paths, &asked, harness).await,
        _ => Err(format!(
            "`muster ticket {}` takes one of: open, close, assign [WORKSPACE], unassign, or an \
             agent's name (`claude`)",
            asked.key.shown()
        )
        .into()),
    }
}

/// The project a ticket command is about, and the workspace the person is
/// in.
fn place<'a>(
    snapshot: &'a Snapshot,
    named: Option<&str>,
) -> Result<(ProjectId, Option<&'a Workspace>), String> {
    let folder = std::env::current_dir().unwrap_or_default();
    let here = targets::current_workspace(snapshot, super::started_by(), &folder);
    if let Some(named) = named {
        let project = commands::find_project(snapshot, named)?.id;
        return Ok((project, here.filter(|w| w.project == project)));
    }
    let here = here.ok_or(
        "you are in none of Muster's projects: name one with --project, or add this one with \
         `muster project add .`",
    )?;
    Ok((here.project, Some(here)))
}

/// The worktree a ticket is worked on in, if one is.
fn ticket_workspace<'a>(
    snapshot: &'a Snapshot,
    project: ProjectId,
    key: &TicketKey,
) -> Option<&'a Workspace> {
    snapshot.workspaces.iter().find(|workspace| {
        workspace.project == project
            && workspace.kind.is_worktree()
            && workspace.ticket.as_ref() == Some(key)
    })
}

/// `muster list tickets`: the project's open tickets, with the agents on
/// each.
pub(crate) async fn list(
    paths: &Paths,
    project: Option<&str>,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let snapshot = commands::connect(&paths.socket())
        .await?
        .subscribe()
        .await?;
    let (project, _) = place(&snapshot, project)?;
    commands::list_issues(&paths.socket(), &project.to_string(), json).await?;
    Ok(ExitCode::SUCCESS)
}

async fn read(paths: &Paths, asked: &Asked) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let (project, _) = place(&snapshot, asked.project.as_deref())?;
    let reply = connection
        .command(Command::ViewIssue {
            project,
            key: asked.key.clone(),
        })
        .await?;
    let Reply::IssueViewed {
        issue,
        body,
        comments,
    } = reply
    else {
        return Err(unexpected(&reply));
    };
    println!("{}  {}", issue.key.shown(), shown(&issue.title));
    let mut facts = Vec::new();
    if let Some(author) = &issue.author {
        facts.push(format!("by {}", shown(author)));
    }
    if !issue.labels.is_empty() {
        facts.push(shown(&issue.labels.join(", ")));
    }
    facts.push(shown(&issue.url));
    println!("{}", facts.join(" · "));
    match ticket_workspace(&snapshot, project, &issue.key) {
        Some(workspace) => {
            let sessions = targets::numbered(&snapshot, workspace.id);
            let agents: Vec<String> = sessions
                .iter()
                .filter(|session| session.kind().is_agent())
                .map(|session| describe(session.status))
                .collect();
            println!(
                "Worked on in {}: {}{}",
                shown(&workspace_name(&snapshot, workspace)),
                count(agents.len(), "agent"),
                if agents.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", agents.join(", "))
                }
            );
        }
        None => println!(
            "No workspace has it: `muster ticket {} claude` puts an agent on it.",
            issue.key.shown()
        ),
    }
    let text = muster_core::text::multi_line(&body);
    if !text.trim().is_empty() {
        println!("\n{}", text.trim_end());
    }
    for comment in comments {
        let who = comment.author.as_deref().map_or("someone", |who| who);
        let now = crate::runs::now();
        let when = comment
            .at
            .map(|at| {
                format!(
                    ", {} ago",
                    muster_core::time::span(muster_core::time::between(at, now))
                )
            })
            .unwrap_or_default();
        println!("\n── {}{when} ──", shown(who));
        println!(
            "{}",
            muster_core::text::multi_line(&comment.body).trim_end()
        );
    }
    Ok(ExitCode::SUCCESS)
}

async fn open(paths: &Paths, asked: &Asked) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let (project, _) = place(&snapshot, asked.project.as_deref())?;
    let reply = connection
        .command(Command::ViewIssue {
            project,
            key: asked.key.clone(),
        })
        .await?;
    let Reply::IssueViewed { issue, .. } = reply else {
        return Err(unexpected(&reply));
    };
    let opened = crate::person::open_in_browser(&issue.url)?;
    if !opened {
        return Err(format!("could not open {}", shown(&issue.url)).into());
    }
    Ok(ExitCode::SUCCESS)
}

async fn close(paths: &Paths, asked: &Asked) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    commands::no_agent_starts_one(&snapshot, "close tickets")?;
    let (project, _) = place(&snapshot, asked.project.as_deref())?;
    let shown_key = asked.key.shown();
    let question = format!("Close {shown_key} in its tracker? Everyone watching it is told.");
    if !asked.yes && !crate::person::agrees(&question)? {
        return Err("nothing was closed (--yes closes it without asking)".into());
    }
    connection
        .command(Command::CloseIssue {
            project,
            key: asked.key.clone(),
        })
        .await?;
    match ticket_workspace(&snapshot, project, &asked.key) {
        Some(workspace) => println!(
            "Closed {shown_key}. Its workspace stays: `muster workspace rm {}` removes it, \
             keeping the branch.",
            shown(&workspace_name(&snapshot, workspace))
        ),
        None => println!("Closed {shown_key}."),
    }
    Ok(ExitCode::SUCCESS)
}

async fn assign(
    paths: &Paths,
    asked: &Asked,
    named: Option<&str>,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    commands::no_agent_starts_one(&snapshot, "assign tickets")?;
    let (project, here) = place(&snapshot, asked.project.as_deref())?;
    let workspace = match named {
        Some(named) => targets::find_workspace(&snapshot, named, Some(project))?,
        None => here
            .filter(|workspace| workspace.kind.is_worktree())
            .ok_or("you are in no worktree: name one (`muster ticket 42 assign fix-login`)")?,
    };
    connection
        .command(Command::AssignTicket {
            workspace: workspace.id,
            key: Some(asked.key.clone()),
        })
        .await?;
    println!(
        "{} is worked on in {} now: its agents are the ticket's.",
        asked.key.shown(),
        shown(&workspace_name(&snapshot, workspace))
    );
    Ok(ExitCode::SUCCESS)
}

async fn unassign(paths: &Paths, asked: &Asked) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    commands::no_agent_starts_one(&snapshot, "assign tickets")?;
    let (project, _) = place(&snapshot, asked.project.as_deref())?;
    let workspace = ticket_workspace(&snapshot, project, &asked.key)
        .ok_or_else(|| format!("no workspace has {}", asked.key.shown()))?;
    connection
        .command(Command::AssignTicket {
            workspace: workspace.id,
            key: None,
        })
        .await?;
    println!(
        "{} is no longer {}'s; the workspace and its agents stay.",
        asked.key.shown(),
        shown(&workspace_name(&snapshot, workspace))
    );
    Ok(ExitCode::SUCCESS)
}

/// `muster ticket 42 claude`: an agent on the ticket, with it as its first
/// prompt: in the workspace the person is in, a worktree, which becomes the
/// ticket's; else in the ticket's own, found or made.
async fn start(
    paths: &Paths,
    asked: &Asked,
    harness: &str,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = crate::daemon_start::connect(paths).await?;
    let snapshot = connection.subscribe().await?;
    commands::no_agent_starts_one(&snapshot, "start agents on tickets")?;
    let (project, here) = place(&snapshot, asked.project.as_deref())?;
    let theirs = ticket_workspace(&snapshot, project, &asked.key);
    if !asked.another
        && let Some(here) = here.filter(|workspace| workspace.kind.is_worktree())
        && theirs.is_none()
    {
        if let Some(other) = &here.ticket {
            return Err(format!(
                "{} is {}'s: `--another` puts the agent in a worktree of its own",
                shown(&workspace_name(&snapshot, here)),
                other.shown()
            )
            .into());
        }
        connection
            .command(Command::AssignTicket {
                workspace: here.id,
                key: Some(asked.key.clone()),
            })
            .await?;
    }
    let size = crossterm::terminal::size()
        .map(|(cols, rows)| TermSize { cols, rows })
        .unwrap_or_default();
    let reply = connection
        .command(Command::StartFromIssue {
            project,
            key: asked.key.clone(),
            harness: harness.to_owned(),
            size,
            another: asked.another,
            on: None,
            brief: None,
        })
        .await?;
    let Reply::IssueStarted { workspace, session } = reply else {
        return Err(unexpected(&reply));
    };
    let in_terminal = std::env::var_os("MUSTER_TERMINAL").is_some();
    if in_terminal && !asked.bg {
        drop(connection);
        return super::tab::show(paths, session).await;
    }
    let snapshot = connection.subscribe().await?;
    let place = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .map(|found| workspace_name(&snapshot, found))
        .unwrap_or_default();
    let number = snapshot
        .sessions
        .iter()
        .find(|found| found.id == session)
        .and_then(|found| targets::number_of(&snapshot, found))
        .unwrap_or_default();
    println!(
        "Started {} on {} in {} ({number}). `muster attach {}/{number}` shows it.",
        shown(harness),
        asked.key.shown(),
        shown(&place),
        shown(&place)
    );
    Ok(ExitCode::SUCCESS)
}
