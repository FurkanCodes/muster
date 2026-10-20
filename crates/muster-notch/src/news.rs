//! What newly needs the person, for Muster's helper to notify them of
//! (`PRODUCT.md` §6), as the UI tells it (`muster_core::news`): a session
//! that comes to need them, once; a finish only once it has stayed one a
//! moment, as an agent that ends a turn often starts another at once; and
//! never what a snapshot shows, which is no news. Pure but for the time
//! it is handed.

use std::collections::HashSet;
use std::time::Duration;

use muster_core::SessionId;
use muster_core::news::{needs_attention, told_at_once};
use muster_core::protocol::Snapshot;
use tokio::time::Instant;

/// How long a finish is held before it is told: as in the UI.
pub(crate) const HOLD: Duration = Duration::from_secs(2);

/// What needed the person when last looked, and the finishes held.
#[derive(Debug, Default)]
pub(crate) struct News {
    needing: HashSet<SessionId>,
    held: Vec<(SessionId, Instant)>,
}

impl News {
    /// A snapshot: what it shows is no news.
    pub(crate) fn known(&mut self, state: &Snapshot) {
        self.needing = needing(state);
        self.held.clear();
    }

    /// The state changed at `now`: the sessions that newly need the person
    /// and are told at once; a finish is held.
    pub(crate) fn changed(&mut self, state: &Snapshot, now: Instant) -> Vec<SessionId> {
        let needing = needing(state);
        let mut told = Vec::new();
        for session in needing.difference(&self.needing) {
            let Some(stored) = state.sessions.iter().find(|stored| stored.id == *session) else {
                continue;
            };
            if told_at_once(stored) {
                told.push(*session);
            } else {
                self.held.retain(|(held, _)| held != session);
                self.held.push((*session, now));
            }
        }
        self.needing = needing;
        told.sort();
        told
    }

    /// The finishes held long enough by `now`, still finished and unseen:
    /// those at work again meanwhile, or gone, are dropped.
    pub(crate) fn due(&mut self, state: &Snapshot, now: Instant) -> Vec<SessionId> {
        let (due, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|(_, at)| now.duration_since(*at) >= HOLD);
        self.held = held;
        due.into_iter()
            .map(|(session, _)| session)
            .filter(|session| {
                state.sessions.iter().any(|stored| {
                    stored.id == *session
                        && stored.status == muster_core::SessionStatus::Finished
                        && needs_attention(stored)
                })
            })
            .collect()
    }

    /// Whether a finish is held, waiting to be told.
    pub(crate) fn holding(&self) -> bool {
        !self.held.is_empty()
    }
}

fn needing(state: &Snapshot) -> HashSet<SessionId> {
    state
        .sessions
        .iter()
        .filter(|session| needs_attention(session))
        .map(|session| session.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use muster_core::{Session, SessionStatus, Timestamp, WaitReason, WorkspaceId};

    use super::*;

    fn session(status: SessionStatus, unseen: bool) -> Session {
        Session {
            id: SessionId::new(),
            workspace: WorkspaceId::new(),
            harness: "claude".to_owned(),
            title: "Fix the login".to_owned(),
            status,
            unseen,
            resume_id: None,
            created_at: Timestamp::from_millis(0),
            ended_at: None,
            status_since: Timestamp::from_millis(0),
            activity: None,
            spend: None,
            tidy: None,
        }
    }

    fn state(sessions: Vec<Session>) -> Snapshot {
        Snapshot {
            sessions,
            ..Snapshot::default()
        }
    }

    #[test]
    fn a_question_is_told_at_once_and_once_and_a_snapshot_never() {
        let start = Instant::now();
        let mut news = News::default();
        let asking = session(
            SessionStatus::NeedsYou {
                reason: WaitReason::Question,
            },
            false,
        );
        news.known(&state(vec![asking.clone()]));
        assert!(news.changed(&state(vec![asking.clone()]), start).is_empty());

        let mut working = asking.clone();
        working.status = SessionStatus::Running;
        assert!(news.changed(&state(vec![working]), start).is_empty());
        assert_eq!(
            news.changed(&state(vec![asking.clone()]), start),
            [asking.id]
        );
        assert!(news.changed(&state(vec![asking]), start).is_empty(), "once");
    }

    #[test]
    fn a_finish_is_told_once_it_has_stayed_one_two_seconds() {
        let start = Instant::now();
        let mut news = News::default();
        let finished = session(SessionStatus::Finished, true);
        assert!(
            news.changed(&state(vec![finished.clone()]), start)
                .is_empty()
        );
        assert!(news.holding());
        let now = state(vec![finished.clone()]);
        assert!(news.due(&now, start + Duration::from_secs(1)).is_empty());
        assert_eq!(news.due(&now, start + HOLD), [finished.id]);
        assert!(!news.holding());

        // At work again within the hold: never told.
        let again = session(SessionStatus::Finished, true);
        news.changed(&state(vec![again.clone()]), start);
        let mut working = again;
        working.status = SessionStatus::Running;
        working.unseen = false;
        assert!(news.due(&state(vec![working]), start + HOLD).is_empty());
    }
}
