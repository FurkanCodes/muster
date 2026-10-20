//! Converting between rows and entities. A value that does not parse is
//! reported as [`StoreError::Invalid`], never silently replaced.

use std::path::{Path, PathBuf};
use std::str::FromStr;

use muster_core::{Project, Session, SessionStatus, Timestamp, Workspace, WorkspaceKind};
use rusqlite::Row;

use super::{Result, StoreError};

/// One entry in a session's status history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatusChange {
    pub status: SessionStatus,
    pub unseen: bool,
    pub at: Timestamp,
}

pub(super) fn id_text(id: impl std::fmt::Display) -> String {
    id.to_string()
}

pub(super) fn path_text(path: &Path) -> Result<&str> {
    path.to_str().ok_or_else(|| {
        StoreError::Invalid(format!("the path {} is not valid UTF-8", path.display()))
    })
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
    let stacked_on: Option<String> = row.get(7)?;
    let stacked_at: Option<String> = row.get(8)?;
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
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: stacked_on
                .zip(stacked_at)
                .map(|(branch, at)| muster_core::stacks::Stacked { branch, at }),
        })
    })())
}

pub(super) fn session(row: &Row<'_>) -> rusqlite::Result<Result<Session>> {
    let (id, workspace, status): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(4)?);
    let (harness, title, unseen, resume_id) = (row.get(2)?, row.get(3)?, row.get(5)?, row.get(6)?);
    let created_at = Timestamp::from_millis(row.get(7)?);
    let ended_at = row.get::<_, Option<i64>>(8)?.map(Timestamp::from_millis);
    // Its latest change of status, or its start where none is kept.
    let status_since = row
        .get::<_, Option<i64>>(9)?
        .map_or(created_at, Timestamp::from_millis);
    let beside: Option<String> = row.get(10)?;
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
            status_since,
            activity: None,
            spend: None,
            tidy: None,
            beside: beside.as_deref().map(parse_id).transpose()?,
        })
    })())
}

pub(super) fn status_change(row: &Row<'_>) -> rusqlite::Result<Result<StatusChange>> {
    let status: String = row.get(0)?;
    let (unseen, at) = (row.get(1)?, Timestamp::from_millis(row.get(2)?));
    Ok(parse_status(&status).map(|status| StatusChange { status, unseen, at }))
}
