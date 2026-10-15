//! Working on tickets: the ones an agent is assigned to, from each
//! project's tracker (`trackers`).
//!
//! Talking to a tracker takes a while, so it never happens inside the
//! Supervisor's loop: listing and viewing run on a task of their own and
//! answer directly; starting work fetches the ticket on its own task, then
//! hands it back to the Supervisor to create the worktree and the session.
//! The first agent on a ticket works in its worktree (made, or found as it
//! is); another works in a worktree of its own, `…-2`, `…-3`.

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::tickets::ticket_branch;
use muster_core::{ProjectId, TicketKey};
use tokio::sync::oneshot;

use super::{Request, Supervisor};
use crate::harness;
use crate::trackers::{self, IssueDetail, ProjectPlace, ServiceError};

/// A ticket fetched for starting work, on its way back to the Supervisor.
#[derive(Debug)]
pub(super) struct FetchedIssue {
    pub(super) project: ProjectId,
    pub(super) harness: String,
    pub(super) size: TermSize,
    pub(super) another: bool,
    /// Whether its branch starts with `muster/`, as the project says.
    pub(super) prefixed: bool,
    pub(super) detail: Result<IssueDetail, ServiceError>,
    pub(super) reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl Supervisor {
    /// Where the project is, for finding its tracker.
    pub(super) fn tracker_place(&self, project: ProjectId) -> Result<ProjectPlace, ErrorBody> {
        let root = self
            .project_root(project)
            .ok_or_else(|| no_project(project))?;
        Ok(ProjectPlace {
            root,
            overrides: self
                .config_dir
                .join("projects")
                .join(format!("{project}.toml")),
        })
    }

    /// Lists a project's tickets on a task of its own.
    pub(super) fn list_issues(
        &self,
        project: ProjectId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let trackers = self.trackers.clone();
        tokio::spawn(async move {
            let outcome = async {
                let source = trackers.for_project(place).await?;
                let issues = source.list().await?;
                Ok::<_, ServiceError>(Reply::Issues {
                    issues,
                    tracker: source.describe(),
                    mine: source.only_mine(),
                })
            }
            .await
            .map_err(|error| error.body());
            if let Err(error) = &outcome {
                tracing::info!(command = "list issues", %error, "command refused");
            }
            let _ = reply.send(outcome);
        });
    }

    /// Fetches one ticket with its text, on a task of its own.
    pub(super) fn view_issue(
        &self,
        project: ProjectId,
        key: TicketKey,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let trackers = self.trackers.clone();
        tokio::spawn(async move {
            let outcome = async {
                let detail = trackers.for_project(place).await?.view(&key).await?;
                Ok::<_, ServiceError>(Reply::IssueViewed {
                    issue: detail.issue,
                    body: detail.body,
                })
            }
            .await
            .map_err(|error| error.body());
            let _ = reply.send(outcome);
        });
    }

    /// Closes a ticket on a task of its own, as the person asked.
    pub(super) fn close_issue(
        &self,
        project: ProjectId,
        key: TicketKey,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let trackers = self.trackers.clone();
        tokio::spawn(async move {
            let outcome = async {
                trackers.for_project(place).await?.close(&key).await?;
                Ok::<_, ServiceError>(Reply::Done)
            }
            .await
            .map_err(|error| error.body());
            match &outcome {
                Ok(_) => tracing::info!(%project, %key, "closed a ticket"),
                Err(error) => tracing::info!(command = "close issue", %error, "command refused"),
            }
            let _ = reply.send(outcome);
        });
    }

    /// Fetches a ticket on a task of its own, then continues in
    /// [`start_fetched_issue`](Self::start_fetched_issue).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn fetch_issue(
        &self,
        project: ProjectId,
        key: TicketKey,
        harness: String,
        size: TermSize,
        another: bool,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        // Work on a ticket goes in a worktree of its own.
        let place = match self
            .repository_root(project, "worktrees for tickets")
            .and_then(|_| self.tracker_place(project))
        {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        if harness::find(&harness).is_none() {
            let error = ErrorBody::new(
                ErrorCode::BadRequest,
                format!("no harness called {harness:?}"),
            );
            let _ = reply.send(Err(error));
            return;
        }
        let (requests, trackers) = (self.requests.clone(), self.trackers.clone());
        tokio::spawn(async move {
            let prefixed =
                crate::config::muster_prefix(place.root.clone(), place.overrides.clone()).await;
            let detail = async { trackers.for_project(place).await?.view(&key).await }.await;
            let fetched = FetchedIssue {
                project,
                harness,
                size,
                another,
                prefixed,
                detail,
                reply,
            };
            let _ = requests
                .send(Request::IssueFetched(Box::new(fetched)))
                .await;
        });
    }

    /// Creates or reuses the ticket's worktree and starts a session in it.
    pub(super) async fn start_fetched_issue(&mut self, fetched: FetchedIssue) {
        let outcome = match fetched.detail {
            Ok(detail) => {
                let FetchedIssue {
                    project,
                    harness,
                    size,
                    another,
                    prefixed,
                    ..
                } = fetched;
                self.start_issue(project, &detail, harness, size, another, prefixed)
                    .await
            }
            Err(error) => Err(error.body()),
        };
        if let Err(error) = &outcome {
            tracing::info!(command = "start from issue", %error, "command refused");
        }
        let _ = fetched.reply.send(outcome);
    }

    async fn start_issue(
        &mut self,
        project: ProjectId,
        detail: &IssueDetail,
        harness: String,
        size: TermSize,
        another: bool,
        prefixed: bool,
    ) -> Result<Reply, ErrorBody> {
        let branch = ticket_branch(&detail.issue.key, &detail.issue.title, prefixed);
        let existing = self.worktree_on(project, &branch).map(|found| found.id);
        let workspace = match (existing, another) {
            (Some(existing), false) => existing,
            (None, false) => self.new_worktree(project, branch).await?,
            (_, true) => {
                let branch = self.unused_worktree_branch(project, &branch).await?;
                self.new_worktree(project, branch).await?
            }
        };
        let takes_prompt = harness::find(&harness).is_some_and(|found| found.takes_prompt());
        let prompt = match takes_prompt.then(trackers::new_fence).transpose() {
            Ok(fence) => fence.map(|fence| trackers::issue_prompt(detail, &fence)),
            Err(error) => {
                return Err(ErrorBody::new(
                    ErrorCode::Internal,
                    format!("could not prepare the prompt: {error}"),
                ));
            }
        };
        let title = format!("{} {}", detail.issue.key.shown(), detail.issue.title);
        match self
            .start_session(workspace, harness, Some(title), size, prompt)
            .await?
        {
            Reply::SessionStarted { id } => Ok(Reply::IssueStarted {
                workspace,
                session: id,
            }),
            other => Err(unexpected(&other)),
        }
    }

    async fn new_worktree(
        &mut self,
        project: ProjectId,
        branch: String,
    ) -> Result<muster_core::WorkspaceId, ErrorBody> {
        match self.create_worktree(project, Some(branch)).await? {
            Reply::WorktreeCreated { id } => Ok(id),
            other => Err(unexpected(&other)),
        }
    }

    /// `branch-2`, `-3`… : the first no workspace has and the repository
    /// does not have yet, for another agent's worktree.
    async fn unused_worktree_branch(
        &self,
        project: ProjectId,
        branch: &str,
    ) -> Result<String, ErrorBody> {
        let root = self
            .project_root(project)
            .ok_or_else(|| no_project(project))?;
        for n in 2..100 {
            let candidate = format!("{branch}-{n}");
            let in_use = self.state.workspaces.iter().any(|workspace| {
                workspace.project == project && workspace.kind.branch() == candidate
            });
            let exists = crate::git::branch_exists(&root, &candidate)
                .await
                .unwrap_or(true);
            if !in_use && !exists {
                return Ok(candidate);
            }
        }
        Err(ErrorBody::new(
            ErrorCode::Conflict,
            format!("{branch} already has 99 worktrees"),
        ))
    }

    pub(super) fn project_root(&self, project: ProjectId) -> Option<std::path::PathBuf> {
        self.state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .map(|found| found.root.clone())
    }
}

fn no_project(project: ProjectId) -> ErrorBody {
    ErrorBody::new(ErrorCode::NotFound, format!("no project {project}"))
}

fn unexpected(reply: &Reply) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, format!("unexpected {reply:?}"))
}
