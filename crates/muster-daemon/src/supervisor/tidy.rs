//! Tidying a branch's commits (`PRODUCT.md` §6, `PRD-14`): at the
//! person's request, never while an agent works in the workspace, the
//! agent they pick is started there to rewrite the commits not pushed yet
//! into clear ones, the files left as they are. Its first turn is in the
//! workspace's timeline, which keeps the files as they were. Each time
//! its agent stops, Muster checks off the loop (`ARC-13`) that the files
//! are as they began and keeps what it found on the session
//! (`Session::tidy`, never stored). What a tidy began from goes with its
//! session (`ARC-14`).

use std::collections::HashMap;
use std::path::PathBuf;

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::tidy::{Tidy, TidyCheck};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use tokio::sync::oneshot;

use super::by_hand;
use super::prepared::{Asked, Purpose, Ready};
use super::sessions::Mode;
use super::timeline::working;
use super::{Request, Supervisor};
use crate::tidy::{self, TidyError};

/// What each tidy under way began from, and where.
#[derive(Debug, Default)]
pub(super) struct Tidies {
    began: HashMap<SessionId, (PathBuf, tidy::Start)>,
}

impl Supervisor {
    /// `TidyCommits`: starts `harness` tidying `workspace`'s branch's
    /// commits not pushed yet, with where they begin asked of git off the
    /// loop (`prepared`).
    pub(super) fn tidy_commits(
        &self,
        workspace: WorkspaceId,
        harness: &str,
        size: TermSize,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let asked = (|| {
            let harness = self
                .harness(harness)
                .filter(|found| found.is_agent() && found.takes_prompt() && found.id() != "review")
                .ok_or_else(|| {
                    ErrorBody::new(
                        ErrorCode::BadRequest,
                        format!("{harness:?} is not a coding agent that can tidy commits"),
                    )
                })?;
            let place = self.place_for(workspace, &harness)?;
            let asked = Asked {
                workspace,
                title: Some(format!("Tidy commits · {}", harness.label())),
                harness,
                place,
                size,
                purpose: Purpose::Tidy,
            };
            let path =
                asked.host_path("commits are tidied in a worktree or the project's checkout")?;
            let branch = self.reviewable_branch(workspace)?;
            if let Some(busy) = self.tidy_refused(workspace) {
                return Err(busy);
            }
            let (overrides, stacked) = self.base_asked(workspace)?;
            Ok((asked, async move {
                let base = by_hand::branch_base(&path, stacked.as_ref(), overrides).await?;
                let start = tidy::plan(&path, &base)
                    .await
                    .map_err(|error| match error {
                        TidyError::Git(error) => {
                            ErrorBody::new(ErrorCode::Internal, error.to_string())
                        }
                        refused => ErrorBody::new(ErrorCode::BadRequest, refused.to_string()),
                    })?;
                let prompt =
                    crate::runs::prompts::tidy_by_hand(&branch, &start.from, start.commits);
                Ok(Ready {
                    mode: Mode::Fresh {
                        prompt: Some(prompt),
                    },
                    tidy: Some(start),
                })
            }))
        })();
        self.prepare(asked, reply);
    }

    /// Why commits in `workspace` cannot be tidied now: an agent is at work
    /// there.
    pub(super) fn tidy_refused(&self, workspace: WorkspaceId) -> Option<ErrorBody> {
        let busy = self
            .state
            .sessions
            .iter()
            .find(|session| session.is_in(workspace) && working(session.status))?;
        Some(ErrorBody::new(
            ErrorCode::Conflict,
            format!(
                "{} is at work there: tidy the commits once it has stopped",
                busy.title
            ),
        ))
    }

    /// The tidy session `id`, started at `path`, began from `start`: kept,
    /// and shown on the session.
    pub(super) fn began_tidy(&mut self, id: SessionId, path: PathBuf, start: tidy::Start) {
        let tidy = Tidy {
            commits: start.commits,
            from: start.from.chars().take(7).collect(),
            checked: None,
        };
        self.tidies.began.insert(id, (path, start));
        if let Some(stored) = self.session_mut(id) {
            stored.tidy = Some(tidy);
            let updated = stored.clone();
            self.broadcast(Delta::SessionUpserted(updated));
        }
    }

    /// A session's status changed from `before` to `after`: a tidy whose
    /// agent stopped is checked, and one whose agent ended is let go.
    pub(super) fn tidy_after_status(
        &mut self,
        session: SessionId,
        before: SessionStatus,
        after: SessionStatus,
    ) {
        if after.is_exited() {
            self.tidies.began.remove(&session);
        }
        if !working(before) || working(after) {
            return;
        }
        let Some((path, start)) = self.tidies.began.get(&session).cloned() else {
            return;
        };
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let checked = match tidy::check(&path, &start).await {
                Ok(checked) => Some(checked),
                Err(error) => {
                    tracing::info!(%error, "could not check a tidy of commits");
                    None
                }
            };
            let _ = requests
                .send(Request::TidyChecked(Box::new((session, checked))))
                .await;
        });
    }

    /// What a check of a tidy found, kept on its session.
    pub(super) fn tidy_checked(&mut self, session: SessionId, checked: Option<TidyCheck>) {
        let Some(checked) = checked else {
            return;
        };
        let Some(stored) = self.session_mut(session) else {
            return;
        };
        let Some(tidy) = stored.tidy.as_mut() else {
            return;
        };
        if tidy.checked.as_ref() == Some(&checked) {
            return;
        }
        tidy.checked = Some(checked);
        let updated = stored.clone();
        self.broadcast(Delta::SessionUpserted(updated));
    }

    /// Lets go of tidies whose sessions are gone (`ARC-14`).
    pub(super) fn forget_tidies(&mut self) {
        let sessions = &self.state.sessions;
        self.tidies
            .began
            .retain(|id, _| sessions.iter().any(|session| session.id == *id));
    }
}
