//! What each workspace's work has changed (`PRODUCT.md` §6): the whole of
//! it for a client that asks, a file's diff, and on each workspace a
//! summary for its line in the list. Git runs off the loop, as a task with
//! a deadline (`ARC-13`), and comes back with what it found.
//!
//! The summary is looked at again when an agent's turn ends there, when
//! its branch changes, on the pull request timer while the person looks at
//! a client (every tenth tick otherwise, and at once when they look again
//! after a tick went by), and whenever a client asks for the whole, so the
//! line agrees with what the person reads. It is kept on the workspace,
//! never stored.

use std::path::PathBuf;

use muster_core::changes::{ChangePart, ChangeSummary};
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use tokio::sync::oneshot;

use super::{Request, Supervisor, by_hand};
use crate::changes::{self, DiffError};

/// While nobody looks at a client, one tick in this many looks.
const UNWATCHED_EVERY: u32 = 10;

/// A look still not back after this many ticks is taken as lost.
const LOST_AFTER: u32 = 10;

/// The Supervisor's part: whether a look at every workspace is under way.
#[derive(Debug, Default)]
pub(super) struct Looks {
    under_way: Option<u32>,
    ticks: u32,
    /// A tick went by without a look, as nobody looked at a client.
    missed: bool,
}

/// A workspace whose changes git can tell.
#[derive(Debug, Clone)]
pub(super) struct Place {
    pub(super) workspace: WorkspaceId,
    /// What it had checked out when asked, so a summary found for one
    /// branch is not kept for the next.
    pub(super) branch: String,
    pub(super) path: PathBuf,
    /// The person's overrides of its project's settings, which may name
    /// the base.
    overrides: PathBuf,
    /// The branch it is stacked on, its base while the repository has it.
    stacked: Option<muster_core::stacks::Stacked>,
}

/// What a look found: each workspace's summary, for the branch it was on.
#[derive(Debug, Default)]
pub(super) struct Looked {
    whole: bool,
    found: Vec<(WorkspaceId, String, Option<ChangeSummary>)>,
}

impl Supervisor {
    /// `WorkspaceChanges`: answered once git has said; the workspace's
    /// summary is kept in step.
    pub(super) fn workspace_changes(
        &self,
        workspace: WorkspaceId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.changes_place(workspace) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let base = base(&place).await;
            match changes::changes(&place.path, &base).await {
                Ok(found) => {
                    let summary = found.summary();
                    let _ = reply.send(Ok(Reply::Changes(found)));
                    let looked = Looked {
                        whole: false,
                        found: vec![(place.workspace, place.branch, summary)],
                    };
                    let _ = requests
                        .send(Request::ChangesLooked(Box::new(looked)))
                        .await;
                }
                Err(error) => {
                    tracing::info!(command = "workspace changes", %error, "command refused");
                    let _ = reply.send(Err(ErrorBody::new(ErrorCode::Internal, error.to_string())));
                }
            }
        });
    }

    /// `FileDiff`: one changed file's diff, answered once git has said.
    pub(super) fn file_diff(
        &self,
        workspace: WorkspaceId,
        part: ChangePart,
        file: String,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.changes_place(workspace) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        self.tasks.spawn(async move {
            let base = base(&place).await;
            let outcome = match changes::file_diff(&place.path, &base, part, &file).await {
                Ok(diff) => Ok(Reply::FileDiff(diff)),
                Err(error @ DiffError::NotChanged(_)) => {
                    Err(ErrorBody::new(ErrorCode::NotFound, error.to_string()))
                }
                Err(DiffError::Git(error)) => {
                    Err(ErrorBody::new(ErrorCode::Internal, error.to_string()))
                }
            };
            if let Err(error) = &outcome {
                tracing::info!(command = "file diff", %error, "command refused");
            }
            let _ = reply.send(outcome);
        });
    }

    /// The pull request timer's tick: a look at every workspace's changes,
    /// unless one is under way, or nobody looks at a client and it is not
    /// that tick's turn.
    pub(super) fn tick_changes(&mut self) {
        self.changes.ticks = self.changes.ticks.wrapping_add(1);
        if self.changes_under_way() {
            return;
        }
        if !self.state.looked_at && !self.changes.ticks.is_multiple_of(UNWATCHED_EVERY) {
            self.changes.missed = true;
            return;
        }
        self.look_at_changes(None);
    }

    /// The person looks at a client again: the look a tick let go by while
    /// nobody did is taken now, so what they see is recent.
    pub(super) fn catch_up_changes(&mut self) {
        if self.changes.missed && !self.changes_under_way() {
            self.look_at_changes(None);
        }
    }

    /// Whether a look at every workspace is under way, and not lost.
    fn changes_under_way(&self) -> bool {
        self.changes
            .under_way
            .is_some_and(|started| self.changes.ticks.wrapping_sub(started) < LOST_AFTER)
    }

    /// A session's status changed from `before` to `after`: once its
    /// agent stops working, what it changed is looked at.
    pub(super) fn after_turn(
        &mut self,
        session: SessionId,
        before: SessionStatus,
        after: SessionStatus,
    ) {
        let working = |status| {
            matches!(
                status,
                SessionStatus::Running | SessionStatus::NeedsYou { .. }
            )
        };
        if !working(before) || working(after) {
            return;
        }
        let workspace = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .map(|stored| stored.workspace);
        if let Some(workspace) = workspace {
            self.look_at_changes(Some(workspace));
        }
    }

    /// Looks at the changes in `only` that workspace, or in every one git
    /// can tell, off the loop.
    pub(super) fn look_at_changes(&mut self, only: Option<WorkspaceId>) {
        let places: Vec<Place> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| only.is_none_or(|only| only == workspace.id))
            .filter_map(|workspace| self.changes_place(workspace.id).ok())
            .collect();
        if places.is_empty() {
            return;
        }
        let whole = only.is_none();
        if whole {
            self.changes.under_way = Some(self.changes.ticks);
            self.changes.missed = false;
        }
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let mut looked = Looked {
                whole,
                found: Vec::new(),
            };
            for place in places {
                let base = base(&place).await;
                match changes::changes(&place.path, &base).await {
                    Ok(found) => {
                        looked
                            .found
                            .push((place.workspace, place.branch, found.summary()));
                    }
                    // Kept as it was: a slow or busy git says nothing new.
                    Err(error) => {
                        tracing::debug!(%error, "could not look at a workspace's changes");
                    }
                }
            }
            let _ = requests
                .send(Request::ChangesLooked(Box::new(looked)))
                .await;
        });
    }

    /// Keeps each summary found on its workspace while it is still on the
    /// branch it was found for, and tells clients of each change. Where
    /// the work meets others' is looked at again in each project whose
    /// work moved, and after a look at one workspace, which follows an
    /// agent's turn or a client's asking.
    pub(super) fn changes_looked(&mut self, looked: Looked) {
        if looked.whole {
            self.changes.under_way = None;
        }
        let mut moved = Vec::new();
        for (workspace, branch, summary) in looked.found {
            let Some(stored) = self
                .state
                .workspaces
                .iter_mut()
                .find(|candidate| candidate.id == workspace)
            else {
                continue;
            };
            if stored.kind.branch() != branch {
                continue;
            }
            if (stored.changes != summary || !looked.whole) && !moved.contains(&stored.project) {
                moved.push(stored.project);
            }
            if stored.changes == summary {
                continue;
            }
            stored.changes = summary;
            let updated = stored.clone();
            self.broadcast(Delta::WorkspaceUpserted(updated));
        }
        for project in moved {
            self.check_clashes(project);
        }
    }

    /// Where a workspace's files are, where git can tell what changed
    /// there.
    pub(super) fn repository_path(&self, workspace: WorkspaceId) -> Result<PathBuf, ErrorBody> {
        self.changes_place(workspace).map(|place| place.path)
    }

    /// Where git can tell a workspace's changes: a worktree's or a
    /// checkout's on the host, in a repository; not a run's container,
    /// whose clone is its own.
    pub(super) fn changes_place(&self, workspace: WorkspaceId) -> Result<Place, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        if !found.kind.in_git() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "that is a plain folder, not a git repository: git cannot tell what changed there",
            ));
        }
        let path = found
            .kind
            .host_path()
            .filter(|_| found.kind.run().is_none());
        let Some(path) = path else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "a run's container keeps its own clone; its changes are the run's",
            ));
        };
        Ok(Place {
            workspace,
            branch: found.kind.branch().to_owned(),
            path: path.to_owned(),
            overrides: self
                .config_dir
                .join("projects")
                .join(format!("{}.toml", found.project)),
            stacked: found.stacked_on.clone(),
        })
    }
}

/// What the workspace's branch is to merge into: the branch it is stacked
/// on, the base its settings name (the branch's own `muster.toml`, with
/// the person's overrides), or its remote's default branch. Settings that
/// do not read name none.
pub(super) async fn base(place: &Place) -> String {
    if let Some(stacked) = super::stacks::stacked_base(&place.path, place.stacked.as_ref()).await {
        return stacked;
    }
    let configured = by_hand::load_settings(place.path.clone(), place.overrides.clone())
        .await
        .ok()
        .and_then(|config| config.base_branch);
    by_hand::base_of(&place.path, configured.as_deref()).await
}
