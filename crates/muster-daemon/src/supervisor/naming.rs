//! Chats named by an agent (`PRODUCT.md` §6): a chat named by its first
//! prompt is named again by the few words an agent writes from it, asked
//! as one question (`crate::ask`) of the session's own agent or the one
//! the person picked, with the model they picked, unless they turned it
//! off. A prompt of a few words is named once more as the first turn ends,
//! from the prompt and the start of the answer, where the agent reports
//! it. A name the person gives ends the naming, so theirs is never
//! replaced. The prompt is kept in memory only while the chat is named,
//! its credentials hidden (`PRD-06`); neither it nor an answer is logged.

use muster_core::ask::Ask;
use muster_core::protocol::{ErrorBody, ErrorCode, Reply};
use muster_core::{SessionId, credentials, names};
use tokio::sync::oneshot;

use super::{Request, Supervisor};
use crate::harness::Harness;

/// How many agents are asked at once, all sessions together (`ARC-14`).
pub(super) const AT_ONCE: usize = 2;

/// A chat being named.
#[derive(Debug)]
pub(super) struct Asking {
    /// Its first prompt, credentials hidden.
    prompt: String,
    /// The question out: 1 from the prompt, 2 after the first turn.
    round: u8,
}

/// An agent's answer, back from off the loop.
#[derive(Debug)]
pub(crate) struct Named {
    session: SessionId,
    round: u8,
    outcome: Result<String, String>,
}

impl Supervisor {
    /// The agent that names `harness`'s chats, and the model it is asked
    /// with, as the person's settings say; `None` where none does.
    fn namer(&self, harness: &Harness) -> Option<(Ask, Option<String>)> {
        let naming = &self.state.preferences.naming;
        if !naming.by_agent {
            return None;
        }
        match &naming.agent {
            Some(agent) => Some((self.harness(agent)?.asking()?, naming.model.clone())),
            None => Some((harness.asking()?, None)),
        }
    }

    /// `session` was named by its first prompt, `prompt`: an agent is asked
    /// for a better name.
    pub(super) fn ask_name(&mut self, session: SessionId, prompt: &str) {
        let (prompt, _) = credentials::hide(prompt);
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        live.naming = Some(Asking { prompt, round: 1 });
        self.ask_round(session, None);
    }

    /// A turn of `session` ended with `answer`: a chat whose first prompt
    /// said too little is named again, once, from the answer too.
    pub(super) fn name_after_turn(&mut self, session: SessionId, answer: &str) {
        let Some(asking) = self
            .live
            .get_mut(&session)
            .and_then(|live| live.naming.as_mut())
        else {
            return;
        };
        if asking.round != 1 || !names::too_short(&asking.prompt) {
            return;
        }
        asking.round = 2;
        self.ask_round(session, Some(answer));
    }

    /// The person named `session`: no agent's name replaces theirs.
    pub(super) fn stop_naming(&mut self, session: SessionId) {
        if let Some(live) = self.live.get_mut(&session) {
            live.naming = None;
        }
    }

    /// Asks the namer for `session`'s name, in the round its naming is at.
    fn ask_round(&mut self, session: SessionId, answer: Option<&str>) {
        let Some(live) = self.live.get(&session) else {
            return;
        };
        let (Some(asking), Some((spec, model))) = (&live.naming, self.namer(&live.harness)) else {
            self.stop_naming(session);
            return;
        };
        let round = asking.round;
        let question = names::question(&asking.prompt, answer);
        let folder = crate::ask::folder(&self.runtime_dir());
        let requests = self.requests.clone();
        let asks = self.asks.clone();
        self.tasks.spawn(async move {
            let Ok(_turn) = asks.acquire_owned().await else {
                return;
            };
            let outcome = crate::ask::ask(&spec, model.as_deref(), &question, &folder)
                .await
                .map_err(|error| error.to_string());
            let named = Named {
                session,
                round,
                outcome,
            };
            let _ = requests.send(Request::Named(Box::new(named))).await;
        });
    }

    /// An agent answered: its name is the chat's, unless the person named
    /// it since or another question has gone out.
    pub(super) async fn named(&mut self, named: Named) {
        let Some(asking) = self
            .live
            .get(&named.session)
            .and_then(|live| live.naming.as_ref())
        else {
            return;
        };
        if asking.round != named.round {
            return;
        }
        if named.round > 1 || !names::too_short(&asking.prompt) {
            self.stop_naming(named.session);
        }
        match named.outcome.as_deref().map(names::from_answer) {
            Ok(Some(name)) => {
                if let Err(error) = self.name_session(named.session, name).await {
                    tracing::warn!(%error, "could not keep the name an agent gave a chat");
                }
            }
            // Said without the prompt or the answer (`PRD-06`).
            Ok(None) => tracing::info!("an agent's answer was no chat's name"),
            Err(error) => tracing::info!(%error, "an agent did not name a chat"),
        }
    }

    /// `ListModels`: the models `agent` lists, or those Muster suggests for
    /// it, answered from a task of its own.
    pub(super) fn list_models(
        &self,
        agent: &str,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let Some(spec) = self.harness(agent).and_then(|found| found.asking()) else {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::NotFound,
                format!("{agent} cannot be asked to name chats"),
            )));
            return;
        };
        let folder = crate::ask::folder(&self.runtime_dir());
        self.tasks.spawn(async move {
            let outcome = match crate::ask::models(&spec, &folder).await {
                None => Ok(Reply::Models(spec.suggestions.clone())),
                Some(Ok(listed)) => Ok(Reply::Models(listed)),
                Some(Err(error)) => Err(ErrorBody::new(
                    ErrorCode::Internal,
                    format!("could not list its models: {error}"),
                )),
            };
            let _ = reply.send(outcome);
        });
    }

    /// Muster's runtime directory, where the hooks' socket is.
    pub(super) fn runtime_dir(&self) -> std::path::PathBuf {
        self.hook_socket
            .parent()
            .map_or_else(std::path::PathBuf::new, std::path::Path::to_path_buf)
    }
}
