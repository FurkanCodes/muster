//! The things the daemon stores and clients draw (`ARCHITECTURE.md` §2).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ids::{ProjectId, SessionId, WorkspaceId};
use crate::session::SessionStatus;
use crate::time::Timestamp;

/// A git repository Muster knows about.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    /// Shown in the UI; defaults to the repository's directory name.
    pub name: String,
    /// The repository's main checkout.
    pub root: PathBuf,
    pub created_at: Timestamp,
}

/// Where a session's code lives.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: WorkspaceId,
    pub project: ProjectId,
    pub kind: WorkspaceKind,
    pub created_at: Timestamp,
}

/// The kinds of workspace. Containers arrive in M2.
///
/// `branch` is what is checked out, kept current as it changes: a branch
/// name, or `detached at <commit>`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceKind {
    /// A git worktree Muster created on the host.
    Worktree { path: PathBuf, branch: String },
    /// The project's own checkout: the repository the person added, where
    /// they work too. Muster never removes it.
    Checkout { path: PathBuf, branch: String },
}

impl WorkspaceKind {
    /// Where the workspace's files are.
    pub fn path(&self) -> &Path {
        match self {
            Self::Worktree { path, .. } | Self::Checkout { path, .. } => path,
        }
    }

    /// What is checked out there.
    pub fn branch(&self) -> &str {
        match self {
            Self::Worktree { branch, .. } | Self::Checkout { branch, .. } => branch,
        }
    }

    /// Whether this is a project's own checkout.
    pub fn is_checkout(&self) -> bool {
        matches!(self, Self::Checkout { .. })
    }
}

/// A GitHub issue, as listed for the person to pick from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub labels: Vec<String>,
    /// Who filed it.
    pub author: Option<String>,
}

/// One agent process in one workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
}
