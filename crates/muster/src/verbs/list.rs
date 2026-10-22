//! `muster ls`: the sessions of the project the person stands in, each
//! on a line with the id the other verbs take.

use std::process::ExitCode;

use muster_client::ClientError;
use muster_core::protocol::Snapshot;
use muster_core::{Paths, Session};

use super::targets;
use crate::commands::{self, describe, shown};

pub(crate) async fn run(
    paths: &Paths,
    all: bool,
    json: bool,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut connection = match commands::connect(&paths.socket()).await {
        Ok(connection) => connection,
        Err(ClientError::Unreachable { .. }) => {
            println!("The daemon is not running, so no session is.");
            return Ok(ExitCode::SUCCESS);
        }
        Err(error) => return Err(error.into()),
    };
    let mut snapshot = connection.subscribe().await?;
    if !all {
        let cwd = std::env::current_dir()?;
        if let Some(project) = targets::here(&snapshot, &cwd).map(|here| here.project.id) {
            keep_project(&mut snapshot, project);
        }
    }
    if json {
        crate::json::print(&crate::json::SessionList::new(&snapshot))?;
        return Ok(ExitCode::SUCCESS);
    }
    if snapshot.sessions.is_empty() {
        println!("No sessions.");
        return Ok(ExitCode::SUCCESS);
    }
    let now = crate::runs::now();
    for session in &snapshot.sessions {
        println!("{}", line(&snapshot, session, now));
    }
    Ok(ExitCode::SUCCESS)
}

/// `snapshot` with only `project`'s workspaces and sessions.
fn keep_project(snapshot: &mut Snapshot, project: muster_core::ProjectId) {
    snapshot
        .workspaces
        .retain(|workspace| workspace.project == project);
    let workspaces: Vec<_> = snapshot.workspaces.iter().map(|w| w.id).collect();
    snapshot
        .sessions
        .retain(|session| workspaces.contains(&session.workspace));
    snapshot.projects.retain(|found| found.id == project);
}

/// `session` on one line: its id in short, how it is doing and for how
/// long, its agent, its branch and its name, and who started it where an
/// agent did.
fn line(snapshot: &Snapshot, session: &Session, now: muster_core::Timestamp) -> String {
    let branch = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.id == session.workspace)
        .filter(|workspace| workspace.kind.in_git())
        .map_or_else(
            || "-".to_owned(),
            |workspace| shown(workspace.kind.branch()),
        );
    let mark = if session.unseen { "•" } else { " " };
    let how = format!(
        "{} {}",
        describe(session.status),
        muster_core::time::span(muster_core::time::between(session.status_since, now))
    );
    let by = session
        .started_by
        .map(|starter| format!("  (started by {})", targets::short(starter)))
        .unwrap_or_default();
    format!(
        "{mark}{}  {how:<28}  {:<8}  {branch:<32}  {}{by}",
        targets::short(session.id),
        shown(&session.harness),
        shown(&session.title)
    )
}
