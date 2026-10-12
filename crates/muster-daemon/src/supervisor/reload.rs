//! The Supervisor's part in the daemon's restart in place (`handoff`):
//! handing every running session over, terminal and all, and taking
//! sessions over, from the daemon's previous version as it starts, or back
//! from a restart that did not happen.

use std::time::Instant;

use muster_core::SessionId;
use muster_state::session::SessionMachine;
use tokio::sync::oneshot;

use super::{Live, Request, Supervisor, SupervisorHandle};
use crate::handoff::{CarriedSession, HandedSession};
use crate::{harness, pty};

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
        let now = Instant::now();
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
            let session = CarriedSession::new(
                id,
                live.harness.id.to_owned(),
                live.token.clone(),
                live.launched.elapsed(),
                live.resumed,
                live.machine.carry(now),
                terminal.terminal,
            );
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
        let now = Instant::now();
        for HandedSession { session, master } in sessions {
            let id = session.id;
            let known = self
                .state
                .sessions
                .iter()
                .any(|stored| stored.id == id && stored.ended_at.is_none());
            let Some(harness) = harness::find(&session.harness).filter(|_| known) else {
                tracing::warn!(session = %id, "a session handed over is not known here; let go");
                continue;
            };
            let events = self.pty_events.clone();
            let pty = match pty::adopt(master, session.terminal, move |event| {
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
                },
            );
            self.carry_watch(id);
            self.publish_terminal(id, pty);
        }
    }
}
