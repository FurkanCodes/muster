//! The subcommands that talk to a running daemon.

use std::path::Path;
use std::process::ExitCode;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{Project, SessionId, SessionStatus, WaitReason, Workspace, WorkspaceKind};

const CLIENT: &str = concat!("muster ", env!("CARGO_PKG_VERSION"));

type Outcome = Result<(), Box<dyn std::error::Error>>;

async fn connect(socket: &Path) -> Result<Connection, ClientError> {
    Connection::connect(socket, CLIENT).await
}

pub(crate) async fn status(socket: &Path) -> ExitCode {
    let mut connection = match connect(socket).await {
        Ok(connection) => connection,
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running. Start it with `muster daemon`.");
            return ExitCode::FAILURE;
        }
        Err(error) => return crate::fail(error),
    };
    let snapshot = match connection.subscribe().await {
        Ok(snapshot) => snapshot,
        Err(error) => return crate::fail(error),
    };
    let welcome = connection.welcome();
    println!(
        "The daemon is running: muster {} (pid {}, protocol {}).",
        welcome.daemon_version, welcome.daemon_pid, welcome.version
    );
    println!(
        "{}, {}, {}.",
        count(snapshot.projects.len(), "project"),
        count(snapshot.workspaces.len(), "worktree"),
        count(snapshot.sessions.len(), "session")
    );
    for project in &snapshot.projects {
        println!(
            "\n{}  {}  ({})",
            project.name,
            project.root.display(),
            project.id
        );
        for workspace in snapshot
            .workspaces
            .iter()
            .filter(|w| w.project == project.id)
        {
            let WorkspaceKind::Worktree { path, branch } = &workspace.kind;
            println!("  {branch}  {}  ({})", path.display(), workspace.id);
            for session in snapshot
                .sessions
                .iter()
                .filter(|s| s.workspace == workspace.id)
            {
                let mark = if session.unseen { "•" } else { " " };
                println!(
                    "   {mark} {}  {}  {}",
                    session.id,
                    session.title,
                    describe(session.status)
                );
            }
        }
    }
    ExitCode::SUCCESS
}

pub(crate) async fn add_project(socket: &Path, path: &Path, name: Option<String>) -> Outcome {
    // The daemon runs elsewhere, so it needs a path that does not depend on
    // this shell's working directory.
    let root = std::path::absolute(path)?;
    let reply = connect(socket)
        .await?
        .command(Command::AddProject { root, name })
        .await?;
    let Reply::ProjectAdded { id } = reply else {
        return Err(unexpected(&reply));
    };
    println!("Added {} as {id}.", path.display());
    Ok(())
}

pub(crate) async fn add_worktree(socket: &Path, project: &str, branch: Option<String>) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?;
    let reply = connection
        .command(Command::CreateWorktree {
            project: project.id,
            branch,
        })
        .await?;
    let Reply::WorktreeCreated { id } = reply else {
        return Err(unexpected(&reply));
    };
    let created = connection.subscribe().await?;
    match created
        .workspaces
        .iter()
        .find(|workspace| workspace.id == id)
    {
        Some(Workspace {
            kind: WorkspaceKind::Worktree { path, branch },
            ..
        }) => println!("Created {branch} at {} ({id}).", path.display()),
        None => println!("Created {id}."),
    }
    Ok(())
}

pub(crate) async fn remove_worktree(socket: &Path, worktree: &str, force: bool) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let workspace = find_worktree(&snapshot, worktree)?.id;
    connection
        .command(Command::RemoveWorktree { workspace, force })
        .await?;
    println!("Removed {workspace}; its branch is kept.");
    Ok(())
}

pub(crate) async fn start_session(
    socket: &Path,
    worktree: &str,
    harness: String,
    title: Option<String>,
    prompt: Option<String>,
) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let workspace = find_worktree(&snapshot, worktree)?.id;
    let reply = connection
        .command(Command::StartSession {
            workspace,
            harness,
            title,
            size: TermSize::default(),
            prompt,
        })
        .await?;
    let Reply::SessionStarted { id } = reply else {
        return Err(unexpected(&reply));
    };
    println!("Started {id}.");
    Ok(())
}

pub(crate) async fn stop_session(socket: &Path, session: &str) -> Outcome {
    let session: SessionId = session.parse()?;
    connect(socket)
        .await?
        .command(Command::StopSession { session })
        .await?;
    println!("Stopping {session}.");
    Ok(())
}

/// A project by id, name or path.
fn find_project<'a>(snapshot: &'a Snapshot, wanted: &str) -> Result<&'a Project, String> {
    let path = std::path::absolute(wanted)
        .ok()
        .and_then(|path| path.canonicalize().ok());
    one(
        snapshot.projects.iter().filter(|project| {
            project.id.to_string() == wanted
                || project.name == wanted
                || path.as_ref().is_some_and(|path| *path == project.root)
        }),
        "project",
        wanted,
    )
}

/// A worktree by id or branch.
fn find_worktree<'a>(snapshot: &'a Snapshot, wanted: &str) -> Result<&'a Workspace, String> {
    one(
        snapshot.workspaces.iter().filter(|workspace| {
            let WorkspaceKind::Worktree { branch, .. } = &workspace.kind;
            workspace.id.to_string() == wanted || branch == wanted
        }),
        "worktree",
        wanted,
    )
}

fn one<'a, T>(
    mut matches: impl Iterator<Item = &'a T>,
    kind: &str,
    wanted: &str,
) -> Result<&'a T, String> {
    match (matches.next(), matches.next()) {
        (Some(found), None) => Ok(found),
        (None, _) => Err(format!("no {kind} matches {wanted:?}")),
        (Some(_), Some(_)) => Err(format!("{wanted:?} matches several; use the {kind}'s id")),
    }
}

fn describe(status: SessionStatus) -> String {
    match status {
        SessionStatus::Fresh => "fresh".to_owned(),
        SessionStatus::Running => "running".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        } => "needs you: permission".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        } => "needs you: question".to_owned(),
        SessionStatus::Finished => "finished".to_owned(),
        SessionStatus::Exited { success: true } => "exited".to_owned(),
        SessionStatus::Exited { success: false } => "exited with an error".to_owned(),
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

fn unexpected(reply: &Reply) -> Box<dyn std::error::Error> {
    format!("unexpected reply {reply:?}").into()
}
