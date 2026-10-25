//! `muster list [workspaces|agents|terminals|sessions|projects|tickets|machines]`: what
//! Muster has, said in the person's words (`PRODUCT.md` §6b). Sessions are
//! listed under their projects and workspaces by number. In a workspace
//! (a worktree, or a Muster terminal's), agents, terminals and sessions are
//! that workspace's; elsewhere in a project, the project's; with `--all`,
//! every project's.

use std::process::ExitCode;

use clap::ValueEnum;
use muster_client::ClientError;
use muster_core::protocol::Snapshot;
use muster_core::spaces::Member;
use muster_core::{Paths, ProjectId, Session, SpaceId, WorkspaceId};
use serde::Serialize;

use super::targets;
use crate::commands::{self, count, describe, shown};

/// What `muster list` lists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Listing {
    /// Worktrees and checkouts, each with its sessions by number.
    #[value(alias = "workspace", alias = "ws")]
    Workspaces,
    /// Coding agents' sessions.
    #[value(alias = "agent")]
    Agents,
    /// Terminals: shells.
    #[value(alias = "terminal")]
    Terminals,
    /// Every session, agents and terminals.
    #[value(alias = "session")]
    Sessions,
    #[value(alias = "project")]
    Projects,
    /// The project's open tickets, with the agents on each.
    #[value(alias = "ticket")]
    Tickets,
    /// Your machines, for `muster ssh`.
    #[value(alias = "machine")]
    Machines,
    /// Your spaces, and what is in each.
    #[value(alias = "space")]
    Spaces,
}

/// Whose a listing is, as asked: every project's, one project's, or one
/// space's; else where the person is.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Only<'a> {
    pub(crate) all: bool,
    pub(crate) project: Option<&'a str>,
    pub(crate) space: Option<&'a str>,
}

/// Whose sessions a listing shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    All,
    Project(ProjectId),
    Workspace(WorkspaceId),
    Space(SpaceId),
}

pub(crate) async fn run(
    paths: &Paths,
    what: Option<Listing>,
    only: Only<'_>,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let Only {
        all,
        project,
        space,
    } = only;
    let what = what.unwrap_or(Listing::Sessions);
    if what == Listing::Workspaces {
        return super::workspaces::list(paths, all, project, json).await;
    }
    if what == Listing::Machines {
        return crate::ssh::list(paths, json);
    }
    if what == Listing::Tickets {
        return super::tickets::list(paths, project, json).await;
    }
    let mut connection = match commands::connect(&paths.socket()).await {
        Ok(connection) => connection,
        // As `muster status` says it, for a script too.
        Err(ClientError::Unreachable { .. }) if json => {
            crate::json::print(&crate::json::NotRunning::new())?;
            return Ok(ExitCode::FAILURE);
        }
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running, so nothing is.");
            return Ok(ExitCode::SUCCESS);
        }
        Err(error) => return Err(error.into()),
    };
    let snapshot = connection.subscribe().await?;
    if what == Listing::Projects {
        return projects(&snapshot, json);
    }
    if what == Listing::Spaces {
        return super::spaces::list(&snapshot, json);
    }
    let scope = match (project, space) {
        (Some(wanted), _) => Scope::Project(commands::find_project(&snapshot, wanted)?.id),
        (None, Some(wanted)) => Scope::Space(super::spaces::find(&snapshot, wanted)?.id),
        (None, None) => scope(&snapshot, all),
    };
    let kept: Vec<&Session> = snapshot
        .sessions
        .iter()
        .filter(|session| !session.kind().is_panel())
        .filter(|session| match what {
            Listing::Agents => session.kind().is_agent(),
            Listing::Terminals => session.is_terminal(),
            _ => true,
        })
        .filter(|session| in_scope(&snapshot, scope, session))
        .collect();
    if json {
        let mut listed = snapshot.clone();
        let ids: Vec<_> = kept.iter().map(|session| session.id).collect();
        listed.sessions.retain(|session| ids.contains(&session.id));
        crate::json::print(&crate::json::SessionList::new(&listed))?;
        return Ok(ExitCode::SUCCESS);
    }
    let noun = match what {
        Listing::Agents => "agents",
        Listing::Terminals => "terminals",
        _ => "sessions",
    };
    if kept.is_empty() {
        println!("No {noun}{}.", where_words(&snapshot, scope));
        return Ok(ExitCode::SUCCESS);
    }
    print_tree(&snapshot, &kept);
    if scope != Scope::All {
        println!(
            "\n{}{}; `muster list {noun} --all` lists every project's.",
            capital(&count(kept.len(), &noun[..noun.len() - 1])),
            where_words(&snapshot, scope)
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// Whose sessions are listed: everything with `all`; the workspace the
/// person works in, where they are in a worktree or a Muster terminal; the
/// project they stand in; else everything.
fn scope(snapshot: &Snapshot, all: bool) -> Scope {
    if all {
        return Scope::All;
    }
    let folder = std::env::current_dir().unwrap_or_default();
    let inside = super::started_by();
    match targets::current_workspace(snapshot, inside, &folder) {
        Some(workspace) if workspace.kind.is_worktree() || inside.is_some() => {
            Scope::Workspace(workspace.id)
        }
        Some(workspace) => Scope::Project(workspace.project),
        None => Scope::All,
    }
}

fn in_scope(snapshot: &Snapshot, scope: Scope, session: &Session) -> bool {
    match scope {
        Scope::All => true,
        Scope::Workspace(workspace) => session.is_in(workspace),
        Scope::Project(project) => snapshot
            .workspaces
            .iter()
            .any(|workspace| session.is_in(workspace.id) && workspace.project == project),
        Scope::Space(id) => snapshot.spaces.iter().any(|space| {
            space.id == id
                && (space.holds(Member::Session(session.id))
                    || snapshot.workspaces.iter().any(|workspace| {
                        session.is_in(workspace.id)
                            && space.holds(Member::Project(workspace.project))
                    }))
        }),
    }
}

/// ` in fix-login`, ` in app`, or nothing for everything.
fn where_words(snapshot: &Snapshot, scope: Scope) -> String {
    match scope {
        Scope::All => String::new(),
        Scope::Workspace(id) => snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == id)
            .map(|workspace| {
                format!(
                    " in {}",
                    shown(&targets::workspace_name(snapshot, workspace))
                )
            })
            .unwrap_or_default(),
        Scope::Project(id) => snapshot
            .projects
            .iter()
            .find(|project| project.id == id)
            .map(|project| format!(" in {}", shown(&project.name)))
            .unwrap_or_default(),
        Scope::Space(id) => snapshot
            .spaces
            .iter()
            .find(|space| space.id == id)
            .map(|space| format!(" in space {}", shown(&space.name)))
            .unwrap_or_default(),
    }
}

fn capital(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// `sessions` under their projects and workspaces, numbered as in their
/// workspace.
fn print_tree(snapshot: &Snapshot, sessions: &[&Session]) {
    let now = crate::runs::now();
    let folder = std::env::current_dir().unwrap_or_default();
    let here = targets::current_workspace(snapshot, super::started_by(), &folder).map(|w| w.id);
    for project in &snapshot.projects {
        let workspaces: Vec<&muster_core::Workspace> = snapshot
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project.id)
            .filter(|workspace| sessions.iter().any(|s| s.is_in(workspace.id)))
            .collect();
        if workspaces.is_empty() {
            continue;
        }
        println!("{}", shown(&project.name));
        for workspace in workspaces {
            println!(
                "{}",
                super::workspaces::line(snapshot, workspace, here == Some(workspace.id))
            );
            for session in targets::numbered(snapshot, workspace.id) {
                if sessions.iter().any(|kept| kept.id == session.id) {
                    let number = targets::number_of(snapshot, session).unwrap_or_default();
                    println!("{}", numbered_line(number, session, now));
                }
            }
        }
    }
    // Terminals in no project (`ARC-33`), by their ids.
    let free: Vec<&&Session> = sessions
        .iter()
        .filter(|session| session.is_free())
        .collect();
    if !free.is_empty() {
        println!("Terminals");
        for (number, session) in (1..).zip(free) {
            println!("{}", numbered_line(number, session, now));
        }
    }
}

/// `session`, numbered under its workspace: its number, its mark, its
/// agent, its name, how it is doing and for how long, its short id, and
/// who started it where an agent did.
fn numbered_line(number: usize, session: &Session, now: muster_core::Timestamp) -> String {
    let unseen = if session.unseen { "•" } else { " " };
    let how = format!(
        "{} {}",
        describe(session.status),
        muster_core::time::span(muster_core::time::between(session.status_since, now))
    );
    let by = session
        .started_by
        .map(|starter| format!("  (started by {})", targets::short(starter)))
        .unwrap_or_default();
    format!(
        "   {unseen}{number:>2} {} {:<8}  {:<32}  {how:<28}  {}{by}",
        super::workspaces::mark_of(session),
        shown(session.shown_harness()),
        shown(&session.title),
        targets::short(session.id),
    )
}

#[derive(Debug, Serialize)]
struct ProjectEntry {
    id: String,
    name: String,
    path: String,
    workspaces: usize,
    sessions: usize,
}

#[derive(Debug, Serialize)]
struct ProjectList {
    format: u32,
    projects: Vec<ProjectEntry>,
}

/// Every project: its name, its folder, and how many workspaces and
/// sessions it has.
fn projects(snapshot: &Snapshot, json: bool) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let entries: Vec<ProjectEntry> = snapshot
        .projects
        .iter()
        .map(|project| {
            let workspaces: Vec<WorkspaceId> = snapshot
                .workspaces
                .iter()
                .filter(|workspace| workspace.project == project.id)
                .map(|workspace| workspace.id)
                .collect();
            ProjectEntry {
                id: project.id.to_string(),
                name: project.name.clone(),
                path: project.root.display().to_string(),
                workspaces: workspaces.len(),
                sessions: snapshot
                    .sessions
                    .iter()
                    .filter(|session| session.workspace.is_some_and(|id| workspaces.contains(&id)))
                    .count(),
            }
        })
        .collect();
    if json {
        crate::json::print(&ProjectList {
            format: crate::json::FORMAT,
            projects: entries,
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    if entries.is_empty() {
        println!("No projects. `muster project add <folder>` adds one.");
    }
    for entry in entries {
        println!(
            "{:<20}  {}  {}, {}",
            shown(&entry.name),
            shown(&entry.path),
            count(entry.workspaces, "workspace"),
            count(entry.sessions, "session")
        );
    }
    Ok(ExitCode::SUCCESS)
}
