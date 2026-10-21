//! Entities to build a state from in tests: the model's own, and those of
//! the clients that keep one (feature `fixtures`).

use std::path::PathBuf;

use muster_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Timestamp, Workspace, WorkspaceId,
    WorkspaceKind,
};

/// A project named `name`, in `/src/<name>`.
pub fn project(name: &str) -> Project {
    Project {
        id: ProjectId::new(),
        name: name.to_owned(),
        root: PathBuf::from(format!("/src/{name}")),
        created_at: Timestamp::from_millis(1),
    }
}

/// A worktree of `project` on `branch`.
pub fn worktree(project: ProjectId, branch: &str) -> Workspace {
    Workspace {
        id: WorkspaceId::new(),
        project,
        kind: WorkspaceKind::Worktree {
            path: PathBuf::from(format!("/w/{branch}")),
            branch: branch.to_owned(),
        },
        created_at: Timestamp::from_millis(2),
        pull_request: None,
        changes: None,
        timeline: None,
        clashes: Vec::new(),
        stacked_on: None,
    }
}

/// A session of an agent that is no one of Muster's own, so nothing
/// an agent of its own would do (a hook, a suspend) comes with it.
pub fn session(workspace: WorkspaceId, title: &str, created: i64) -> Session {
    Session {
        id: SessionId::new(),
        workspace,
        harness: "agent".to_owned(),
        title: title.to_owned(),
        status: SessionStatus::Fresh,
        unseen: false,
        resume_id: None,
        created_at: Timestamp::from_millis(created),
        ended_at: None,
        status_since: Timestamp::from_millis(created),
        activity: None,
        spend: None,
        tidy: None,
        beside: None,
    }
}
