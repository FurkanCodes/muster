//! The Supervisor's part in the daemon's restart in place (`handoff`):
//! handing every running session over, terminal and all, and taking
//! sessions over, from the daemon's previous version as it starts, or back
//! from a restart that did not happen.

use muster_core::{SessionId, SessionStatus};
use muster_state::session::SessionMachine;
use tokio::sync::oneshot;

use super::{Live, Request, Supervisor, SupervisorHandle};
use crate::handoff::{CarriedSession, HandedSession};
use crate::harness::Harness;
use crate::pty;

impl SupervisorHandle {
    /// Hands every running session over; the Supervisor no longer holds
    /// them, their programs running on.
    pub(crate) async fn hand_over(&self) -> Vec<HandedSession> {
        let (reply, answer) = oneshot::channel();
        if self.requests.send(Request::HandOver(reply)).await.is_err() {
            return Vec::new();
        }
        answer.await.unwrap_or_default()
    }

    /// Takes sessions back that were handed over and not carried.
    pub(crate) async fn adopt(&self, sessions: Vec<HandedSession>) {
        let (reply, answer) = oneshot::channel();
        if self
            .requests
            .send(Request::Adopt(sessions, reply))
            .await
            .is_ok()
        {
            let _ = answer.await;
        }
    }
}

impl Supervisor {
    /// Hands over every session whose program runs: its terminal, and its
    /// machine, token and history as carried. Sessions whose program has
    /// ended stay, as their output alone is left.
    pub(super) async fn hand_over_sessions(&mut self) -> Vec<HandedSession> {
        let now = self.clock.instant();
        let running: Vec<SessionId> = self
            .live
            .iter()
            .filter(|(_, live)| live.ended.is_none())
            .map(|(id, _)| *id)
            .collect();
        let mut handed = Vec::new();
        for id in running {
            let Some(pty) = self.live.get(&id).map(|live| live.pty.clone()) else {
                continue;
            };
            let Some(terminal) = pty.hand_over().await else {
                continue;
            };
            let Some(live) = self.live.get(&id) else {
                continue;
            };
            let mut session = CarriedSession::new(
                id,
                live.harness.id().to_owned(),
                live.token.clone(),
                now.saturating_duration_since(live.launched),
                live.resumed,
                live.machine.carry(now),
                terminal.terminal,
            );
            session.turn.clone_from(&live.turn);
            session.agent = live.agent.as_ref().map(|agent| agent.id().to_owned());
            // Its hooks and terminal are the next daemon's now.
            self.release_terminal(id);
            handed.push(HandedSession {
                session,
                master: terminal.master,
            });
        }
        handed
    }

    /// Takes over sessions handed over: each one this daemon knows as
    /// running gets its terminal, machine and hook token back, its program
    /// never having stopped. One it does not know is let go: closing its
    /// terminal hangs its program up.
    pub(super) fn adopt_sessions(&mut self, sessions: Vec<HandedSession>) {
        let now = self.clock.instant();
        for HandedSession { session, master } in sessions {
            let id = session.id;
            let known = self
                .state
                .sessions
                .iter()
                .any(|stored| stored.id == id && stored.ended_at.is_none());
            let Some(harness) = known.then(|| self.running_harness(&session.harness)) else {
                tracing::warn!(session = %id, "a session handed over is not known here; let go");
                continue;
            };
            // The agent in front in a terminal, as the daemon before found it.
            let agent = session
                .agent
                .as_deref()
                .and_then(|id| self.harness(id))
                .filter(Harness::is_agent);
            let events = self.pty_events.clone();
            let size = session.terminal.size;
            let pty = match pty::adopt(master, session.terminal, self.colours(), move |event| {
                let _ = events.send((id, event));
            }) {
                Ok(pty) => pty,
                Err(error) => {
                    tracing::warn!(session = %id, %error, "could not take a session's terminal over");
                    continue;
                }
            };
            if let Some(token) = &session.token {
                self.hook_tokens.insert(token.clone(), id);
            }
            // A finish still in its hold as the daemon before let go of it.
            let finished = (session.machine.status == SessionStatus::Finished)
                .then_some(session.machine.finished)
                .flatten();
            self.live.insert(
                id,
                Live {
                    pty: pty.clone(),
                    machine: SessionMachine::adopt(session.machine, self.timing, now),
                    harness,
                    token: session.token,
                    launched: now.checked_sub(session.running_for).unwrap_or(now),
                    resumed: session.resumed,
                    ended: None,
                    title: None,
                    turn: session.turn,
                    // Not carried: what this daemon has heard.
                    signals: super::explain::Recent::default(),
                    naming: None,
                    named_by_person: false,
                    last_reply: None,
                    agent: agent.clone(),
                },
            );
            if let Some(agent) = agent {
                let since = self.clock.now();
                if let Some(stored) = self.session_mut(id) {
                    stored.agent = Some(muster_core::AgentNow {
                        harness: agent.id().to_owned(),
                        since,
                    });
                }
            }
            if let Some(ago) = finished {
                self.hold_carried_finish(id, ago);
            }
            // Told it is out of focus by a daemon before it stayed in focus,
            // it is in focus again.
            if self
                .live
                .get(&id)
                .is_some_and(|live| live.harness.stays_in_focus())
            {
                pty.focus_now(true);
            }
            self.carry_watch(id);
            self.publish_terminal(id, pty, size);
        }
    }
}
