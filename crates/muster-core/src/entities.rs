//! The things the daemon stores and clients draw (`ARCHITECTURE.md` §2).

use std::path::PathBuf;

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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceKind {
    /// A git worktree on the host.
    Worktree { path: PathBuf, branch: String },
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
