//! What chats are called (`PRODUCT.md` §6): a session with no name of its
//! own (its agent's, as it started) is named by the first prompt it is
//! given, from the New session dialog or, where its agent reports the
//! prompts typed in its terminal, the first of those; and the person
//! renames any session. A name is cleaned and its credentials hidden
//! before it is stored or told (`muster_core::names`, `PRD-06`); the
//! prompt itself is neither.

use muster_core::SessionId;
use muster_core::names;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};

use super::{Supervisor, store_closed, store_error};

impl Supervisor {
    /// `RenameSession`: the session is called `title` from now on.
    pub(super) async fn rename_session(
        &mut self,
        session: SessionId,
        title: &str,
    ) -> Result<Reply, ErrorBody> {
        if !self
            .state
            .sessions
            .iter()
            .any(|stored| stored.id == session)
        {
            return Err(ErrorBody::new(
                ErrorCode::NotFound,
                format!("no session {session}"),
            ));
        }
        let title = names::given(title).ok_or_else(|| {
            ErrorBody::new(ErrorCode::BadRequest, "a session's name cannot be empty")
        })?;
        // Theirs, never replaced by an agent's.
        self.stop_naming(session);
        self.name_session(session, title).await
    }

    /// The person gave `session` the prompt `prompt`: where it is still
    /// called by its agent's name alone, it is named by the prompt, and
    /// then by an agent.
    pub(super) async fn name_from_prompt(&mut self, session: SessionId, prompt: &str) {
        let unnamed = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .is_some_and(|stored| {
                self.harness(&stored.harness)
                    .is_some_and(|found| stored.title == found.label())
            });
        if !unnamed {
            return;
        }
        let Some(title) = names::from_prompt(prompt) else {
            return;
        };
        match self.name_session(session, title).await {
            Ok(_) => self.ask_name(session, prompt),
            Err(error) => tracing::warn!(%error, "could not name a session by its first prompt"),
        }
    }

    /// Stores `session`'s name `title` and tells clients.
    pub(super) async fn name_session(
        &mut self,
        session: SessionId,
        title: String,
    ) -> Result<Reply, ErrorBody> {
        let stored_title = title.clone();
        self.store
            .call(move |store| store.rename_session(session, &stored_title))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        if let Some(stored) = self.session_mut(session) {
            stored.title = title;
            let updated = stored.clone();
            self.broadcast(Delta::SessionUpserted(updated));
        }
        Ok(Reply::Done)
    }
}
