//! What a plugin's hooks are told of (`PRD-15`, `ARCHITECTURE.md` §7a):
//! the kinds of event the daemon raises, and an event as a hook is given
//! it, one JSON object on its stdin. Names and ids only: a project's,
//! a workspace's folder and branch, a session's id, its chat's name (its
//! credentials hidden, as the UI shows it), harness and status, a pull
//! request's number, address and state, a ticket's key. Never a ticket's
//! text, a prompt, an agent's output or code (`PRD-06`, `PRD-07`). Pure:
//! the daemon hands in what it knows.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::placeholders::Context;
use crate::clashes::Clash;
use crate::pulls::{Checks, PullRequestState, PullRequestStatus, Review};
use crate::time::Timestamp;
use crate::{Project, Session, SessionStatus, WaitReason, Workspace};

/// What happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventKind {
    /// An agent's session was started, or started again after it ended.
    #[serde(rename = "session.started")]
    SessionStarted,
    /// It came to need the person: a question, a permission, something
    /// asked before it starts.
    #[serde(rename = "session.needs_you")]
    SessionNeedsYou,
    /// Its turn ended and stayed ended a moment ([`crate::news::FINISH_HOLD`]),
    /// as the person is told of a finish.
    #[serde(rename = "session.finished")]
    SessionFinished,
    /// Its program ended with an error; `session.ended` follows.
    #[serde(rename = "session.error")]
    SessionError,
    /// Its program ended, well or not.
    #[serde(rename = "session.ended")]
    SessionEnded,
    /// Muster made a worktree, or found one made outside it.
    #[serde(rename = "worktree.created")]
    WorktreeCreated,
    /// A worktree was removed, by Muster or outside it.
    #[serde(rename = "worktree.removed")]
    WorktreeRemoved,
    /// The pull request from a workspace's branch changed: opened, its
    /// state, draft or not, its checks or its review.
    #[serde(rename = "pull_request.changed")]
    PullRequestChanged,
    /// The work in two workspaces of a project came to meet in a file.
    #[serde(rename = "clash.found")]
    ClashFound,
    /// The person started an agent on a ticket.
    #[serde(rename = "ticket.assigned")]
    TicketAssigned,
}

impl EventKind {
    /// Every kind, in the order the manual lists them.
    pub const ALL: [Self; 10] = [
        Self::SessionStarted,
        Self::SessionNeedsYou,
        Self::SessionFinished,
        Self::SessionError,
        Self::SessionEnded,
        Self::WorktreeCreated,
        Self::WorktreeRemoved,
        Self::PullRequestChanged,
        Self::ClashFound,
        Self::TicketAssigned,
    ];

    /// Its name, as a manifest's `on` writes it: `session.finished`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SessionStarted => "session.started",
            Self::SessionNeedsYou => "session.needs_you",
            Self::SessionFinished => "session.finished",
            Self::SessionError => "session.error",
            Self::SessionEnded => "session.ended",
            Self::WorktreeCreated => "worktree.created",
            Self::WorktreeRemoved => "worktree.removed",
            Self::PullRequestChanged => "pull_request.changed",
            Self::ClashFound => "clash.found",
            Self::TicketAssigned => "ticket.assigned",
        }
    }

    /// The kind `name` names, if it is one.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// Whether an event of this kind is a session's, with its id and
    /// harness: `{session}` and `{harness}` have values.
    pub const fn is_a_session_s(self) -> bool {
        matches!(
            self,
            Self::SessionStarted
                | Self::SessionNeedsYou
                | Self::SessionFinished
                | Self::SessionError
                | Self::SessionEnded
                | Self::TicketAssigned
        )
    }

    /// Whether an event of this kind says why its session waits.
    pub const fn has_reason(self) -> bool {
        matches!(self, Self::SessionNeedsYou)
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// An event, as a hook is given it on stdin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub event: EventKind,
    /// When it happened, in milliseconds since 1970, UTC.
    pub at: Timestamp,
    /// Made up, by `muster plugin hook`, for trying a hook.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sample: bool,
    #[serde(default)]
    pub project: Option<EventProject>,
    #[serde(default)]
    pub workspace: Option<EventWorkspace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<EventSession>,
    /// The ticket the workspace's branch is for, by its key: `42`,
    /// `APP-7`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ticket: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pull_request: Option<EventPullRequest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clash: Option<EventClash>,
}

/// The project an event is in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventProject {
    pub id: String,
    pub name: String,
    /// Its folder.
    pub root: String,
}

/// The workspace an event is in.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventWorkspace {
    pub id: String,
    /// Its folder; none for a run's container, whose files are in it only.
    #[serde(default)]
    pub folder: Option<String>,
    /// What it has checked out; none in a plain folder.
    #[serde(default)]
    pub branch: Option<String>,
}

/// The session an event is of.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventSession {
    pub id: String,
    /// Its chat's name, as the UI shows it.
    pub name: String,
    /// `claude`, `codex`, an agent the person added.
    pub harness: String,
    /// `fresh`, `running`, `needs_you`, `finished` or `exited`.
    pub status: String,
    /// Why it waits, while it needs the person.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<WaitReason>,
    /// Whether its program ended well, once it has ended.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
}

/// The pull request from a workspace's branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventPullRequest {
    pub number: u64,
    pub url: String,
    pub state: PullRequestState,
    pub draft: bool,
    #[serde(default)]
    pub checks: Option<Checks>,
    #[serde(default)]
    pub review: Option<Review>,
}

/// Where the work in an event's workspace meets another's: the other,
/// and how many files both change, never which or what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventClash {
    pub with: EventWorkspace,
    pub files: u64,
    /// How many of them would conflict, were both merged.
    pub conflicts: u64,
}

impl Event {
    /// An event of `kind` at `at`, in `workspace` of `project`, with
    /// nothing else yet: its ticket is the one its branch is for.
    pub fn new(
        kind: EventKind,
        at: Timestamp,
        project: Option<&Project>,
        workspace: Option<&Workspace>,
    ) -> Self {
        let workspace = workspace.map(EventWorkspace::of);
        let ticket = workspace
            .as_ref()
            .and_then(|workspace| workspace.branch.as_deref())
            .and_then(crate::tickets::issue_of_branch)
            .map(String::from);
        Self {
            event: kind,
            at,
            sample: false,
            project: project.map(|project| EventProject {
                id: project.id.to_string(),
                name: crate::text::one_line(&project.name),
                root: project.root.to_string_lossy().into_owned(),
            }),
            workspace,
            session: None,
            ticket,
            pull_request: None,
            clash: None,
        }
    }

    /// The same, of `session`, as it is now.
    #[must_use]
    pub fn of_session(mut self, session: &Session) -> Self {
        let (status, reason, success) = match session.status {
            SessionStatus::Fresh => ("fresh", None, None),
            SessionStatus::Running => ("running", None, None),
            SessionStatus::NeedsYou { reason } => ("needs_you", Some(reason), None),
            SessionStatus::Finished => ("finished", None, None),
            SessionStatus::Exited { success } => ("exited", None, Some(success)),
        };
        let (name, _) = crate::credentials::hide(&session.title);
        self.session = Some(EventSession {
            id: session.id.to_string(),
            name: crate::text::one_line(&name),
            harness: session.harness.clone(),
            status: status.to_owned(),
            reason,
            success,
        });
        self
    }

    /// The same, with `pull`, the pull request from its branch.
    #[must_use]
    pub fn with_pull_request(mut self, pull: &PullRequestStatus) -> Self {
        self.pull_request = Some(EventPullRequest {
            number: pull.number,
            url: crate::text::one_line(&pull.url),
            state: pull.state,
            draft: pull.state == PullRequestState::Draft,
            checks: pull.checks,
            review: pull.review,
        });
        self
    }

    /// The same, with `clash`, where its work meets `with`'s.
    #[must_use]
    pub fn with_clash(mut self, clash: &Clash, with: &Workspace) -> Self {
        self.clash = Some(EventClash {
            with: EventWorkspace::of(with),
            files: clash.files,
            conflicts: clash.conflicts,
        });
        self
    }

    /// A made-up event of `kind` at `at`, for trying a hook
    /// (`muster plugin hook`): marked `sample`, with every value an event
    /// of its kind has.
    pub fn sample(kind: EventKind, at: Timestamp) -> Self {
        let workspace = |id: &str, branch: &str| EventWorkspace {
            id: format!("wsp_{id}"),
            folder: Some(format!("/tmp/muster-sample/{branch}")),
            branch: Some(format!("muster/{branch}")),
        };
        let mut event = Self {
            event: kind,
            at,
            sample: true,
            project: Some(EventProject {
                id: "prj_sample".to_owned(),
                name: "sample".to_owned(),
                root: "/tmp/muster-sample/sample".to_owned(),
            }),
            workspace: Some(workspace("sample", "issue-42-sample")),
            session: None,
            ticket: Some("42".to_owned()),
            pull_request: None,
            clash: None,
        };
        let session = |status: &str, reason, success| EventSession {
            id: "ses_sample".to_owned(),
            name: "A sample chat".to_owned(),
            harness: "claude".to_owned(),
            status: status.to_owned(),
            reason,
            success,
        };
        match kind {
            EventKind::SessionStarted | EventKind::TicketAssigned => {
                event.session = Some(session("running", None, None));
            }
            EventKind::SessionNeedsYou => {
                event.session = Some(session("needs_you", Some(WaitReason::Question), None));
            }
            EventKind::SessionFinished => event.session = Some(session("finished", None, None)),
            EventKind::SessionError => event.session = Some(session("exited", None, Some(false))),
            EventKind::SessionEnded => event.session = Some(session("exited", None, Some(true))),
            EventKind::WorktreeCreated | EventKind::WorktreeRemoved => {}
            EventKind::PullRequestChanged => {
                event.pull_request = Some(EventPullRequest {
                    number: 7,
                    url: "https://example.com/sample/pull/7".to_owned(),
                    state: PullRequestState::Open,
                    draft: false,
                    checks: Some(Checks::Passing),
                    review: Some(Review::Required),
                });
            }
            EventKind::ClashFound => {
                event.clash = Some(EventClash {
                    with: workspace("other", "other-work"),
                    files: 2,
                    conflicts: 1,
                });
            }
        }
        event
    }

    /// Its project's name, if it is in one.
    pub fn project_name(&self) -> Option<&str> {
        self.project.as_ref().map(|project| project.name.as_str())
    }

    /// What a hook's command is filled in from, and its `MUSTER_PLUGIN_*`
    /// variables: names and ids only, never an agent's conversation
    /// (`{resume}`, `{transcript}`).
    pub fn context(&self) -> Context {
        let workspace = self.workspace.as_ref();
        Context {
            project: self.project.as_ref().map(|project| project.name.clone()),
            root: self.project.as_ref().map(|project| project.root.clone()),
            workspace: workspace.and_then(|workspace| workspace.folder.clone()),
            branch: workspace.and_then(|workspace| workspace.branch.clone()),
            session: self.session.as_ref().map(|session| session.id.clone()),
            harness: self.session.as_ref().map(|session| session.harness.clone()),
            resume: None,
            transcript: None,
            ticket: self.ticket.clone(),
            pr: self.pull_request.as_ref().map(|pull| pull.url.clone()),
        }
    }

    /// The JSON a hook reads on its stdin: one line.
    pub fn line(&self) -> String {
        let mut line = serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned());
        line.push('\n');
        line
    }
}

impl EventWorkspace {
    fn of(workspace: &Workspace) -> Self {
        let branch = workspace.kind.branch();
        Self {
            id: workspace.id.to_string(),
            folder: workspace
                .kind
                .host_path()
                .map(|path| path.to_string_lossy().into_owned()),
            branch: (!branch.is_empty()).then(|| crate::text::one_line(branch)),
        }
    }
}

/// The variable a hook finds its event's name in.
pub const EVENT_VARIABLE: &str = "MUSTER_PLUGIN_EVENT";

#[cfg(test)]
mod tests;
