//! Workspaces from a shell (`PRODUCT.md` §6b): `muster workspace add`
//! makes a worktree of the project the person stands in, `muster list
//! workspaces` lists them with what works in each, `rm` removes one, and `muster enter` takes the
//! tab into a workspace's terminal, from where what the person starts
//! belongs to it.

use std::process::ExitCode;
use std::time::Duration;

use clap::Subcommand;
use muster_client::Connection;
use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{Paths, ProjectId, Session, SessionId, Workspace, WorkspaceId};
use serde::Serialize;

use super::targets::{self, numbered, workspace_name};
use crate::commands::{self, shown, unexpected};

/// How long `muster workspace rm` waits for the sessions it stopped.
const STOPPING: Duration = Duration::from_secs(10);

#[derive(Debug, Subcommand)]
pub(crate) enum WorkspaceCommand {
    /// Make a workspace: a worktree of the project you are in (or
    /// --project's), on a new branch named from NAME (`muster/NAME`), or
    /// --branch.
    Add {
        name: Option<String>,
        /// The project (its name, id or folder), where you are not in one.
        #[arg(short, long)]
        project: Option<String>,
        /// The branch to check out, made from the newest of the branch it
        /// will merge into where it is not there yet.
        #[arg(long)]
        branch: Option<String>,
        /// Go into it straight away, as `muster enter` does.
        #[arg(long)]
        enter: bool,
    },
    /// Remove workspace WS: its sessions stopped, after asking, and its
    /// worktree; its branch is kept.
    #[command(visible_alias = "remove")]
    Rm {
        workspace: String,
        /// Stop its sessions without asking.
        #[arg(short, long)]
        yes: bool,
        /// Remove it even with changes not committed, which are lost.
        #[arg(long)]
        force: bool,
    },
}

pub(crate) async fn run(paths: &Paths, command: WorkspaceCommand) -> super::Then {
    let ran = match command {
        WorkspaceCommand::Add {
            name,
            project,
            branch,
            enter,
        } => add(paths, name, project, branch, enter).await,
        WorkspaceCommand::Rm {
            workspace,
            yes,
            force,
        } => remove(paths, &workspace, yes, force).await,
    };
    super::Then::Exit(ran.unwrap_or_else(crate::fail))
}

/// The project a command is about: the one named, else the one the person
/// is in.
fn project_of(snapshot: &Snapshot, named: Option<&str>) -> Result<ProjectId, String> {
    if let Some(named) = named {
        return Ok(commands::find_project(snapshot, named)?.id);
    }
    let folder = std::env::current_dir().unwrap_or_default();
    targets::current_workspace(snapshot, super::started_by(), &folder)
        .map(|workspace| workspace.project)
        .ok_or_else(|| {
            "you are in none of Muster's projects: name one with --project, or add this one \
             with `muster project add .`"
                .to_owned()
        })
}

async fn add(
    paths: &Paths,
    name: Option<String>,
    project: Option<String>,
    branch: Option<String>,
    enter: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = crate::daemon_start::connect(paths).await?;
    let snapshot = connection.subscribe().await?;
    let project = project_of(&snapshot, project.as_deref())?;
    if targets::checkout_of(&snapshot, project).is_some_and(|checkout| !checkout.kind.in_git()) {
        return Err(
            "the project is a folder, not a git repository: the folder is its one \
                    workspace (`muster enter` goes in)"
                .into(),
        );
    }
    let named_for = name
        .filter(|_| branch.is_none())
        .filter(|name| !name.trim().is_empty());
    let reply = connection
        .command(Command::CreateWorktree {
            project,
            branch,
            on: None,
            named_for,
        })
        .await?;
    let Reply::WorktreeCreated { id } = reply else {
        return Err(unexpected(&reply));
    };
    let snapshot = connection.subscribe().await?;
    let made = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
        .ok_or("the new workspace went at once")?;
    let name = workspace_name(&snapshot, made);
    let path = made
        .kind
        .host_path()
        .map_or_else(String::new, |path| path.display().to_string());
    if enter {
        drop(connection);
        return go_in(paths, id, false).await;
    }
    println!(
        "Made workspace {} on {} at {}. `muster enter {}` goes in.",
        shown(&name),
        shown(made.kind.branch()),
        shown(&path),
        shown(&name)
    );
    Ok(ExitCode::SUCCESS)
}

/// One workspace, for `--json`.
#[derive(Debug, Serialize)]
struct WorkspaceEntry {
    id: String,
    name: String,
    project: String,
    kind: &'static str,
    branch: Option<String>,
    path: Option<String>,
    ticket: Option<String>,
    sessions: Vec<NumberedEntry>,
}

/// A session under its workspace: as every document gives one, with its
/// number there and its short id.
#[derive(Debug, Serialize)]
struct NumberedEntry {
    number: usize,
    short: String,
    #[serde(flatten)]
    session: crate::json::SessionEntry,
}

#[derive(Debug, Serialize)]
struct WorkspaceList {
    format: u32,
    workspaces: Vec<WorkspaceEntry>,
}

/// `muster list workspaces`: the project's (or with `all` every
/// project's), each with its branch, what its work comes to, its pull
/// request, and its sessions by number.
pub(crate) async fn list(
    paths: &Paths,
    all: bool,
    project: Option<&str>,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let snapshot = commands::connect(&paths.socket())
        .await?
        .subscribe()
        .await?;
    let project = match project {
        Some(wanted) => Some(project_of(&snapshot, Some(wanted))?),
        None if all => None,
        None => project_of(&snapshot, None).ok(),
    };
    let workspaces: Vec<&Workspace> = snapshot
        .workspaces
        .iter()
        .filter(|workspace| project.is_none_or(|project| workspace.project == project))
        .filter(|workspace| workspace.kind.host_path().is_some())
        .collect();
    if json {
        let workspaces = workspaces
            .iter()
            .map(|workspace| entry(&snapshot, workspace))
            .collect();
        crate::json::print(&WorkspaceList {
            format: crate::json::FORMAT,
            workspaces,
        })?;
        return Ok(ExitCode::SUCCESS);
    }
    if workspaces.is_empty() {
        println!("No workspaces. `muster workspace add <name>` makes one.");
        return Ok(ExitCode::SUCCESS);
    }
    let folder = std::env::current_dir().unwrap_or_default();
    let here = targets::current_workspace(&snapshot, super::started_by(), &folder).map(|w| w.id);
    for project in &snapshot.projects {
        let own: Vec<&&Workspace> = workspaces
            .iter()
            .filter(|workspace| workspace.project == project.id)
            .collect();
        if own.is_empty() {
            continue;
        }
        if all || project_of(&snapshot, None).is_err() {
            println!("{}", shown(&project.name));
        }
        for workspace in own {
            println!("{}", line(&snapshot, workspace, here == Some(workspace.id)));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A workspace on one line: `▸` where the person is, its name, its branch,
/// its ticket, what its work comes to, its pull request, and its sessions
/// by number with how each is doing.
pub(crate) fn line(snapshot: &Snapshot, workspace: &Workspace, here: bool) -> String {
    let mark = if here { "▸" } else { " " };
    let name = shown(&workspace_name(snapshot, workspace));
    let branch = if workspace.kind.is_worktree() || !workspace.kind.in_git() {
        String::new()
    } else {
        format!(" ({})", shown(workspace.kind.branch()))
    };
    let mut facts = Vec::new();
    if let Some(ticket) = &workspace.ticket {
        facts.push(ticket.shown());
    }
    if let Some(changes) = workspace
        .changes
        .as_ref()
        .filter(|changes| changes.files > 0)
    {
        facts.push(changes.words());
    }
    if let Some(pull) = &workspace.pull_request {
        facts.push(format!("#{}", pull.number));
    }
    if !workspace.clashes.is_empty() {
        facts.push("meets other work".to_owned());
    }
    let sessions: Vec<String> = numbered(snapshot, workspace.id)
        .iter()
        .enumerate()
        .map(|(i, session)| format!("{}{}", i + 1, mark_of(session)))
        .collect();
    let facts = if facts.is_empty() {
        String::new()
    } else {
        format!("  {}", facts.join("  "))
    };
    let sessions = if sessions.is_empty() {
        String::new()
    } else {
        format!("  {}", sessions.join(" "))
    };
    format!("{mark} {name}{branch}{facts}{sessions}")
}

/// A session's mark, as the lists show how it is doing: `$` a terminal,
/// `●` at work, `!` needs you, `✓` finished, `·` ended or not started.
pub(crate) fn mark_of(session: &Session) -> &'static str {
    use muster_core::SessionStatus;
    if session.kind().is_shell() && !session.status.is_exited() {
        return "$";
    }
    match session.status {
        SessionStatus::Running => "●",
        SessionStatus::NeedsYou { .. } => "!",
        SessionStatus::Finished => "✓",
        SessionStatus::Fresh | SessionStatus::Exited { .. } => "·",
    }
}

fn entry(snapshot: &Snapshot, workspace: &Workspace) -> WorkspaceEntry {
    let project = snapshot
        .projects
        .iter()
        .find(|project| project.id == workspace.project)
        .map_or_else(String::new, |project| project.name.clone());
    WorkspaceEntry {
        id: workspace.id.to_string(),
        name: workspace_name(snapshot, workspace),
        project,
        kind: if workspace.kind.is_worktree() {
            "worktree"
        } else {
            "checkout"
        },
        branch: workspace
            .kind
            .in_git()
            .then(|| workspace.kind.branch().to_owned()),
        path: workspace
            .kind
            .host_path()
            .map(|path| path.display().to_string()),
        ticket: workspace.ticket.as_ref().map(muster_core::TicketKey::shown),
        sessions: numbered(snapshot, workspace.id)
            .iter()
            .enumerate()
            .map(|(i, session)| NumberedEntry {
                number: i + 1,
                short: targets::short(session.id),
                session: crate::json::session_entry(snapshot, session),
            })
            .collect(),
    }
}

async fn remove(
    paths: &Paths,
    wanted: &str,
    yes: bool,
    force: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let folder = std::env::current_dir().unwrap_or_default();
    let prefer = targets::current_workspace(&snapshot, super::started_by(), &folder)
        .map(|workspace| workspace.project);
    let workspace = targets::find_workspace(&snapshot, wanted, prefer)?;
    if !workspace.kind.is_worktree() {
        return Err(
            "only a worktree is removed; a project's own checkout stays, and `muster \
                    project remove` stops tracking the project"
                .into(),
        );
    }
    let (id, name) = (workspace.id, workspace_name(&snapshot, workspace));
    let running: Vec<SessionId> = numbered(&snapshot, id)
        .iter()
        .filter(|session| !session.status.is_exited())
        .map(|session| session.id)
        .collect();
    if !running.is_empty() {
        let question = format!(
            "{} {} running in {}: stop {} and remove it?",
            commands::count(running.len(), "session"),
            if running.len() == 1 { "is" } else { "are" },
            shown(&name),
            if running.len() == 1 { "it" } else { "them" }
        );
        if !yes && !crate::person::agrees(&question)? {
            return Err("nothing was removed".into());
        }
        for session in &running {
            connection
                .command(Command::StopSession { session: *session })
                .await?;
        }
        stopped(&mut connection, &running).await?;
    }
    connection
        .command(Command::RemoveWorktree {
            workspace: id,
            force,
        })
        .await?;
    println!("Removed workspace {}; its branch is kept.", shown(&name));
    Ok(ExitCode::SUCCESS)
}

/// Waits until `sessions` have ended.
pub(crate) async fn stopped(
    connection: &mut Connection,
    sessions: &[SessionId],
) -> Result<(), Box<dyn std::error::Error>> {
    let deadline = crate::instant_now() + STOPPING;
    loop {
        let snapshot = connection.subscribe().await?;
        let still = snapshot
            .sessions
            .iter()
            .any(|session| sessions.contains(&session.id) && !session.status.is_exited());
        if !still {
            return Ok(());
        }
        if crate::instant_now() >= deadline {
            return Err("its sessions are still stopping; try again in a moment".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// `muster enter [WS]`: the tab into workspace WS's terminal, the one used
/// last, or a new one.
pub(crate) async fn enter(
    paths: &Paths,
    wanted: Option<&str>,
    new: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let snapshot = crate::daemon_start::connect(paths)
        .await?
        .subscribe()
        .await?;
    let folder = std::env::current_dir().unwrap_or_default();
    let here = targets::current_workspace(&snapshot, super::started_by(), &folder);
    let workspace = match wanted {
        Some(wanted) => targets::find_workspace(&snapshot, wanted, here.map(|w| w.project))?,
        None => here.ok_or(
            "you are in no workspace: name one (`muster list workspaces`), or make one with \
             `muster workspace add <name>`",
        )?,
    };
    go_in(paths, workspace.id, new).await
}

/// Shows `workspace`'s terminal where the person is: the newest one still
/// running, or, with `new` or where none runs, one started for it.
async fn go_in(
    paths: &Paths,
    workspace: WorkspaceId,
    new: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = commands::connect(&paths.socket()).await?;
    let snapshot = connection.subscribe().await?;
    let running = numbered(&snapshot, workspace)
        .into_iter()
        .rev()
        .find(|session| session.is_terminal() && !session.status.is_exited())
        .map(|session| session.id);
    let terminal = match running.filter(|_| !new) {
        Some(terminal) => terminal,
        None => start_terminal(&mut connection, workspace, None).await?,
    };
    drop(connection);
    super::tab::show(paths, terminal).await
}

/// Starts a terminal (a shell session) in `workspace`; named `title`, as
/// the person names one, which what runs in it never renames.
pub(crate) async fn start_terminal(
    connection: &mut Connection,
    workspace: WorkspaceId,
    title: Option<String>,
) -> Result<SessionId, Box<dyn std::error::Error>> {
    let size = crossterm::terminal::size()
        .map(|(cols, rows)| TermSize { cols, rows })
        .unwrap_or_default();
    let reply = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: title.clone(),
            size,
            prompt: None,
            // Marked as started by the agent the command runs under, if any
            // (`PRD-01`).
            started_by: super::started_by(),
        })
        .await?;
    let Reply::SessionStarted { id } = reply else {
        return Err(unexpected(&reply));
    };
    if let Some(title) = title.filter(|title| !title.trim().is_empty()) {
        connection
            .command(Command::RenameSession { session: id, title })
            .await?;
    }
    Ok(id)
}
