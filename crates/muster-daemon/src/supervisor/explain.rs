//! Why a session has its status (`Command::ExplainSession`): what its
//! machine holds, and the signals it last received, kept in a short ring
//! on each session whose terminal the daemon holds and gone with it
//! (`ARC-14`). Only each signal's kind and a tool's name are kept, never a
//! payload's body, a prompt or terminal output (`PRD-06`, `ARC-20`).

use std::collections::VecDeque;

use muster_core::SessionId;
use muster_core::explain::{Explanation, SignalSeen};
use muster_core::protocol::{ErrorBody, ErrorCode, Reply};
use muster_state::session::Signal;

use super::Supervisor;

/// How many signals a session keeps: enough for a turn or two.
const KEPT: usize = 32;

/// A session's last signals, oldest first.
#[derive(Debug, Default)]
pub(super) struct Recent(VecDeque<SignalSeen>);

impl Recent {
    fn push(&mut self, seen: SignalSeen) {
        if self.0.len() == KEPT {
            self.0.pop_front();
        }
        self.0.push_back(seen);
    }

    fn all(&self) -> Vec<SignalSeen> {
        self.0.iter().cloned().collect()
    }
}

impl Supervisor {
    /// Notes `signal` in its session's ring before the machine takes it,
    /// with the tool its hook named; one the machine will ignore says so.
    pub(super) fn note_signal(
        &mut self,
        session: SessionId,
        signal: &Signal,
        tool: Option<String>,
    ) {
        let at = self.clock.now();
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        let mut what = signal.describe();
        if !live.machine.heeds(signal) {
            what.push_str(" (ignored)");
        }
        live.signals.push(SignalSeen { at, what, tool });
    }

    /// Why `session` has its status.
    pub(super) fn explain_session(&self, session: SessionId) -> Result<Reply, ErrorBody> {
        let stored = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no session {session}")))?;
        let now = self.clock.now();
        let live = self.live.get(&session);
        Ok(Reply::Explanation(Box::new(Explanation {
            session,
            status: stored.status,
            since: stored.status_since,
            unseen: stored.unseen,
            now,
            machine: live.map(|live| live.machine.explain(self.clock.instant(), now)),
            signals: live.map(|live| live.signals.all()).unwrap_or_default(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use muster_core::Timestamp;

    use super::*;

    #[test]
    fn the_ring_keeps_the_newest_few_oldest_first() {
        let mut recent = Recent::default();
        for n in 0..KEPT + 5 {
            recent.push(SignalSeen {
                at: Timestamp::from_millis(i64::try_from(n).unwrap()),
                what: format!("signal {n}"),
                tool: None,
            });
        }
        let all = recent.all();
        assert_eq!(all.len(), KEPT, "ARC-14: bounded");
        assert_eq!(all[0].what, "signal 5");
        assert_eq!(all[KEPT - 1].what, format!("signal {}", KEPT + 4));
    }
}
