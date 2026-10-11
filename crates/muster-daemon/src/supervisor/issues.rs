//! Working on GitHub issues (M1 step 8).
//!
//! Talking to GitHub takes a while, so it never happens inside the
//! Supervisor's loop: listing runs on its own task and answers directly;
//! starting work fetches the issue on its own task, then hands it back to
//! the Supervisor to create the worktree and the session.

use muster_core::ProjectId;
use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use tokio::sync::oneshot;

use super::{Request, Supervisor};
use crate::github::{self, GitHubError, IssueDetail};
use crate::harness;

/// An issue fetched for starting work, on its way back to the Supervisor.
#[derive(Debug)]
pub(super) struct FetchedIssue {
    pub(super) project: ProjectId,
    pub(super) harness: String,
    pub(super) size: TermSize,
    pub(super) detail: Result<IssueDetail, GitHubError>,
    pub(super) reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl Supervisor {
    /// Lists a project's issues on a task of its own.
    pub(super) fn list_issues(
        &self,
        project: ProjectId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let Some(root) = self.project_root(project) else {
            let _ = reply.send(Err(no_project(project)));
            return;
        };
        let tickets = self.tickets.clone();
        tokio::spawn(async move {
            let outcome = tickets
                .list(&root)
                .await
                .map(|issues| Reply::Issues { issues })
                .map_err(github_error);
            if let Err(error) = &outcome {
                tracing::info!(command = "list issues", %error, "command refused");
            }
            let _ = reply.send(outcome);
        });
    }

    /// Fetches an issue on a task of its own, then continues in
    /// [`start_fetched_issue`](Self::start_fetched_issue).
    pub(super) fn fetch_issue(
        &self,
        project: ProjectId,
        number: u64,
        harness: String,
        size: TermSize,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let Some(root) = self.project_root(project) else {
            let _ = reply.send(Err(no_project(project)));
            return;
        };
        if harness::find(&harness).is_none() {
            let error = ErrorBody::new(
                ErrorCode::BadRequest,
                format!("no harness called {harness:?}"),
            );
            let _ = reply.send(Err(error));
            return;
        }
        let (requests, tickets) = (self.requests.clone(), self.tickets.clone());
        tokio::spawn(async move {
            let detail = tickets.view(&root, number).await;
            let fetched = FetchedIssue {
                project,
                harness,
                size,
                detail,
                reply,
            };
            let _ = requests
                .send(Request::IssueFetched(Box::new(fetched)))
                .await;
        });
    }

    /// Creates or reuses the issue's worktree and starts a session in it.
    pub(super) async fn start_fetched_issue(&mut self, fetched: FetchedIssue) {
        let outcome = match fetched.detail {
            Ok(detail) => {
                self.start_issue(fetched.project, &detail, fetched.harness, fetched.size)
                    .await
            }
            Err(error) => Err(github_error(error)),
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
    ) -> Result<Reply, ErrorBody> {
        let branch = github::issue_branch(&detail.issue);
        let workspace = match self.worktree_on(project, &branch).map(|found| found.id) {
            Some(existing) => existing,
            None => match self.create_worktree(project, Some(branch)).await? {
                Reply::WorktreeCreated { id } => id,
                other => return Err(unexpected(&other)),
            },
        };
        let takes_prompt = harness::find(&harness).is_some_and(|found| found.takes_prompt());
        let prompt = match takes_prompt.then(github::new_fence).transpose() {
            Ok(fence) => fence.map(|fence| github::issue_prompt(detail, &fence)),
            Err(error) => {
                return Err(ErrorBody::new(
                    ErrorCode::Internal,
                    format!("could not prepare the prompt: {error}"),
                ));
            }
        };
        let title = format!("#{} {}", detail.issue.number, detail.issue.title);
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

fn github_error(error: GitHubError) -> ErrorBody {
    let code = match &error {
        GitHubError::Failed(message)
            if message.contains("Could not resolve") || message.contains("not found") =>
        {
            ErrorCode::NotFound
        }
        GitHubError::Missing | GitHubError::Failed(_) => ErrorCode::BadRequest,
        GitHubError::Spawn(_) | GitHubError::TimedOut | GitHubError::Unreadable(_) => {
            ErrorCode::Internal
        }
    };
    ErrorBody::new(code, error.to_string())
}
