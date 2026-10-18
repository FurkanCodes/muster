//! How sessions' statuses move: a hook is known by its token and fed to its
//! session's machine, whose decisions are stored, then told to clients.

use muster_core::SessionId;
use muster_core::SessionStatus;
use muster_core::protocol::Delta;
use muster_state::session::{Effect, Signal};

use super::Supervisor;
use crate::hooks::HookDelivery;

impl Supervisor {
    /// A hook arrived: the token says whose it is, never the payload.
    pub(super) async fn on_hook(&mut self, delivery: HookDelivery) {
        let Some(&session) = self.hook_tokens.get(&delivery.token) else {
            tracing::debug!("ignored a hook with an unknown token");
            return;
        };
        let Some(harness) = self.live.get(&session).map(|live| live.harness) else {
            return;
        };
        if harness.hook_route() != Some(delivery.harness.as_str()) {
            tracing::debug!("ignored a hook posted for another harness");
            return;
        }
        if let Some(spend) = harness.spend(&delivery.body) {
            self.set_spend(session, spend);
            return;
        }
        if let Some(turn) = harness.turn_begun(&delivery.body)
            && let Some(live) = self.live.get_mut(&session)
        {
            live.turn = Some(turn);
        }
        let event = delivery.event.as_deref();
        if let Some((agent_session, event)) = harness.translate(event, &delivery.body) {
            if let Some(change) = crate::harness::activity::change(&event, &delivery.body) {
                self.set_activity(session, change);
            }
            self.step_session(
                session,
                Signal::Hook {
                    agent_session,
                    event,
                },
            )
            .await;
        }
    }
    /// Turns the person cut short, as the agents' records say: each ends
    /// its session's turn if it is still the one under way, so a prompt
    /// declined, or a turn stopped, waits on nobody. A turn begun since,
    /// or before this daemon heard of any, is left as it is.
    pub(super) async fn record_interrupted(&mut self, turns: Vec<crate::spend::Interrupted>) {
        for interrupted in turns {
            let current = self
                .live
                .get(&interrupted.session)
                .and_then(|live| live.turn.as_deref());
            if current != Some(interrupted.prompt.as_str()) {
                continue;
            }
            self.set_activity(
                interrupted.session,
                crate::harness::activity::ActivityChange::Clear,
            );
            self.step_session(
                interrupted.session,
                Signal::Hook {
                    agent_session: Some(interrupted.agent_session),
                    event: muster_state::session::HookEvent::TurnInterrupted,
                },
            )
            .await;
        }
    }

    /// Feeds a session's machine and carries out what it decides.
    pub(super) async fn step_session(&mut self, session: SessionId, signal: Signal) {
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        let effects = live.machine.step(signal, self.clock.instant());
        for effect in effects {
            self.apply(session, effect).await;
        }
    }

    async fn apply(&mut self, session: SessionId, effect: Effect) {
        match effect {
            Effect::Status { status, unseen } => self.set_status(session, status, unseen).await,
            Effect::SaveResumeId(resume_id) => {
                let id = resume_id.clone();
                self.write(move |store| store.save_resume_id(session, &id))
                    .await;
                if let Some(stored) = self.session_mut(session) {
                    stored.resume_id = Some(resume_id);
                }
            }
        }
    }

    pub(super) async fn set_status(
        &mut self,
        session: SessionId,
        status: SessionStatus,
        unseen: bool,
    ) {
        let at = self.clock.now();
        self.write(move |store| store.record_status(session, status, unseen, at))
            .await;
        let mut since = at;
        let mut before = status;
        if let Some(stored) = self.session_mut(session) {
            before = stored.status;
            // Only a new status starts its clock again: the unseen mark
            // alone changing leaves it.
            if stored.status == status {
                since = stored.status_since;
            }
            stored.status = status;
            stored.unseen = unseen;
            stored.status_since = since;
        }
        self.broadcast(Delta::SessionStatus {
            session,
            status,
            unseen,
            since,
        });
        // Done working and not waiting: nothing is going on to tell.
        if !matches!(
            status,
            SessionStatus::Running | SessionStatus::NeedsYou { .. }
        ) {
            self.set_activity(session, crate::harness::activity::ActivityChange::Clear);
        }
        self.after_turn(session, before, status);
        self.timeline_after_status(session, before, status);
        self.tidy_after_status(session, before, status);
    }

    /// What a session is doing now, as its hooks tell; told to clients
    /// when it changes, and never stored.
    fn set_activity(
        &mut self,
        session: SessionId,
        change: crate::harness::activity::ActivityChange,
    ) {
        let now = self.clock.now();
        let Some(stored) = self.session_mut(session) else {
            return;
        };
        let text = match change {
            crate::harness::activity::ActivityChange::Set(text) => Some(text),
            crate::harness::activity::ActivityChange::Clear => None,
        };
        if stored.activity.as_ref().map(|activity| &activity.text) == text.as_ref() {
            return;
        }
        stored.activity = text.map(|text| muster_core::Activity { text, since: now });
        let activity = stored.activity.clone();
        self.broadcast(Delta::SessionActivity { session, activity });
    }
}
