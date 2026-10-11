//! Converting between rows and entities. A value that does not parse is
//! reported as [`StoreError::Invalid`], never silently replaced.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use muster_core::{
    Project, Session, SessionId, SessionStatus, Timestamp, Workspace, WorkspaceKind,
};
use rusqlite::Row;

use super::{Result, StoreError};

/// One entry in a session's status history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub status: SessionStatus,
    pub unseen: bool,
    pub at: Timestamp,
}

/// A session's resource use at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResourceSample {
    pub session: SessionId,
    pub at: Timestamp,
    /// CPU use in thousandths of one core: 1500 is one and a half cores.
    pub cpu_permille: u32,
    pub memory_bytes: u64,
}

pub(super) fn id_text(id: impl std::fmt::Display) -> String {
    id.to_string()
}

pub(super) fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| StoreError::Invalid(format!("the path {path:?} is not valid UTF-8")))
}

pub(super) fn status_json(status: SessionStatus) -> Result<String> {
    serde_json::to_string(&status).map_err(|error| StoreError::Invalid(error.to_string()))
}

/// A workspace as its row stores it.
pub(super) struct WorkspaceColumns<'a> {
    pub(super) kind: &'static str,
    /// Where it is on the host; empty for a container.
    pub(super) path: &'a str,
    pub(super) branch: &'a str,
    /// The run, for a run's container.
    pub(super) run: Option<String>,
}

pub(super) fn workspace_columns(workspace: &Workspace) -> Result<WorkspaceColumns<'_>> {
    Ok(match &workspace.kind {
        WorkspaceKind::Worktree { path, branch } => WorkspaceColumns {
            kind: "worktree",
            path: path_text(path)?,
            branch,
            run: None,
        },
        WorkspaceKind::Checkout { path, branch } => WorkspaceColumns {
            kind: "checkout",
            path: path_text(path)?,
            branch,
            run: None,
        },
        WorkspaceKind::Container { run, branch } => WorkspaceColumns {
            kind: "container",
            path: "",
            branch,
            run: Some(id_text(run)),
        },
    })
}

fn parse_id<T: FromStr>(text: &str) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    text.parse()
        .map_err(|error: T::Err| StoreError::Invalid(error.to_string()))
}

fn parse_status(json: &str) -> Result<SessionStatus> {
    serde_json::from_str(json)
        .map_err(|error| StoreError::Invalid(format!("status {json}: {error}")))
}

// Each reader returns `rusqlite::Result<Result<T>>`: the outer error is
// SQLite's, the inner one a value that did not parse.

pub(super) fn project(row: &Row<'_>) -> rusqlite::Result<Result<Project>> {
    let id: String = row.get(0)?;
    let root: String = row.get(2)?;
    let (name, created_at) = (row.get(1)?, row.get(3)?);
    Ok(parse_id(&id).map(|id| Project {
        id,
        name,
        root: PathBuf::from(root),
        created_at: Timestamp::from_millis(created_at),
    }))
}

pub(super) fn workspace(row: &Row<'_>) -> rusqlite::Result<Result<Workspace>> {
    let (id, project): (String, String) = (row.get(0)?, row.get(1)?);
    let (kind, path, branch): (String, String, String) = (row.get(2)?, row.get(3)?, row.get(4)?);
    let created_at = Timestamp::from_millis(row.get(5)?);
    let run: Option<String> = row.get(6)?;
    Ok((|| {
        let kind = match kind.as_str() {
            "worktree" => WorkspaceKind::Worktree {
                path: PathBuf::from(path),
                branch,
            },
            "checkout" => WorkspaceKind::Checkout {
                path: PathBuf::from(path),
                branch,
            },
            "container" => WorkspaceKind::Container {
                run: parse_id(run.as_deref().unwrap_or_default())?,
                branch,
            },
            other => return Err(StoreError::Invalid(format!("workspace kind {other:?}"))),
        };
        Ok(Workspace {
            id: parse_id(&id)?,
            project: parse_id(&project)?,
            kind,
            created_at,
        })
    })())
}

pub(super) fn session(row: &Row<'_>) -> rusqlite::Result<Result<Session>> {
    let (id, workspace, status): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(4)?);
    let (harness, title, unseen, resume_id) = (row.get(2)?, row.get(3)?, row.get(5)?, row.get(6)?);
    let created_at = Timestamp::from_millis(row.get(7)?);
    let ended_at = row.get::<_, Option<i64>>(8)?.map(Timestamp::from_millis);
    Ok((|| {
        Ok(Session {
            id: parse_id(&id)?,
            workspace: parse_id(&workspace)?,
            harness,
            title,
            status: parse_status(&status)?,
            unseen,
            resume_id,
            created_at,
            ended_at,
        })
    })())
}

pub(super) fn status_change(row: &Row<'_>) -> rusqlite::Result<Result<StatusChange>> {
    let status: String = row.get(0)?;
    let (unseen, at) = (row.get(1)?, Timestamp::from_millis(row.get(2)?));
    Ok(parse_status(&status).map(|status| StatusChange { status, unseen, at }))
}

pub(super) fn sample(row: &Row<'_>) -> rusqlite::Result<Result<ResourceSample>> {
    let session: String = row.get(0)?;
    let at = Timestamp::from_millis(row.get(1)?);
    let (cpu_permille, memory): (u32, i64) = (row.get(2)?, row.get(3)?);
    Ok((|| {
        Ok(ResourceSample {
            session: parse_id(&session)?,
            at,
            cpu_permille,
            memory_bytes: u64::try_from(memory)
                .map_err(|_| StoreError::Invalid(format!("memory {memory}")))?,
        })
    })())
}
