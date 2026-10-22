//! The subcommands that talk to a running daemon.

use std::path::Path;
use std::process::ExitCode;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{Project, SessionId, SessionStatus, WaitReason, Workspace};

pub(crate) const CLIENT: &str = concat!("muster ", env!("CARGO_PKG_VERSION"));

pub(crate) type Outcome = Result<(), Box<dyn std::error::Error>>;

pub(crate) async fn connect(socket: &Path) -> Result<Connection, ClientError> {
    Connection::connect(socket, CLIENT).await
}

/// `muster status`: in words, or with `json` as one JSON document.
pub(crate) async fn status(socket: &Path, json: bool) -> ExitCode {
    let mut connection = match connect(socket).await {
        Ok(connection) => connection,
        Err(ClientError::Unreachable { .. }) => {
            if json {
                if let Err(error) = crate::json::print(&crate::json::NotRunning::new()) {
                    return crate::fail(error);
                }
            } else {
                println!("The daemon is not running. Start it with `muster daemon`.");
            }
            return ExitCode::FAILURE;
        }
        Err(error) => return crate::fail(error),
    };
    let snapshot = match connection.subscribe().await {
        Ok(snapshot) => snapshot,
        Err(error) => return crate::fail(error),
    };
    let welcome = connection.welcome().clone();
    // Counts that fall back as work and clients end; one that only grows
    // is a leak (`ARC-13`, `ARC-14`). Older daemons do not know to answer.
    let health = if welcome.version >= 22 {
        match connection.command(Command::Health).await {
            Ok(Reply::Health(health)) => Some(health),
            _ => None,
        }
    } else {
        None
    };
    if json {
        return match crate::json::print(&crate::json::Status::new(&welcome, health, &snapshot)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => crate::fail(error),
        };
    }
    println!(
        "The daemon is running: muster {} (pid {}, protocol {}).",
        welcome.daemon_version, welcome.daemon_pid, welcome.version
    );
    if let Some(health) = health {
        let count = |n: u64, noun| count(usize::try_from(n).unwrap_or(usize::MAX), noun);
        println!(
            "Inside: {} ({} at work), {}, {}.",
            count(health.tasks, "task"),
            health.work,
            count(health.clients, "client"),
            count(health.terminals, "terminal")
        );
    }
    let going: Vec<&muster_core::Run> = snapshot
        .runs
        .iter()
        .filter(|run| !run.stage.is_over())
        .collect();
    println!(
        "{}, {}, {}, {} going.",
        count(snapshot.projects.len(), "project"),
        count(
            snapshot
                .workspaces
                .iter()
                .filter(|workspace| !workspace.kind.is_checkout())
                .count(),
            "worktree"
        ),
        count(snapshot.sessions.len(), "session"),
        count(going.len(), "run")
    );
    for project in &snapshot.projects {
        print_project(&snapshot, project, &going);
    }
    ExitCode::SUCCESS
}

/// A project as `muster status` prints it: its runs under way, then each
/// workspace with its sessions.
fn print_project(snapshot: &Snapshot, project: &Project, going: &[&muster_core::Run]) {
    println!(
        "\n{}  {}  ({})",
        shown(&project.name),
        shown(&project.root.display().to_string()),
        project.id
    );
    for run in going.iter().filter(|run| run.project == project.id) {
        let reason = run
            .reason
            .as_deref()
            .map(|reason| format!(": {}", shown(reason)))
            .unwrap_or_default();
        println!(
            "  run #{}  {}{reason}  ({})",
            run.ticket.number,
            run.stage.label(),
            run.id
        );
    }
    for workspace in snapshot
        .workspaces
        .iter()
        .filter(|w| w.project == project.id)
    {
        print_workspace(snapshot, workspace);
    }
}

/// A workspace as `muster status` prints it, with its sessions.
fn print_workspace(snapshot: &Snapshot, workspace: &Workspace) {
    let place = match (&workspace.kind, workspace.kind.host_path()) {
        (muster_core::WorkspaceKind::Checkout { .. }, Some(path)) => format!(
            "  {}  ({})  (the project's checkout)",
            shown(&path.display().to_string()),
            workspace.id
        ),
        (_, Some(path)) => format!(
            "  {}  ({})",
            shown(&path.display().to_string()),
            workspace.id
        ),
        (_, None) => {
            let number = workspace
                .kind
                .run()
                .and_then(|run| snapshot.runs.iter().find(|found| found.id == run))
                .map_or_else(String::new, |run| format!(" #{}", run.ticket.number));
            format!("  ({})  (the container of run{number})", workspace.id)
        }
    };
    let branch = if workspace.kind.in_git() {
        shown(workspace.kind.branch())
    } else {
        "(not a git repository)".to_owned()
    };
    println!("  {branch}{place}");
    if let Some(pull_request) = &workspace.pull_request {
        println!("    pull request {}", shown(&pull_request.summary()));
    }
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
        let spend = session
            .spend
            .map(|spend| format!("  {}", spend.text()))
            .unwrap_or_default();
        println!(
            "   {mark} {}  {}  {}{usage}{spend}",
            session.id,
            shown(&session.title),
            describe(session.status)
        );
    }
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
    println!("Added {} as {id}.", shown(&path.display().to_string()));
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
    println!(
        "Muster no longer tracks {}; its folder is untouched.",
        shown(&name)
    );
    Ok(())
}

#[expect(
    clippy::too_many_lines,
    reason = "every setting, in muster.toml's order"
)]
pub(crate) async fn show_config(
    socket: &Path,
    paths: &muster_core::Paths,
    project: &str,
) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let found = find_project(&snapshot, project)?;
    let overrides = paths.project_overrides(found.id);
    println!(
        "Settings for {} come from {} and {}.",
        shown(&found.name),
        shown(
            &found
                .root
                .join(muster_daemon::config::FILE_NAME)
                .display()
                .to_string()
        ),
        shown(&overrides.display().to_string())
    );
    let loaded = muster_daemon::config::load(&found.root, &overrides)?;
    if loaded.files.is_empty() {
        println!("Neither exists yet; these are the defaults.");
    }
    let config = &loaded.config;
    let gib = |bytes: u64| muster_core::num::fraction(bytes) / f64::from(1u32 << 30);
    println!(
        "\nbase branch   {}",
        config
            .base_branch
            .as_deref()
            .map_or("the repository's default".to_owned(), shown)
    );
    println!(
        "image         {}",
        config
            .dockerfile
            .as_ref()
            .map_or("Muster's base image".to_owned(), |path| shown(
                &path.display().to_string()
            ))
    );
    println!(
        "resources     {:.1} GB, {} CPUs",
        gib(config.memory_bytes),
        f64::from(config.millicpus) / 1000.0
    );
    println!(
        "checks        {} (each within {} min)",
        if config.checks.commands.is_empty() {
            "none".to_owned()
        } else {
            shown(&config.checks.commands.join("; "))
        },
        config.checks.timeout.as_secs().div_ceil(60)
    );
    println!(
        "agents        {} builds, {} reviews",
        shown(&config.agents.builder),
        shown(&config.agents.reviewer)
    );
    println!(
        "limits        {} attempts, {} min, {}",
        config.limits.attempts,
        config.limits.run_timeout.as_secs().div_ceil(60),
        config
            .limits
            .max_cost_usd
            .map_or("no cost limit".to_owned(), |usd| format!("${usd:.2}"))
    );
    let tickets = &config.tickets;
    let name = tickets
        .tracker
        .as_deref()
        .map(muster_core::config::tracker_name)
        .unwrap_or_default();
    let source = tickets
        .written(&name)
        .map_or("as the git remote says".to_owned(), |written| {
            shown(&written)
        });
    let whose = if tickets.mine {
        ", assigned to you"
    } else {
        ""
    };
    println!("tickets       {source}{whose}");
    if let Some(host) = config.pull_requests.host {
        println!("pull requests {}", host.name());
    }
    println!(
        "branches      {}",
        if config.branches.muster_prefix {
            "muster/issue-42-…, with muster/"
        } else {
            "issue-42-…, without muster/"
        }
    );
    match &config.intake.label {
        Some(label) => println!(
            "pickup        issues labelled {:?} by {}",
            shown(label),
            shown(&config.intake.allowed_accounts.join(", "))
        ),
        None => println!("pickup        only issues you choose"),
    }
    match config.ready_for_runs() {
        Ok(()) => println!("\nRuns can use these settings."),
        Err(reason) => println!("\nRuns cannot start yet: {reason}."),
    }
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
            on: None,
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
            shown(workspace.kind.branch()),
            shown(
                &workspace
                    .kind
                    .host_path()
                    .map_or_else(String::new, |path| path.display().to_string())
            )
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
    no_agent_starts_one(&snapshot, "start sessions")?;
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
    let mut connection = connect(socket).await?;
    no_agent_starts_one(&connection.subscribe().await?, "start sessions again")?;
    connection
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
    let deadline = crate::instant_now() + std::time::Duration::from_secs(10);
    // Gone once it no longer answers; it removes its socket as it stops.
    while muster_client::daemon_pid(socket, CLIENT).await.is_ok() {
        if crate::instant_now() >= deadline {
            return crate::fail(format!(
                "the daemon (pid {pid}) is still running after 10 seconds; `kill -9 {pid}` ends it"
            ));
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    println!("Stopped the daemon (pid {pid}).");
    ExitCode::SUCCESS
}

pub(crate) async fn close_issue(
    socket: &Path,
    project: &str,
    key: muster_core::TicketKey,
) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?.id;
    let shown = key.shown();
    connection
        .command(Command::CloseIssue { project, key })
        .await?;
    println!("Closed {shown}.");
    Ok(())
}

/// `muster session list`: every session, in words or with `json` as one
/// JSON document.
pub(crate) async fn list_sessions(socket: &Path, json: bool) -> Outcome {
    let snapshot = connect(socket).await?.subscribe().await?;
    if json {
        return Ok(crate::json::print(&crate::json::SessionList::new(
            &snapshot,
        ))?);
    }
    if snapshot.sessions.is_empty() {
        println!("No sessions.");
    }
    for session in &snapshot.sessions {
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.id == session.workspace);
        let project = workspace
            .and_then(|workspace| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == workspace.project)
            })
            .map_or_else(|| "?".to_owned(), |project| shown(&project.name));
        let place = match workspace {
            Some(workspace) if workspace.kind.in_git() => shown(workspace.kind.branch()),
            Some(_) => "(not a git repository)".to_owned(),
            None => "?".to_owned(),
        };
        let mark = if session.unseen { "•" } else { " " };
        println!(
            "{mark} {}  {}  {}  ({project}, {place})",
            session.id,
            shown(&session.title),
            describe(session.status)
        );
    }
    Ok(())
}

/// `muster issue list`: a project's tickets with the agents on each, in
/// words or with `json` as one JSON document.
pub(crate) async fn list_issues(socket: &Path, project: &str, json: bool) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let project = find_project(&snapshot, project)?.id;
    let reply = connection.command(Command::ListIssues { project }).await?;
    let Reply::Issues {
        issues,
        tracker,
        mine,
        ..
    } = reply
    else {
        return Err(unexpected(&reply));
    };
    if json {
        let tickets = issues
            .into_iter()
            .map(|issue| {
                let agents = sessions_on(&snapshot, project, &issue.key)
                    .into_iter()
                    .map(|session| crate::json::session_entry(&snapshot, session))
                    .collect();
                (issue, agents)
            })
            .collect();
        let document = crate::json::TicketList::new(project, tracker, mine, tickets);
        return Ok(crate::json::print(&document)?);
    }
    if issues.is_empty() {
        let whose = if mine { " assigned to you" } else { "" };
        println!("No open tickets{whose} in {}.", shown(&tracker));
    }
    for issue in issues {
        let labels = if issue.labels.is_empty() {
            String::new()
        } else {
            format!("  [{}]", shown(&issue.labels.join(", ")))
        };
        let agents = agents_on(&snapshot, project, &issue.key);
        let agents = if agents.is_empty() {
            String::new()
        } else {
            format!(
                "  · {}: {}",
                count(agents.len(), "agent"),
                agents.join(", ")
            )
        };
        println!(
            "{:<7} {}{labels}{agents}",
            issue.key.shown(),
            shown(&issue.title)
        );
    }
    Ok(())
}

/// How each agent on ticket `key` is doing.
fn agents_on(
    snapshot: &Snapshot,
    project: muster_core::ProjectId,
    key: &muster_core::TicketKey,
) -> Vec<String> {
    sessions_on(snapshot, project, key)
        .iter()
        .map(|session| describe(session.status))
        .collect()
}

/// The agents on ticket `key`: the sessions in the project's worktrees on
/// branches named for it, oldest first; a plugin's panel beside them is
/// none.
fn sessions_on<'a>(
    snapshot: &'a Snapshot,
    project: muster_core::ProjectId,
    key: &muster_core::TicketKey,
) -> Vec<&'a muster_core::Session> {
    let mut agents: Vec<&muster_core::Session> = snapshot
        .sessions
        .iter()
        .filter(|session| {
            snapshot.workspaces.iter().any(|workspace| {
                workspace.id == session.workspace
                    && workspace.project == project
                    && workspace.kind.is_worktree()
                    && workspace.ticket.as_ref() == Some(key)
            })
        })
        .filter(|session| !session.kind().is_panel())
        .collect();
    agents.sort_by_key(|session| (session.created_at, session.id));
    agents
}

pub(crate) async fn start_issue(
    socket: &Path,
    project: &str,
    key: muster_core::TicketKey,
    harness: String,
    another: bool,
) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    no_agent_starts_one(&snapshot, "start agents on tickets")?;
    let project = find_project(&snapshot, project)?.id;
    let reply = connection
        .command(Command::StartFromIssue {
            project,
            key: key.clone(),
            harness,
            size: TermSize::default(),
            another,
            on: None,
            brief: None,
        })
        .await?;
    let Reply::IssueStarted { workspace, session } = reply else {
        return Err(unexpected(&reply));
    };
    println!("Started {session} for {} in {workspace}.", key.shown());
    Ok(())
}

/// Refuses to set an agent working from inside one of this daemon's
/// sessions that is no shell (`PRD-01`): an agent there could run the
/// command as well as the person, and nothing starts on its own. The
/// person's own shell in Muster may, as may a session of another daemon
/// (one under test), which this one does not know. A guard against an
/// agent's ordinary use, as `muster plugin`'s is, not a wall: a program
/// running as the person can do what the person can.
pub(crate) fn no_agent_starts_one(snapshot: &Snapshot, what: &str) -> Result<(), String> {
    let Some(inside) = std::env::var("MUSTER_SESSION")
        .ok()
        .filter(|session| !session.is_empty())
    else {
        return Ok(());
    };
    let Some(session) = snapshot
        .sessions
        .iter()
        .find(|session| session.id.to_string() == inside)
    else {
        return Ok(());
    };
    if session.kind().is_shell() {
        return Ok(());
    }
    Err(format!(
        "this runs inside {}, a session in Muster, where an agent could run it as well, and \
         agents do not set agents working: {what} in Muster's window, in a shell Muster \
         opened, or in a terminal of your own",
        shown(&session.title)
    ))
}

/// A project by id, name or path.
pub(crate) fn find_project<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
) -> Result<&'a Project, String> {
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
pub(crate) fn find_worktree<'a>(
    snapshot: &'a Snapshot,
    wanted: &str,
) -> Result<&'a Workspace, String> {
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
                || (workspace.kind.in_git() && workspace.kind.branch() == wanted)
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

pub(crate) fn describe(status: SessionStatus) -> String {
    match status {
        SessionStatus::Fresh => "fresh".to_owned(),
        SessionStatus::Running => "running".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        } => "needs you: permission".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        } => "needs you: question".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        } => "needs you: asking before it starts".to_owned(),
        SessionStatus::NeedsYou {
            reason: WaitReason::Action,
        } => "needs you: action required".to_owned(),
        SessionStatus::Finished => "finished".to_owned(),
        SessionStatus::Exited { success: true } => "exited".to_owned(),
        SessionStatus::Exited { success: false } => "exited with an error".to_owned(),
    }
}

pub(crate) fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

pub(crate) fn unexpected(reply: &Reply) -> Box<dyn std::error::Error> {
    format!("unexpected reply {reply:?}").into()
}

/// Text from the daemon as it is printed: issue titles and labels can be
/// written by anyone, and names come from the file system, so control
/// characters, which a terminal would act on, are taken out.
pub(crate) fn shown(text: &str) -> String {
    muster_core::text::one_line(text)
}

/// Stores the token runs' agents use in the macOS Keychain. `security`
/// asks for it itself, on the terminal, so the token is never in a
/// process's arguments, where any other process could read it.
pub(crate) fn store_claude_token() -> ExitCode {
    if !cfg!(target_os = "macos") {
        println!(
            "Muster keeps the token in the macOS Keychain. Elsewhere, set \
             CLAUDE_CODE_OAUTH_TOKEN in the environment the daemon starts in."
        );
        return ExitCode::FAILURE;
    }
    println!(
        "Make a token with `claude setup-token`, then paste it below (twice; it is not shown)."
    );
    let status = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-U",
            "-s",
            muster_daemon::KEYCHAIN_SERVICE,
            "-a",
            muster_daemon::KEYCHAIN_ACCOUNT,
            "-w",
        ])
        .status();
    match status {
        Ok(status) if status.success() => {
            println!("Stored. Runs' agents will use it.");
            ExitCode::SUCCESS
        }
        Ok(_) => crate::fail("the Keychain did not store the token"),
        Err(error) => crate::fail(format!("could not run `security`: {error}")),
    }
}

/// Runs the checks in a worktree, copying their terminal's output here as
/// it comes, and exits as they did.
pub(crate) async fn check(socket: &Path, worktree: &str) -> ExitCode {
    match run_checks(socket, worktree).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => crate::fail(error),
    }
}

async fn run_checks(socket: &Path, worktree: &str) -> Result<bool, Box<dyn std::error::Error>> {
    use muster_core::protocol::{DetachReason, ServerFrame};
    use std::io::Write;

    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let workspace = find_worktree(&snapshot, worktree)?.id;
    let columns = std::env::var("COLUMNS")
        .ok()
        .and_then(|columns| columns.parse().ok())
        .unwrap_or(120);
    let reply = connection
        .command(Command::CheckWorkspace {
            workspace,
            size: TermSize {
                cols: columns,
                rows: 40,
            },
        })
        .await?;
    let Reply::SessionStarted { id } = reply else {
        return Err(unexpected(&reply));
    };
    connection.attach(id, None).await?;
    let mut out = std::io::stdout();
    let mut position = crate::output::Position::default();
    loop {
        match connection.next_event().await? {
            Some(
                ServerFrame::Replay {
                    session, seq, data, ..
                }
                | ServerFrame::Output { session, seq, data },
            ) if session == id => {
                let fresh = position.take(seq, &data);
                if fresh.missed > 0 {
                    // This terminal fell behind the checks' output.
                    out.flush()?;
                    eprintln!(
                        "\n[muster: {} bytes of output were not kept while this terminal fell behind]",
                        fresh.missed
                    );
                }
                out.write_all(fresh.data)?;
                out.flush()?;
            }
            Some(ServerFrame::Detached { session, reason }) if session == id => {
                if reason == DetachReason::NotRunning {
                    return Err("the checks' terminal went away".into());
                }
                break;
            }
            Some(_) => {}
            None => return Err("the daemon closed the connection".into()),
        }
    }
    // The session's end is in the state once its terminal has ended.
    for _ in 0..100 {
        let snapshot = connection.subscribe().await?;
        if let Some(session) = snapshot.sessions.iter().find(|session| session.id == id)
            && let SessionStatus::Exited { success } = session.status
        {
            return Ok(success);
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    Err("the checks ended without saying how".into())
}

pub(crate) async fn review(socket: &Path, worktree: &str) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    no_agent_starts_one(&snapshot, "start reviews")?;
    let workspace = find_worktree(&snapshot, worktree)?;
    let (id, branch) = (workspace.id, workspace.kind.branch().to_owned());
    let reply = connection
        .command(Command::ReviewWorkspace {
            workspace: id,
            size: TermSize::default(),
        })
        .await?;
    let Reply::SessionStarted { id: session } = reply else {
        return Err(unexpected(&reply));
    };
    println!(
        "Claude Code reviews the change on {} in session {session}, changing nothing; \
         `muster` shows it.",
        shown(&branch)
    );
    Ok(())
}

pub(crate) async fn open_pull_request(socket: &Path, worktree: &str) -> Outcome {
    let mut connection = connect(socket).await?;
    let snapshot = connection.subscribe().await?;
    let workspace = find_worktree(&snapshot, worktree)?.id;
    let reply = connection
        .command(Command::OpenPullRequest { workspace })
        .await?;
    let Reply::PullRequestOpened { pr } = reply else {
        return Err(unexpected(&reply));
    };
    println!("Draft pull request #{}: {}", pr.number, shown(&pr.url));
    Ok(())
}
