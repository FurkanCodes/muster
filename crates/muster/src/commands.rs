//! The subcommands that talk to a running daemon.

use std::path::Path;
use std::process::ExitCode;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{Project, SessionId, SessionStatus, WaitReason, Workspace};

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
        count(
            snapshot
                .workspaces
                .iter()
                .filter(|workspace| !workspace.kind.is_checkout())
                .count(),
            "worktree"
        ),
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
            let place = if workspace.kind.is_checkout() {
                "  (the project's checkout)"
            } else {
                ""
            };
            println!(
                "  {}  {}  ({}){place}",
                workspace.kind.branch(),
                workspace.kind.path().display(),
                workspace.id
            );
            for session in snapshot
                .sessions
                .iter()
                .filter(|s| s.workspace == workspace.id)
            {
                let mark = if session.unseen { "•" } else { " " };
                let usage = snapshot
                    .resources
                    .as_ref()
                    .and_then(|report| report.sessions.iter().find(|u| u.session == session.id))
                    .map(|usage| {
                        format!(
                            "  {} MB, {}% CPU",
                            usage.memory_bytes.div_ceil(1_000_000),
                            usage.cpu_permille.div_ceil(10)
                        )
                    })
                    .unwrap_or_default();
                println!(
                    "   {mark} {}  {}  {}{usage}",
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

pub(crate) async fn remove_project(socket: &Path, project: &str) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let found = find_project(&snapshot, project)?;
    let (id, name) = (found.id, found.name.clone());
    connection
        .command(Command::RemoveProject { project: id })
        .await?;
    println!("Muster no longer tracks {name}; its repository is untouched.");
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
        Some(workspace) => println!(
            "Created {} at {} ({id}).",
            workspace.kind.branch(),
            workspace.kind.path().display()
        ),
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

pub(crate) async fn restart_session(socket: &Path, session: &str) -> Outcome {
    let session: SessionId = session.parse()?;
    connect(socket)
        .await?
        .command(Command::RestartSession {
            session,
            size: TermSize::default(),
        })
        .await?;
    println!("Restarted {session}.");
    Ok(())
}

pub(crate) async fn remove_session(socket: &Path, session: &str) -> Outcome {
    let session: SessionId = session.parse()?;
    connect(socket)
        .await?
        .command(Command::RemoveSession { session })
        .await?;
    println!("Removed {session}.");
    Ok(())
}

/// Stops whichever daemon answers on `socket`, waiting until it has gone.
pub(crate) async fn stop_daemon(socket: &Path) -> ExitCode {
    let pid = match muster_client::daemon_pid(socket, CLIENT).await {
        Ok(pid) => pid,
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running.");
            return ExitCode::SUCCESS;
        }
        Err(error) => return crate::fail(error),
    };
    let Ok(raw) = i32::try_from(pid) else {
        return crate::fail(format!("the daemon reported an impossible pid {pid}"));
    };
    let process = nix::unistd::Pid::from_raw(raw);
    if let Err(error) = nix::sys::signal::kill(process, nix::sys::signal::Signal::SIGTERM) {
        return crate::fail(format!("could not stop the daemon (pid {pid}): {error}"));
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    // Gone once it no longer answers; it removes its socket as it stops.
    while muster_client::daemon_pid(socket, CLIENT).await.is_ok() {
        if std::time::Instant::now() >= deadline {
            return crate::fail(format!(
                "the daemon (pid {pid}) is still running after 10 seconds; `kill -9 {pid}` ends it"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    println!("Stopped the daemon (pid {pid}).");
    ExitCode::SUCCESS
}

pub(crate) async fn list_issues(socket: &Path, project: &str) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?.id;
    let reply = connection.command(Command::ListIssues { project }).await?;
    let Reply::Issues { issues } = reply else {
        return Err(unexpected(&reply));
    };
    if issues.is_empty() {
        println!("No open issues.");
    }
    for issue in issues {
        let labels = if issue.labels.is_empty() {
            String::new()
        } else {
            format!("  [{}]", issue.labels.join(", "))
        };
        println!("#{:<6} {}{labels}", issue.number, issue.title);
    }
    Ok(())
}

pub(crate) async fn start_issue(
    socket: &Path,
    project: &str,
    number: u64,
    harness: String,
) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?.id;
    let reply = connection
        .command(Command::StartFromIssue {
            project,
            number,
            harness,
            size: TermSize::default(),
        })
        .await?;
    let Reply::IssueStarted { workspace, session } = reply else {
        return Err(unexpected(&reply));
    };
    println!("Started {session} for issue #{number} in {workspace}.");
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

/// A workspace by id or branch, or a project's own checkout by the
/// project's name.
fn find_worktree<'a>(snapshot: &'a Snapshot, wanted: &str) -> Result<&'a Workspace, String> {
    let named_project = |workspace: &Workspace| {
        workspace.kind.is_checkout()
            && snapshot
                .projects
                .iter()
                .any(|project| project.id == workspace.project && project.name == wanted)
    };
    one(
        snapshot.workspaces.iter().filter(|workspace| {
            workspace.id.to_string() == wanted
                || workspace.kind.branch() == wanted
                || named_project(workspace)
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
