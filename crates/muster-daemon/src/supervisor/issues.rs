//! Working on tickets: the ones an agent is assigned to, from each
//! project's tracker (`trackers`).
//!
//! Talking to a tracker takes a while, so it never happens inside the
//! Supervisor's loop: listing and viewing run on a task of their own and
//! answer directly; starting work fetches the ticket on its own task, then
//! hands it back to the Supervisor to create the worktree, which git makes
//! off the loop too (`worktrees`), and then the session.
//! The first agent on a ticket works in its worktree (made, or found as it
//! is); another works in a worktree of its own, `…-2`, `…-3`.

use std::sync::Arc;

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::tickets::ticket_branch;
use muster_core::{ProjectId, TicketBrief, TicketKey, WorkspaceId};
use tokio::sync::oneshot;

use super::{Request, Supervisor, worktrees};
use crate::trackers::{self, IssueDetail, ProjectPlace, ServiceError, TicketSource};

/// A ticket fetched for starting work, on its way back to the Supervisor.
#[derive(Debug)]
pub(super) struct FetchedIssue {
    pub(super) project: ProjectId,
    pub(super) harness: String,
    pub(super) size: TermSize,
    pub(super) another: bool,
    pub(super) on: Option<muster_core::WorkspaceId>,
    /// Whether its branch starts with `muster/`, as the project says.
    pub(super) prefixed: bool,
    pub(super) brief: TicketBrief,
    pub(super) detail: Result<IssueDetail, ServiceError>,
    pub(super) reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

/// A ticket's agent waiting for its worktree to be made off the loop
/// (`worktrees::Then`).
pub(super) struct TicketThen {
    detail: IssueDetail,
    /// What of it the agent is given.
    brief: TicketBrief,
    harness: String,
    size: TermSize,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl TicketThen {
    /// The ticket's key.
    pub(super) fn key(&self) -> &TicketKey {
        &self.detail.issue.key
    }

    pub(super) fn refuse(self, error: ErrorBody) {
        refuse(self.reply, error);
    }
}

/// Work on a ticket the person asked for (`Command::StartFromIssue`).
pub(super) struct TicketStart {
    pub(super) project: ProjectId,
    pub(super) key: TicketKey,
    /// A harness id, such as `claude` or `shell`.
    pub(super) harness: String,
    pub(super) size: TermSize,
    /// Another agent on it, in a worktree of its own.
    pub(super) another: bool,
    /// The workspace whose branch a worktree made for it is made on top
    /// of.
    pub(super) on: Option<muster_core::WorkspaceId>,
    /// What of it the agent is given: its text, and the comments chosen.
    pub(super) brief: TicketBrief,
}

impl Supervisor {
    /// Where the project is, for finding its tracker.
    pub(super) fn tracker_place(&self, project: ProjectId) -> Result<ProjectPlace, ErrorBody> {
        let root = self
            .project_root(project)
            .ok_or_else(|| no_project(project))?;
        Ok(ProjectPlace {
            root,
            overrides: self.overrides_of(project),
        })
    }

    /// Lists a project's tickets on a task of its own.
    pub(super) fn list_issues(
        &self,
        project: ProjectId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.ask_tickets(project, "list issues", reply, |source| async move {
            let issues = source.list().await?;
            Ok(Reply::Issues {
                issues,
                tracker: source.describe(),
                mine: source.only_mine(),
                source: source.source(),
            })
        });
    }

    /// Fetches one ticket with its text, on a task of its own.
    pub(super) fn view_issue(
        &self,
        project: ProjectId,
        key: TicketKey,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.ask_tickets(project, "view issue", reply, move |source| async move {
            let detail = source.view(&key).await?;
            Ok(Reply::IssueViewed {
                issue: detail.issue,
                body: detail.body,
                comments: detail.comments,
            })
        });
    }

    /// Closes a ticket on a task of its own, as the person asked.
    pub(super) fn close_issue(
        &self,
        project: ProjectId,
        key: TicketKey,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.ask_tickets(project, "close issue", reply, move |source| async move {
            source.close(&key).await?;
            tracing::info!(%project, %key, "closed a ticket");
            Ok(Reply::Done)
        });
    }

    /// Answers `reply` with what `ask` makes of `project`'s ticket source,
    /// on a task of its own, as trackers are slow; `command` names it in
    /// the log where it is refused.
    fn ask_tickets<F>(
        &self,
        project: ProjectId,
        command: &'static str,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
        ask: impl FnOnce(Arc<dyn TicketSource>) -> F + Send + 'static,
    ) where
        F: Future<Output = Result<Reply, ServiceError>> + Send,
    {
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let trackers = self.systems.trackers.clone();
        self.tasks.spawn(async move {
            let outcome = async { ask(trackers.for_project(place).await?).await }
                .await
                .map_err(|error| error.body());
            if let Err(error) = &outcome {
                tracing::info!(command, %error, "command refused");
            }
            let _ = reply.send(outcome);
        });
    }

    /// Fetches a ticket on a task of its own, then continues in
    /// [`start_fetched_issue`](Self::start_fetched_issue).
    pub(super) fn fetch_issue(
        &self,
        TicketStart {
            project,
            key,
            harness,
            size,
            another,
            on,
            brief,
        }: TicketStart,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        if brief.is_empty() {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "give the agent the ticket's text or a comment on it",
            )));
            return;
        }
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
        if self.harness(&harness).is_none() {
            let _ = reply.send(Err(self.no_harness(&harness)));
            return;
        }
        let (requests, trackers) = (self.requests.clone(), self.systems.trackers.clone());
        self.tasks.spawn(async move {
            let prefixed =
                crate::config::muster_prefix(place.root.clone(), place.overrides.clone()).await;
            let detail = async { trackers.for_project(place).await?.view(&key).await }.await;
            let fetched = FetchedIssue {
                project,
                harness,
                size,
                another,
                on,
                prefixed,
                brief,
                detail,
                reply,
            };
            let _ = requests
                .send(Request::IssueFetched(Box::new(fetched)))
                .await;
        });
    }

    /// Starts the ticket's agent in its worktree, found as it is, or made
    /// first (another agent's always is).
    pub(super) async fn start_fetched_issue(&mut self, fetched: FetchedIssue) {
        let FetchedIssue {
            project,
            harness,
            size,
            another,
            on,
            prefixed,
            brief,
            detail,
            reply,
        } = fetched;
        let detail = match detail {
            Ok(detail) => detail,
            Err(error) => return refuse(reply, error.body()),
        };
        // The comments chosen are those the person saw: one gone since
        // is said, not a comment put in its place.
        if brief
            .comments
            .iter()
            .any(|at| usize::try_from(*at).map_or(true, |at| at >= detail.comments.len()))
        {
            return refuse(
                reply,
                ErrorBody::new(
                    ErrorCode::Conflict,
                    format!(
                        "{} has fewer comments than when you chose them: look at it again",
                        detail.issue.key.shown()
                    ),
                ),
            );
        }
        let branch = ticket_branch(&detail.issue.key, &detail.issue.title, prefixed);
        // Its worktree, made for it, whatever its branch is called now.
        let existing = self
            .state
            .workspaces
            .iter()
            .find(|workspace| {
                workspace.project == project
                    && workspace.kind.is_worktree()
                    && workspace.ticket.as_ref() == Some(&detail.issue.key)
            })
            .map(|found| found.id);
        let ticket = TicketThen {
            detail,
            brief,
            harness,
            size,
            reply,
        };
        let branch = match (existing, another) {
            (Some(existing), false) => return self.start_ticket_agent(existing, ticket).await,
            (None, false) => worktrees::Branch::Named(branch),
            (_, true) => worktrees::Branch::Another(branch),
        };
        let then = worktrees::Then::Ticket(Box::new(ticket));
        self.create_worktree(project, branch, on, then);
    }

    /// Starts the ticket's agent in `workspace`, with the ticket as its
    /// first prompt where it takes one, and answers.
    pub(super) async fn start_ticket_agent(&mut self, workspace: WorkspaceId, ticket: TicketThen) {
        let TicketThen {
            detail,
            brief,
            harness,
            size,
            reply,
        } = ticket;
        match self
            .ticket_agent(workspace, &detail, &brief, harness, size)
            .await
        {
            Ok(started) => {
                let _ = reply.send(Ok(started));
            }
            Err(error) => refuse(reply, error),
        }
    }

    async fn ticket_agent(
        &mut self,
        workspace: WorkspaceId,
        detail: &IssueDetail,
        brief: &TicketBrief,
        harness: String,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let takes_prompt = self
            .harness(&harness)
            .is_some_and(|found| found.takes_prompt());
        let prompt = match takes_prompt.then(trackers::new_fence).transpose() {
            Ok(fence) => fence.map(|fence| trackers::issue_prompt(detail, brief, &fence)),
            Err(error) => {
                return Err(ErrorBody::new(
                    ErrorCode::Internal,
                    format!("could not prepare the prompt: {error}"),
                ));
            }
        };
        let title = format!("{} {}", detail.issue.key.shown(), detail.issue.title);
        match self
            .start_session(
                Some(workspace),
                harness,
                size,
                super::sessions::Asked::new(Some(title), prompt, None),
            )
            .await?
        {
            Reply::SessionStarted { id } => {
                self.ticket_assigned(id, &detail.issue.key);
                Ok(Reply::IssueStarted {
                    workspace,
                    session: id,
                })
            }
            other => Err(unexpected(&other)),
        }
    }

    /// Makes `workspace`, a worktree, the one `key` is worked on in, or no
    /// ticket's (`Command::AssignTicket`), as the person asked.
    pub(super) async fn assign_ticket(
        &mut self,
        workspace: WorkspaceId,
        key: Option<muster_core::TicketKey>,
    ) -> Result<Reply, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, "no such workspace"))?;
        if !found.kind.is_worktree() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "a ticket is worked on in a worktree; this is the project's own checkout",
            ));
        }
        if let Some(key) = &key
            && let Some(other) = self.state.workspaces.iter().find(|other| {
                other.id != workspace
                    && other.project == found.project
                    && other.ticket.as_ref() == Some(key)
            })
        {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!(
                    "{} is worked on in {} already",
                    key.shown(),
                    other.kind.branch()
                ),
            ));
        }
        let stored = key.as_ref().map(|key| key.as_str().to_owned());
        self.write(move |store| store.update_ticket(workspace, stored.as_deref()))
            .await;
        let Some(found) = self
            .state
            .workspaces
            .iter_mut()
            .find(|candidate| candidate.id == workspace)
        else {
            return Err(ErrorBody::new(ErrorCode::NotFound, "no such workspace"));
        };
        found.ticket = key;
        let updated = found.clone();
        self.broadcast(muster_core::protocol::Delta::WorkspaceUpserted(updated));
        Ok(Reply::Done)
    }

    pub(super) fn project_root(&self, project: ProjectId) -> Option<std::path::PathBuf> {
        self.state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .map(|found| found.root.clone())
    }
}

/// Answers a start from a ticket with `error`, logged.
fn refuse(reply: oneshot::Sender<Result<Reply, ErrorBody>>, error: ErrorBody) {
    tracing::info!(command = "start from issue", %error, "command refused");
    let _ = reply.send(Err(error));
}

fn no_project(project: ProjectId) -> ErrorBody {
    ErrorBody::new(ErrorCode::NotFound, format!("no project {project}"))
}

fn unexpected(reply: &Reply) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, format!("unexpected {reply:?}"))
}
