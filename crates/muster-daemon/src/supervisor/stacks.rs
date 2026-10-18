//! Stacked work in the Supervisor (`PRODUCT.md` §6, `PRD-14`): a new
//! worktree's branch made on top of another workspace's, the branch it is
//! stacked on kept on the workspace and in the store; and, at the person's
//! request, the branch moved onto the newest of that branch, or of its
//! base, as a job on the workspace's timeline, which keeps what was there
//! first and is refused while an agent works there.

use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::stacks::Stacked;
use muster_core::{ProjectId, PullRequestState, WorkspaceId};
use tokio::sync::oneshot;

use super::timeline::{Job, JobDone, MOVE};
use super::{Request, Supervisor, by_hand};
use crate::git;
use crate::timeline::{MovePlan, Store, TimelineError};

impl Supervisor {
    /// What a new branch `branch` in `project` is stacked on when made on
    /// top of `on`'s: that branch, at its newest commit.
    pub(super) async fn stack_on(
        &self,
        project: ProjectId,
        on: WorkspaceId,
        branch: &str,
        exists: bool,
    ) -> Result<Stacked, ErrorBody> {
        let parent = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == on && candidate.project == project)
            .ok_or_else(|| {
                ErrorBody::new(
                    ErrorCode::NotFound,
                    format!("no workspace {on} in this project"),
                )
            })?;
        let parent_branch = parent.kind.branch().to_owned();
        let path = parent
            .kind
            .host_path()
            .filter(|_| parent.kind.in_git() && parent.kind.run().is_none())
            .ok_or_else(|| {
                ErrorBody::new(
                    ErrorCode::BadRequest,
                    "work is stacked only on a worktree's or the checkout's branch",
                )
            })?
            .to_owned();
        if parent_branch.is_empty() || parent_branch.starts_with("detached at ") {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("there is no branch checked out there to stack on ({parent_branch})"),
            ));
        }
        if exists {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{branch} exists already, so it cannot be made on top of {parent_branch}"),
            ));
        }
        let at = git::checked(
            &path,
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("refs/heads/{parent_branch}^{{commit}}"),
            ],
        )
        .await
        .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?;
        Ok(Stacked {
            branch: parent_branch,
            at,
        })
    }

    /// `MoveOnto`: refused while an agent works there, else done once the
    /// workspace's timeline is free.
    pub(super) async fn move_onto(
        &mut self,
        workspace: WorkspaceId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        match self.move_plan(workspace).await {
            Ok(plan) => self.queue_timeline(workspace, Job::Move { plan, reply }),
            Err(error) => {
                tracing::info!(command = "move onto", %error, "command refused");
                let _ = reply.send(Err(error));
            }
        }
    }

    /// What moving `workspace`'s branch knows: its base, the branch it is
    /// stacked on, and whether that branch's pull request has merged.
    async fn move_plan(&self, workspace: WorkspaceId) -> Result<MovePlan, ErrorBody> {
        let (path, _) = self.timeline_place(workspace)?;
        self.reviewable_branch(workspace)?;
        if let Some(busy) = self.busy_agent(workspace, MOVE) {
            return Err(busy);
        }
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        let stacked = found.stacked_on.clone();
        let parent_merged = stacked.as_ref().is_some_and(|stacked| {
            self.state.workspaces.iter().any(|other| {
                other.project == found.project
                    && other.kind.branch() == stacked.branch
                    && other
                        .pull_request
                        .as_ref()
                        .is_some_and(|pull| pull.state == PullRequestState::Merged)
            })
        });
        let configured = self.settings_in(workspace, &path).await?.base_branch;
        let base = by_hand::base_of(&path, configured.as_deref()).await;
        Ok(MovePlan {
            base,
            stacked,
            parent_merged,
        })
    }

    pub(super) fn run_move(
        &mut self,
        workspace: WorkspaceId,
        path: PathBuf,
        dir: PathBuf,
        plan: MovePlan,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let requests = self.requests.clone();
        let at = self.clock.now();
        self.tasks.spawn(async move {
            let outcome = match Store::open(&dir, &path).await {
                Ok(store) => store.move_onto(&plan, at).await,
                Err(error) => Err(error),
            };
            let recorded = outcome.as_ref().ok().map(|done| done.entry);
            if let Ok(done) = &outcome {
                let moved = Box::new((workspace, done.stacked.clone()));
                let _ = requests.send(Request::StackMoved(moved)).await;
            }
            let _ = reply.send(
                outcome
                    .map(|done| Reply::Moved {
                        onto: done.onto,
                        commits: done.commits,
                        entry: done.entry,
                    })
                    .map_err(|error| {
                        tracing::info!(command = "move onto", %error, "command refused");
                        move_error(&error)
                    }),
            );
            let done = JobDone {
                workspace,
                recorded,
                undid: recorded.is_some(),
            };
            let _ = requests
                .send(Request::TimelineJobDone(Box::new(done)))
                .await;
        });
    }

    /// The branch moved: what it is stacked on now is kept and told.
    pub(super) async fn stack_moved(&mut self, workspace: WorkspaceId, stacked: Option<Stacked>) {
        let Some(found) = self
            .state
            .workspaces
            .iter_mut()
            .find(|candidate| candidate.id == workspace)
        else {
            return;
        };
        if found.stacked_on == stacked {
            return;
        }
        found.stacked_on.clone_from(&stacked);
        let updated = found.clone();
        self.write(move |store| store.update_stack(workspace, stacked.as_ref()))
            .await;
        self.broadcast(Delta::WorkspaceUpserted(updated));
    }
}

/// The branch a workspace's work is measured against where it is stacked:
/// the branch it is stacked on, while the repository at `path` has it.
pub(super) async fn stacked_base(path: &Path, stacked: Option<&Stacked>) -> Option<String> {
    let stacked = stacked?;
    git::answer(
        path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("refs/heads/{}^{{commit}}", stacked.branch),
        ],
    )
    .await
    .ok()
    .flatten()
    .map(|_| stacked.branch.clone())
}

fn move_error(error: &TimelineError) -> ErrorBody {
    let code = match error {
        TimelineError::Git(_) | TimelineError::Io(..) => ErrorCode::Internal,
        _ => ErrorCode::Conflict,
    };
    ErrorBody::new(code, error.to_string())
}
