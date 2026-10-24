//! A session the person asks for whose start first asks git or the
//! project's settings: the checks, a review by hand and a tidy of commits.
//! What the loop knows is checked on it; the asking, which takes a while in
//! a large repository, is done off it (`ARC-13`); and the session is added
//! once the answer is back, after what may have changed meanwhile is
//! checked again (`ARC-10`).

use std::future::Future;
use std::path::PathBuf;

use muster_core::protocol::{ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{SessionStatus, WorkspaceId};
use muster_state::session::Start;
use tokio::sync::oneshot;

use super::sessions::Mode;
use super::{Request, Supervisor};
use crate::harness::{Harness, Place};

/// Which of the sessions it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Purpose {
    Checks,
    Review,
    Tidy,
}

impl Purpose {
    fn name(self) -> &'static str {
        match self {
            Self::Checks => "the checks",
            Self::Review => "the review",
            Self::Tidy => "the tidy",
        }
    }
}

/// What the loop decided before asking: where the session runs, and as
/// what.
pub(super) struct Asked {
    pub(super) workspace: WorkspaceId,
    pub(super) harness: Harness,
    pub(super) place: Place,
    pub(super) size: TermSize,
    pub(super) title: Option<String>,
    pub(super) purpose: Purpose,
}

/// What the asking found: how the session's program starts, and for a
/// tidy what it began from.
pub(super) struct Ready {
    pub(super) mode: Mode,
    pub(super) tidy: Option<crate::tidy::Start>,
}

impl Ready {
    pub(super) fn with(mode: Mode) -> Self {
        Self { mode, tidy: None }
    }
}

/// A session's start, asked off the loop, on its way back with its answer.
pub(crate) struct Prepared {
    asked: Asked,
    ready: Result<Ready, ErrorBody>,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl Supervisor {
    /// Answers `reply` once `ask` has found how the session `asked` names
    /// starts; refused at once where the loop refused it.
    pub(super) fn prepare<F>(
        &self,
        asked: Result<(Asked, F), ErrorBody>,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) where
        F: Future<Output = Result<Ready, ErrorBody>> + Send + 'static,
    {
        let (asked, ask) = match asked {
            Ok(asked) => asked,
            Err(error) => {
                tracing::info!(%error, "a session by hand refused");
                let _ = reply.send(Err(error));
                return;
            }
        };
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let ready = ask.await;
            let prepared = Prepared {
                asked,
                ready,
                reply,
            };
            let _ = requests.send(Request::Prepared(Box::new(prepared))).await;
        });
    }

    /// A session's start came back: added and started, unless it was
    /// refused or something changed meanwhile that refuses it now.
    pub(super) async fn prepared(&mut self, prepared: Prepared) {
        let Prepared {
            asked,
            ready,
            reply,
        } = prepared;
        let purpose = asked.purpose;
        let started = match ready {
            Ok(ready) => self.start_prepared(asked, ready).await,
            Err(error) => Err(error),
        };
        if let Err(error) = &started {
            tracing::info!(what = purpose.name(), %error, "a session by hand refused");
        }
        let _ = reply.send(started);
    }

    async fn start_prepared(&mut self, asked: Asked, ready: Ready) -> Result<Reply, ErrorBody> {
        let Asked {
            workspace,
            harness,
            place,
            size,
            title,
            purpose,
        } = asked;
        // The workspace may have gone, or its worktree be on its way out.
        self.place_for(workspace, &harness)?;
        if purpose == Purpose::Tidy
            && let Some(busy) = self.tidy_refused(workspace)
        {
            return Err(busy);
        }
        let id = self
            .add_session(workspace, &harness, title, Start::WithPrompt)
            .await?;
        if let Err(error) = self.launch(
            id,
            harness,
            place.clone(),
            size,
            ready.mode,
            Start::WithPrompt,
            false,
        ) {
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {}: {error}", purpose.name()),
            ));
        }
        if let (Some(start), Place::Host(path)) = (ready.tidy, place) {
            self.began_tidy(id, path, start);
        }
        Ok(Reply::SessionStarted { id })
    }
}

impl Asked {
    /// The folder on the host the session runs in, or why there is none:
    /// `refusal` says where it must run.
    pub(super) fn host_path(&self, refusal: &str) -> Result<PathBuf, ErrorBody> {
        match &self.place {
            Place::Host(path) => Ok(path.clone()),
            Place::Container { .. } => Err(ErrorBody::new(ErrorCode::BadRequest, refusal)),
        }
    }
}
