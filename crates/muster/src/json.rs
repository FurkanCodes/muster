//! What the read commands print with `--json`: one JSON document per
//! command, for the person's own scripts (`muster status`, `muster list`).
//!
//! The shape is the command line's own, built from the daemon's typed
//! replies rather than copied from the protocol, so the protocol can change
//! without breaking a script. Every document says its `format` at the top;
//! it changes only when the shape does, deliberately.
//!
//! Nothing secret is printed (`PRD-06`): Muster holds no credential in what
//! these report, and a ticket's text has its credentials hidden, as on the
//! UI's screen. Text from outside Muster is printed as it is, but no control
//! character reaches the terminal raw (`PRD-07`): JSON escapes those below
//! a space, and [`terminal_safe`] the rest a terminal acts on.

use std::fmt::Write as _;

use muster_core::protocol::Snapshot;
use muster_core::{
    Checks, Issue, PullRequestState, Review, Session, SessionStatus, WaitReason, Workspace,
    WorkspaceKind,
};
use serde::Serialize;

/// The shape of every document, as its `format` says.
pub(crate) const FORMAT: u32 = 2;

/// Prints `document` as JSON on standard output.
pub(crate) fn print(document: &impl Serialize) -> Result<(), serde_json::Error> {
    let text = serde_json::to_string_pretty(document)?;
    println!("{}", terminal_safe(&text));
    Ok(())
}

/// `json` with the characters a terminal acts on that JSON leaves as they
/// are, DEL and the C1 controls, written as escapes. In a serialized
/// document they can only be inside strings, where an escape reads the
/// same, so the document says what it said.
pub(crate) fn terminal_safe(json: &str) -> String {
    let mut safe = String::with_capacity(json.len());
    for character in json.chars() {
        if ('\u{7f}'..='\u{9f}').contains(&character) {
            let _ = write!(safe, "\\u{:04x}", u32::from(character));
        } else {
            safe.push(character);
        }
    }
    safe
}

/// `muster status --json` when no daemon answers.
#[derive(Debug, Serialize)]
pub(crate) struct NotRunning {
    pub(crate) format: u32,
    pub(crate) running: bool,
}

impl NotRunning {
    pub(crate) fn new() -> Self {
        Self {
            format: FORMAT,
            running: false,
        }
    }
}

/// `muster status --json`: the daemon, and every project with its
/// workspaces and their sessions.
#[derive(Debug, Serialize)]
pub(crate) struct Status {
    format: u32,
    running: bool,
    daemon: Daemon,
    /// How the daemon is doing inside; `None` from a daemon too old to say.
    health: Option<Health>,
    /// The machine's memory, once measured.
    memory: Option<Memory>,
    projects: Vec<ProjectEntry>,
    /// The terminals in no project (`ARC-33`).
    terminals: Vec<SessionEntry>,
    /// The person's spaces, by name, with the ids of what is in each.
    spaces: Vec<SpaceEntry>,
}

#[derive(Debug, Serialize)]
struct SpaceEntry {
    id: String,
    name: String,
    /// The projects' and terminals' ids, in the order they were put in.
    members: Vec<String>,
}

#[derive(Debug, Serialize)]
struct Daemon {
    version: String,
    pid: u32,
    protocol: u32,
}

#[derive(Debug, Serialize)]
struct Health {
    tasks: u64,
    work: u64,
    clients: u64,
    terminals: u64,
}

#[derive(Debug, Serialize)]
struct Memory {
    used_bytes: u64,
    total_bytes: u64,
}

#[derive(Debug, Serialize)]
struct ProjectEntry {
    id: String,
    name: String,
    path: String,
    workspaces: Vec<WorkspaceEntry>,
}

#[derive(Debug, Serialize)]
struct WorkspaceEntry {
    id: String,
    /// `worktree`, `checkout` (the project's own folder) or `container` (a
    /// run's).
    kind: &'static str,
    /// Where its files are on the host; `None` for a container.
    path: Option<String>,
    /// What is checked out; `None` in a plain folder, not a repository.
    branch: Option<String>,
    /// The branch it was made on top of, if stacked on another's.
    stacked_on: Option<String>,
    pull_request: Option<PullRequestEntry>,
    changes: Option<ChangesEntry>,
    sessions: Vec<SessionEntry>,
}

#[derive(Debug, Serialize)]
struct PullRequestEntry {
    number: u64,
    url: String,
    /// `draft`, `open`, `merged` or `closed`.
    state: &'static str,
    /// `running`, `passed` or `failed`; `None` where none ran or the host
    /// does not say.
    checks: Option<&'static str>,
    /// `required`, `approved` or `changes_requested`; `None` where none is
    /// asked for or the host does not say.
    review: Option<&'static str>,
}

#[derive(Debug, Serialize)]
struct ChangesEntry {
    files: u64,
    added: u64,
    removed: u64,
}

/// A session, as every document gives it.
#[derive(Debug, Serialize)]
pub(crate) struct SessionEntry {
    id: String,
    project: Option<String>,
    /// `None` for a terminal in no project (`ARC-33`).
    workspace: Option<String>,
    /// What its workspace has checked out; `None` in a plain folder.
    branch: Option<String>,
    /// What it was started as: `shell` for a terminal, else the agent's.
    harness: String,
    /// The coding agent running in front in it, where it is a terminal
    /// (`ARC-28`).
    agent: Option<String>,
    title: String,
    /// `fresh`, `running`, `needs_you`, `finished` or `exited`.
    status: &'static str,
    /// While it needs the person: `permission`, `question`, `startup` or
    /// `action`.
    needs: Option<&'static str>,
    /// Once it has exited: whether it exited cleanly.
    success: Option<bool>,
    /// Something happened the person has not looked at yet.
    unseen: bool,
    /// When it came to its status, in milliseconds since the Unix epoch.
    status_since_ms: i64,
    created_at_ms: i64,
    ended_at_ms: Option<i64>,
    /// What it is doing now, as its agent tells: `editing view.rs`.
    activity: Option<String>,
    spend: Option<SpendEntry>,
    usage: Option<UsageEntry>,
    /// The agent's session that started it, where an agent did (`PRD-01`).
    started_by: Option<String>,
}

#[derive(Debug, Serialize)]
struct SpendEntry {
    tokens: u64,
    /// Where the agent reports a cost.
    cost_usd: Option<f64>,
}

#[derive(Debug, Serialize)]
struct UsageEntry {
    memory_bytes: u64,
    /// Of one core: 150 is one and a half.
    cpu_percent: f64,
}

impl Status {
    pub(crate) fn new(
        welcome: &muster_core::protocol::Welcome,
        health: Option<muster_core::protocol::Health>,
        snapshot: &Snapshot,
    ) -> Self {
        let projects = snapshot
            .projects
            .iter()
            .map(|project| ProjectEntry {
                id: project.id.to_string(),
                name: project.name.clone(),
                path: project.root.display().to_string(),
                workspaces: snapshot
                    .workspaces
                    .iter()
                    .filter(|workspace| workspace.project == project.id)
                    .map(|workspace| workspace_entry(snapshot, workspace))
                    .collect(),
            })
            .collect();
        Self {
            format: FORMAT,
            running: true,
            daemon: Daemon {
                version: welcome.daemon_version.clone(),
                pid: welcome.daemon_pid,
                protocol: welcome.version,
            },
            health: health.map(|health| Health {
                tasks: health.tasks,
                work: health.work,
                clients: health.clients,
                terminals: health.terminals,
            }),
            memory: snapshot.resources.as_ref().map(|report| Memory {
                used_bytes: report.host_memory_used,
                total_bytes: report.host_memory_total,
            }),
            projects,
            terminals: snapshot
                .sessions
                .iter()
                .filter(|session| session.is_free())
                .map(|session| session_entry(snapshot, session))
                .collect(),
            spaces: snapshot
                .spaces
                .iter()
                .map(|space| SpaceEntry {
                    id: space.id.to_string(),
                    name: space.name.clone(),
                    members: space
                        .members
                        .iter()
                        .map(|member| match member {
                            muster_core::Member::Project(id) => id.to_string(),
                            muster_core::Member::Session(id) => id.to_string(),
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

fn workspace_entry(snapshot: &Snapshot, workspace: &Workspace) -> WorkspaceEntry {
    let kind = match workspace.kind {
        WorkspaceKind::Worktree { .. } => "worktree",
        WorkspaceKind::Checkout { .. } => "checkout",
        WorkspaceKind::Container { .. } => "container",
    };
    WorkspaceEntry {
        id: workspace.id.to_string(),
        kind,
        path: workspace
            .kind
            .host_path()
            .map(|path| path.display().to_string()),
        branch: branch(workspace),
        stacked_on: workspace
            .stacked_on
            .as_ref()
            .map(|stacked| stacked.branch.clone()),
        pull_request: workspace
            .pull_request
            .as_ref()
            .map(|pull_request| PullRequestEntry {
                number: pull_request.number,
                url: pull_request.url.clone(),
                state: match pull_request.state {
                    PullRequestState::Draft => "draft",
                    PullRequestState::Open => "open",
                    PullRequestState::Merged => "merged",
                    PullRequestState::Closed => "closed",
                },
                checks: pull_request.checks.map(|checks| match checks {
                    Checks::Pending => "running",
                    Checks::Passing => "passed",
                    Checks::Failing => "failed",
                }),
                review: pull_request.review.map(|review| match review {
                    Review::Required => "required",
                    Review::Approved => "approved",
                    Review::ChangesRequested => "changes_requested",
                }),
            }),
        changes: workspace.changes.as_ref().map(|changes| ChangesEntry {
            files: changes.files,
            added: changes.added,
            removed: changes.removed,
        }),
        sessions: snapshot
            .sessions
            .iter()
            .filter(|session| session.is_in(workspace.id))
            .map(|session| session_entry(snapshot, session))
            .collect(),
    }
}

/// What `workspace` has checked out; `None` in a plain folder.
fn branch(workspace: &Workspace) -> Option<String> {
    workspace
        .kind
        .in_git()
        .then(|| workspace.kind.branch().to_owned())
}

/// `session`, with where it works and what it uses as `snapshot` says.
pub(crate) fn session_entry(snapshot: &Snapshot, session: &Session) -> SessionEntry {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| session.is_in(workspace.id));
    let (status, needs, success) = match session.status {
        SessionStatus::Fresh => ("fresh", None, None),
        SessionStatus::Running => ("running", None, None),
        SessionStatus::NeedsYou { reason } => (
            "needs_you",
            Some(match reason {
                WaitReason::Permission => "permission",
                WaitReason::Question => "question",
                WaitReason::Startup => "startup",
                WaitReason::Action => "action",
            }),
            None,
        ),
        SessionStatus::Finished => ("finished", None, None),
        SessionStatus::Exited { success } => ("exited", None, Some(success)),
    };
    let usage = snapshot
        .resources
        .as_ref()
        .and_then(|report| report.sessions.iter().find(|u| u.session == session.id))
        .map(|usage| UsageEntry {
            memory_bytes: usage.memory_bytes,
            cpu_percent: f64::from(usage.cpu_permille) / 10.0,
        });
    SessionEntry {
        id: session.id.to_string(),
        project: workspace.map(|workspace| workspace.project.to_string()),
        workspace: session.workspace.map(|id| id.to_string()),
        branch: workspace.and_then(branch),
        harness: session.harness.clone(),
        agent: session.agent.as_ref().map(|agent| agent.harness.clone()),
        title: session.title.clone(),
        status,
        needs,
        success,
        unseen: session.unseen,
        status_since_ms: session.status_since.as_millis(),
        created_at_ms: session.created_at.as_millis(),
        ended_at_ms: session.ended_at.map(muster_core::Timestamp::as_millis),
        activity: session
            .activity
            .as_ref()
            .map(|activity| activity.text.clone()),
        spend: session.spend.map(|spend| SpendEntry {
            tokens: spend.tokens,
            cost_usd: spend
                .cost_micros
                .map(|micros| muster_core::num::fraction(micros) / 1_000_000.0),
        }),
        usage,
        started_by: session.started_by.map(|starter| starter.to_string()),
    }
}

/// `muster list --json`.
#[derive(Debug, Serialize)]
pub(crate) struct SessionList {
    format: u32,
    sessions: Vec<SessionEntry>,
}

impl SessionList {
    pub(crate) fn new(snapshot: &Snapshot) -> Self {
        Self {
            format: FORMAT,
            sessions: snapshot
                .sessions
                .iter()
                .map(|session| session_entry(snapshot, session))
                .collect(),
        }
    }
}

/// `muster list tickets --json`: a project's open tickets, with the agents
/// on each.
#[derive(Debug, Serialize)]
pub(crate) struct TicketList {
    format: u32,
    project: String,
    /// Where they came from, in words: `GitHub issues`.
    tracker: String,
    /// Only those assigned to the person signed in.
    mine: bool,
    tickets: Vec<TicketEntry>,
}

#[derive(Debug, Serialize)]
struct TicketEntry {
    /// `#42` or `APP-7`, as `muster ticket` takes it.
    key: String,
    title: String,
    url: String,
    labels: Vec<String>,
    author: Option<String>,
    agents: Vec<SessionEntry>,
}

impl TicketList {
    pub(crate) fn new(
        project: muster_core::ProjectId,
        tracker: String,
        mine: bool,
        tickets: Vec<(Issue, Vec<SessionEntry>)>,
    ) -> Self {
        Self {
            format: FORMAT,
            project: project.to_string(),
            tracker,
            mine,
            tickets: tickets
                .into_iter()
                .map(|(issue, agents)| TicketEntry {
                    key: issue.key.shown(),
                    title: hidden(&issue.title),
                    url: hidden(&issue.url),
                    labels: issue.labels.iter().map(|label| hidden(label)).collect(),
                    author: issue.author.as_deref().map(hidden),
                    agents,
                })
                .collect(),
        }
    }
}

/// A ticket's `text` with any credential in it hidden, as the UI shows it
/// (`PRD-06`). The daemon hides them in what it sends; hiding them here
/// as well keeps that true of what is printed whatever sent it.
fn hidden(text: &str) -> String {
    muster_core::credentials::hide(text).0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_terminal_acts_on_is_escaped_and_reads_the_same() {
        let document = serde_json::json!({"title": "a\u{1b}[2Jb\u{9b}6nc\u{7f}d é ✓"});
        let text = terminal_safe(&serde_json::to_string(&document).unwrap());
        assert!(
            !text
                .chars()
                .any(|c| c.is_control() && c != '\n' && c != '\t'),
            "{text:?}"
        );
        assert!(
            text.contains("\\u009b") && text.contains("\\u007f"),
            "{text}"
        );
        assert!(text.contains("é ✓"), "{text}");
        let read: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(read, document);
    }

    #[test]
    fn a_ticket_s_credentials_stay_hidden() {
        let token =
            "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJleHAiOjk5fQ.c2lnbmF0dXJlLWZvci10ZXN0cw";
        let issue = Issue {
            key: muster_core::TicketKey::number(4),
            title: format!("Fails with Bearer {token}"),
            url: "https://github.com/o/app/issues/4".to_owned(),
            labels: vec![format!("token={token}")],
            author: None,
        };
        let tickets = TicketList::new(
            muster_core::ProjectId::new(),
            "GitHub issues".to_owned(),
            false,
            vec![(issue, Vec::new())],
        );
        let text = serde_json::to_string(&tickets).unwrap();
        assert!(!text.contains(token), "{text}");
        assert!(text.contains(muster_core::credentials::HIDDEN), "{text}");
        assert!(text.contains("\"key\":\"#4\""), "{text}");
        assert!(
            text.starts_with(&format!("{{\"format\":{FORMAT},")),
            "{text}"
        );
    }
}
