//! What each session's agent has used (`PRD-12`): the sessions whose
//! records the reader reads (`crate::spend`), and the totals it, or
//! `OpenCode`'s plugin, hands back, kept on the session for clients and
//! never stored.

use muster_core::protocol::Delta;
use muster_core::{SessionId, Spend};

use super::Supervisor;
use crate::spend::{Target, readable};

impl Supervisor {
    /// Every session whose agent keeps records Muster can read, once a turn
    /// has given it its own id: ended ones too, read once after a restart.
    pub(super) fn spend_targets(&self) -> Vec<Target> {
        self.state
            .sessions
            .iter()
            .filter(|session| readable(&session.harness))
            .filter_map(|session| {
                Some(Target {
                    session: session.id,
                    harness: session.harness.clone(),
                    agent_session: session.resume_id.clone()?,
                })
            })
            .collect()
    }

    /// Keeps each total that changed and tells clients.
    pub(super) fn record_spends(&mut self, spends: Vec<(SessionId, Spend)>) {
        for (session, spend) in spends {
            self.set_spend(session, spend);
        }
    }

    pub(super) fn set_spend(&mut self, session: SessionId, spend: Spend) {
        let Some(stored) = self.session_mut(session) else {
            return;
        };
        if stored.spend == Some(spend) {
            return;
        }
        stored.spend = Some(spend);
        let updated = stored.clone();
        self.broadcast(Delta::SessionUpserted(updated));
    }
}
