//! Sessions the last daemon left running, brought back after a restart
//! (`PRD-09`, `ARCHITECTURE.md` §4).
//!
//! A worktree session's program died with the daemon that held its
//! terminal, so a harness that can resume is started again on the
//! conversation it had. Two rules keep that from doing harm. A conversation
//! is resumed in one session only: two agents on one conversation would
//! write one transcript at once. And agents start one at a time, [`SPACING`]
//! apart, on deadlines of the injected clock (`ARC-03`) the Supervisor's
//! loop wakes for, so a dozen agents never start in the same instant.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use muster_core::protocol::TermSize;
use muster_core::{Session, SessionId, SessionStatus};
use muster_state::session::Start;

use super::Supervisor;
use super::sessions::Mode;
use crate::harness::activity::ActivityChange;
use crate::harness::{self, Harness, Place};

/// How long after one session's program has been started again the next
/// one's starts.
pub(super) const SPACING: Duration = Duration::from_millis(150);

/// What a session not resumed, because another one has its conversation,
/// says on its line.
pub(super) const CONVERSATION_TAKEN: &str = "not resumed: another has its conversation";

/// Sessions waiting their turn to be started again, oldest first. One entry
/// per session the last daemon left running, each leaving as its turn
/// comes, or as the person stops it first (`ARC-14`).
#[derive(Debug, Default)]
pub(super) struct Resumes {
    waiting: VecDeque<SessionId>,
    /// When the next may start: [`SPACING`] after the last one started.
    next_at: Option<Instant>,
}

impl Resumes {
    /// Puts `session` at the back of the line.
    pub(super) fn wait(&mut self, session: SessionId) {
        if !self.waiting.contains(&session) {
            self.waiting.push_back(session);
        }
    }

    /// Whether `session` waits its turn.
    pub(super) fn is_waiting(&self, session: SessionId) -> bool {
        self.waiting.contains(&session)
    }

    /// Takes `session` out of the line; `false` if it was not in it.
    pub(super) fn forget(&mut self, session: SessionId) -> bool {
        let before = self.waiting.len();
        self.waiting.retain(|waiting| *waiting != session);
        self.waiting.len() != before
    }

    /// When the Supervisor should wake for the next turn, if one waits.
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.next_at.filter(|_| !self.waiting.is_empty())
    }

    /// The session whose turn it is at `now`, if one's is.
    pub(super) fn due(&mut self, now: Instant) -> Option<SessionId> {
        if self.next_at.is_some_and(|at| at > now) {
            return None;
        }
        self.waiting.pop_front()
    }

    /// A program was started at `at`: the next waits [`SPACING`].
    pub(super) fn started(&mut self, at: Instant) {
        self.next_at = Some(at + SPACING);
    }
}

impl Supervisor {
    /// Sessions the last daemon left running died with it, because it held
    /// their terminals. Each one in a run's container is taken up with its
    /// run at once; the rest wait their turn to resume, the first starting
    /// now.
    pub(super) async fn recover_sessions(&mut self) {
        let orphaned: Vec<Session> = self
            .state
            .sessions
            .iter()
            .filter(|session| session.ended_at.is_none() && !self.live.contains_key(&session.id))
            .cloned()
            .collect();
        for session in orphaned {
            if let Some(run) = self.workspace_run(session.workspace) {
                self.recover_in_container(&session, run).await;
                continue;
            }
            self.resumes.wait(session.id);
        }
        self.resume_next().await;
    }

    /// Brings back the session whose turn it is, if one's is. One whose
    /// program is not started takes no turn: the next is looked at at once.
    pub(super) async fn resume_next(&mut self) {
        let now = self.clock.instant();
        while let Some(session) = self.resumes.due(now) {
            if self.bring_back(session).await {
                self.resumes.started(self.clock.instant());
                return;
            }
        }
    }

    /// A session stopped while it waited its turn: it is not started again.
    /// `false` if it was not waiting.
    pub(super) async fn stop_waiting(&mut self, session: SessionId) -> bool {
        if !self.resumes.forget(session) {
            return false;
        }
        tracing::info!(%session, "stopped before it was resumed");
        self.end_session(session, SessionStatus::Exited { success: true }, false)
            .await;
        true
    }

    /// Resumes `session` if it can be; records it as ended otherwise, with
    /// why where another session has its conversation. `true` if its
    /// program was started.
    async fn bring_back(&mut self, session: SessionId) -> bool {
        // Since it began to wait, the person may have removed it, or it may
        // have been started some other way.
        let Some(stored) = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .cloned()
        else {
            return false;
        };
        if stored.ended_at.is_some() || self.live.contains_key(&session) {
            return false;
        }
        let conversation = harness::find(&stored.harness)
            .and_then(|harness| Some((harness, resumable(harness, &stored)?)));
        let Some((harness, agent_session)) = conversation else {
            tracing::info!(%session, "a session ended when the daemon stopped");
            self.end_session(session, SessionStatus::Exited { success: false }, true)
                .await;
            return false;
        };
        if let Some(holder) = self.conversation_holder(harness, &agent_session, session) {
            tracing::info!(%session, %holder, "not resumed: another session has its conversation");
            self.end_session(session, SessionStatus::Exited { success: false }, true)
                .await;
            self.set_activity(session, ActivityChange::Set(CONVERSATION_TAKEN.to_owned()));
            return false;
        }
        if self.resume(&stored, harness, agent_session).await {
            return true;
        }
        tracing::info!(%session, "a session ended when the daemon stopped");
        self.end_session(session, SessionStatus::Exited { success: false }, true)
            .await;
        false
    }

    /// The session, other than `except`, whose running program is on
    /// `harness`'s conversation `agent_session`, if one is.
    pub(super) fn conversation_holder(
        &self,
        harness: Harness,
        agent_session: &str,
        except: SessionId,
    ) -> Option<SessionId> {
        self.state
            .sessions
            .iter()
            .filter(|other| other.id != except && other.harness == harness.id)
            .find(|other| {
                self.live.get(&other.id).is_some_and(|live| {
                    !live.machine.status().is_exited()
                        && live.machine.agent_session().or(other.resume_id.as_deref())
                            == Some(agent_session)
                })
            })
            .map(|other| other.id)
    }

    /// Starts `session`'s program again on `agent_session`. A turn the
    /// restart cut short counts as unseen.
    async fn resume(&mut self, session: &Session, harness: Harness, agent_session: String) -> bool {
        let Some(cwd) = self.workspace_path(session.workspace) else {
            return false;
        };
        let unseen = session.unseen
            || matches!(
                session.status,
                SessionStatus::Running | SessionStatus::NeedsYou { .. }
            );
        let mode = Mode::Resume { agent_session };
        let (size, place) = (TermSize::default(), Place::Host(cwd));
        match self.launch(
            session.id,
            harness,
            place,
            size,
            mode,
            Start::Resumed,
            unseen,
        ) {
            Ok(()) => {
                tracing::info!(session = %session.id, "resumed");
                if let Some((status, unseen)) = self.launched_status(session.id) {
                    self.set_status(session.id, status, unseen).await;
                }
                true
            }
            Err(error) => {
                tracing::warn!(session = %session.id, %error, "could not resume");
                false
            }
        }
    }
}

/// The harness session to resume `session` from: one whose harness can
/// resume, whose id was saved after a turn ran, and whose transcript has
/// not been deleted. An unreadable transcript folder is no verdict, so the
/// resume is tried; if it fails the session starts fresh.
pub(super) fn resumable(harness: Harness, session: &Session) -> Option<String> {
    let agent_session = session.resume_id.clone().filter(|_| harness.can_resume())?;
    if harness.has_conversation(&agent_session) == Some(false) {
        tracing::info!(session = %session.id, "no transcript to resume");
        return None;
    }
    Some(agent_session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_waiting_resumes_at_once_and_each_next_one_spacing_after_arc_13() {
        let start = crate::clock::monotonic();
        let (one, two, three) = (SessionId::new(), SessionId::new(), SessionId::new());
        let mut resumes = Resumes::default();
        for session in [one, two, three] {
            resumes.wait(session);
        }
        // Nothing started yet: the first's turn is now.
        assert_eq!(resumes.due(start), Some(one));
        resumes.started(start);
        assert_eq!(resumes.next_deadline(), Some(start + SPACING));
        // Not before its time.
        assert_eq!(resumes.due(start + Duration::from_millis(149)), None);
        assert_eq!(resumes.due(start + SPACING), Some(two));
        // A program that took a while to start puts the next off from when
        // it had started.
        let later = start + SPACING + Duration::from_millis(40);
        resumes.started(later);
        assert_eq!(resumes.next_deadline(), Some(later + SPACING));
        assert_eq!(resumes.due(later + SPACING), Some(three));
        resumes.started(later + SPACING);
        // No one waits: nothing to wake for.
        assert_eq!(resumes.next_deadline(), None);
        assert_eq!(resumes.due(later + SPACING * 10), None);
    }

    #[test]
    fn one_not_started_takes_no_turn() {
        let start = crate::clock::monotonic();
        let (one, two) = (SessionId::new(), SessionId::new());
        let mut resumes = Resumes::default();
        resumes.wait(one);
        resumes.wait(two);
        // The first was not started (it could not resume): the next is
        // due at once.
        assert_eq!(resumes.due(start), Some(one));
        assert_eq!(resumes.due(start), Some(two));
    }

    #[test]
    fn a_session_waits_once_and_leaves_when_stopped_arc_14() {
        let start = crate::clock::monotonic();
        let (one, two) = (SessionId::new(), SessionId::new());
        let mut resumes = Resumes::default();
        resumes.wait(one);
        resumes.wait(one);
        resumes.wait(two);
        assert!(resumes.is_waiting(one));
        assert!(resumes.forget(one));
        assert!(!resumes.forget(one));
        assert!(!resumes.is_waiting(one));
        assert_eq!(resumes.due(start), Some(two));
        assert_eq!(resumes.due(start), None);
    }
}
