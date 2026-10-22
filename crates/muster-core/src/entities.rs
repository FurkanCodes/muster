//! The things the daemon stores and clients draw (`ARCHITECTURE.md` §2).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectId, RunId, SessionId, WorkspaceId};
use crate::session::SessionStatus;
use crate::tickets::TicketKey;
use crate::time::Timestamp;

/// A folder Muster knows about: a git repository, or a plain folder.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Project {
    pub id: ProjectId,
    /// Shown in the UI; defaults to the folder's name.
    pub name: String,
    /// The folder: a repository's main checkout, or a plain folder.
    pub root: PathBuf,
    pub created_at: Timestamp,
}

/// Where a session's code lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Workspace {
    pub id: WorkspaceId,
    pub project: ProjectId,
    pub kind: WorkspaceKind,
    pub created_at: Timestamp,
    /// The pull request from its branch, as the git host last said; kept
    /// only while the branch is checked out there, and never stored.
    #[serde(default)]
    pub pull_request: Option<crate::pulls::PullRequestStatus>,
    /// What its work has changed, committed on its branch or not, as git
    /// last said; kept only while it is a repository's, and never stored.
    #[serde(default)]
    pub changes: Option<crate::changes::ChangeSummary>,
    /// The number of the newest entry in its timeline, as its store last
    /// said: a client showing the timeline asks again when it changes.
    /// Kept only while it is a repository's, and never stored.
    #[serde(default)]
    pub timeline: Option<u64>,
    /// The project's other workspaces whose work changes files its own
    /// work changes, as git last said, those that would conflict first;
    /// kept only while it is a repository's, and never stored.
    #[serde(default)]
    pub clashes: Vec<crate::clashes::Clash>,
    /// The branch its own is stacked on, if it was made on top of
    /// another's; stored.
    #[serde(default)]
    pub stacked_on: Option<crate::stacks::Stacked>,
}

/// The kinds of workspace.
///
/// `branch` is what is checked out, kept current as it changes: a branch
/// name, or `detached at <commit>`; empty in a plain folder, which is not
/// a git repository.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceKind {
    /// A git worktree Muster created on the host.
    Worktree { path: PathBuf, branch: String },
    /// The project's own checkout: the repository or plain folder the
    /// person added, where they work too. Muster never removes it.
    Checkout { path: PathBuf, branch: String },
    /// A run's container, where its agents work on a clone of their own
    /// (`ARCHITECTURE.md` §8). Its sessions are the person taking the run
    /// over, or looking around in it; it goes with the run's container.
    Container { run: RunId, branch: String },
}

impl WorkspaceKind {
    /// Where the workspace's files are on the host; a container's are in
    /// the container only.
    pub fn host_path(&self) -> Option<&Path> {
        match self {
            Self::Worktree { path, .. } | Self::Checkout { path, .. } => Some(path),
            Self::Container { .. } => None,
        }
    }

    /// What is checked out there.
    pub fn branch(&self) -> &str {
        match self {
            Self::Worktree { branch, .. }
            | Self::Checkout { branch, .. }
            | Self::Container { branch, .. } => branch,
        }
    }

    /// Whether the workspace is in a git repository: a project that is a
    /// plain folder has none, so its checkout has no branch.
    pub fn in_git(&self) -> bool {
        !self.branch().is_empty()
    }

    /// Whether this is a project's own checkout.
    pub fn is_checkout(&self) -> bool {
        matches!(self, Self::Checkout { .. })
    }

    /// Whether this is a git worktree on the host.
    pub fn is_worktree(&self) -> bool {
        matches!(self, Self::Worktree { .. })
    }

    /// The run whose container this is.
    pub fn run(&self) -> Option<RunId> {
        match self {
            Self::Container { run, .. } => Some(*run),
            Self::Worktree { .. } | Self::Checkout { .. } => None,
        }
    }
}

/// A ticket, as listed for the person to pick from: an issue on GitHub or
/// GitLab, a Jira work item, a Linear issue.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Issue {
    pub key: TicketKey,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Who filed it.
    pub author: Option<String>,
}

/// One agent process in one workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Session {
    pub id: SessionId,
    pub workspace: WorkspaceId,
    /// The harness descriptor's id, such as `claude`.
    pub harness: String,
    pub title: String,
    pub status: SessionStatus,
    /// Something happened the person has not looked at yet.
    pub unseen: bool,
    /// The harness session to resume after a daemon restart, once a turn has run.
    pub resume_id: Option<String>,
    pub created_at: Timestamp,
    pub ended_at: Option<Timestamp>,
    /// When it came to its status, so a person sees how long it has run,
    /// waited or sat finished.
    pub status_since: Timestamp,
    /// What it is doing now, as its hooks tell; kept only while it works
    /// or waits, and never stored. An ended session a daemon restart did
    /// not resume carries why instead.
    #[serde(default)]
    pub activity: Option<crate::session::Activity>,
    /// What its agent has used so far, as the agent's own records say;
    /// read again after a restart, and never stored.
    #[serde(default)]
    pub spend: Option<crate::session::Spend>,
    /// The tidy of its branch's commits it was started for, and what
    /// Muster found once its agent stopped; never stored.
    #[serde(default)]
    pub tidy: Option<crate::tidy::Tidy>,
    /// The chat a plugin's panel was opened beside, which it is shown
    /// beside and told of; `None` for every other session, and for a
    /// panel opened on its workspace. From protocol 43.
    #[serde(default)]
    pub beside: Option<SessionId>,
}

impl Session {
    /// What it is, as its harness says: an agent's chat, the checks, the
    /// person's shell or a plugin's panel.
    pub fn kind(&self) -> crate::harness::SessionKind<'_> {
        crate::harness::SessionKind::of(&self.harness)
    }
}
