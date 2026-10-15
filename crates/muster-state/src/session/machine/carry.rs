//! A session machine carried across the daemon's restart in place
//! (`muster reload`): everything it knows, with each moment written as how
//! long before the handoff it was, since an `Instant` means nothing outside
//! the process that read it. The new daemon takes it back at its own now,
//! so open prompts, subagents still working, holds and the start grace go
//! on as if nothing happened. Watching is not carried: clients connect
//! again and say so.

use std::time::{Duration, Instant};

use muster_core::{SessionStatus, WaitReason};
use serde::{Deserialize, Serialize};

use super::{Hold, SessionMachine, Timing};
use crate::session::Origin;

/// A session machine as carried.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    pub status: SessionStatus,
    pub unseen: bool,
    pub agent_session: Option<String>,
    pub resume_id: Option<String>,
    pub waits: Vec<(Origin, WaitReason)>,
    /// The last prompt answered, and how long ago.
    pub answered: Option<(Origin, Duration)>,
    pub subagents: Vec<String>,
    pub subagent_activity: Option<Duration>,
    /// Since how long ago it holds, and how long ago the last subagent
    /// stopped.
    pub hold: Option<(Duration, Option<Duration>)>,
    pub finished: Option<Duration>,
    pub busy_seen: bool,
    /// How long until it must report its start (`None` once it has), as
    /// milliseconds that are negative once that has passed.
    pub start_in_ms: Option<i64>,
    pub before_startup: Option<(SessionStatus, bool)>,
}

/// How long before `now` the moment `at` was.
fn ago(at: Instant, now: Instant) -> Duration {
    now.saturating_duration_since(at)
}

/// The moment `ago` before `now`; `now` itself if that is before the clock
/// began.
fn back(ago: Duration, now: Instant) -> Instant {
    now.checked_sub(ago).unwrap_or(now)
}

impl SessionMachine {
    /// This machine as carried, at `now`.
    pub fn carry(&self, now: Instant) -> Carried {
        Carried {
            status: self.status,
            unseen: self.unseen,
            agent_session: self.agent_session.clone(),
            resume_id: self.resume_id.clone(),
            waits: self.waits.clone(),
            answered: self
                .answered
                .clone()
                .map(|(origin, at)| (origin, ago(at, now))),
            subagents: self.subagents.iter().cloned().collect(),
            subagent_activity: self.subagent_activity.map(|at| ago(at, now)),
            hold: self
                .hold
                .map(|hold| (ago(hold.since, now), hold.drained_at.map(|at| ago(at, now)))),
            finished: self.finished_at.map(|at| ago(at, now)),
            busy_seen: self.busy_seen,
            start_in_ms: self.start_by.map(|by| {
                if by >= now {
                    i64::try_from((by - now).as_millis()).unwrap_or(i64::MAX)
                } else {
                    -i64::try_from((now - by).as_millis()).unwrap_or(i64::MAX)
                }
            }),
            before_startup: self.before_startup,
        }
    }

    /// The machine `carried` was, taken back at `now`, unwatched.
    pub fn adopt(carried: Carried, timing: Timing, now: Instant) -> Self {
        Self {
            timing,
            status: carried.status,
            unseen: carried.unseen,
            watched: false,
            agent_session: carried.agent_session,
            resume_id: carried.resume_id,
            waits: carried.waits,
            answered: carried
                .answered
                .map(|(origin, ago)| (origin, back(ago, now))),
            subagents: carried.subagents.into_iter().collect(),
            subagent_activity: carried.subagent_activity.map(|ago| back(ago, now)),
            hold: carried.hold.map(|(since, drained)| Hold {
                since: back(since, now),
                drained_at: drained.map(|ago| back(ago, now)),
            }),
            finished_at: carried.finished.map(|ago| back(ago, now)),
            busy_seen: carried.busy_seen,
            start_by: carried.start_in_ms.map(|ms| {
                if ms >= 0 {
                    now + Duration::from_millis(ms.unsigned_abs())
                } else {
                    back(Duration::from_millis(ms.unsigned_abs()), now)
                }
            }),
            before_startup: carried.before_startup,
        }
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "tests time themselves")]
mod tests {
    use super::*;
    use crate::session::{HookEvent, Launch, Signal, Start};

    fn hook(machine: &mut SessionMachine, event: HookEvent, at: Instant) {
        machine.step(
            Signal::Hook {
                agent_session: None,
                event,
            },
            at,
        );
    }

    #[test]
    fn a_machine_mid_turn_comes_back_as_it_was() {
        let timing = Timing::default();
        let start = Instant::now();
        let mut machine = SessionMachine::new(
            Launch {
                agent_session: Some("s".to_owned()),
                start: Start::WithPrompt,
                unseen: false,
                reports_start: false,
            },
            timing,
            start,
        );
        let at = |secs| start + Duration::from_secs(secs);
        hook(
            &mut machine,
            HookEvent::SubagentStarted { id: "a".into() },
            at(1),
        );
        hook(
            &mut machine,
            HookEvent::PermissionRequested {
                origin: Origin::Subagent("a".into()),
                deferred: false,
            },
            at(2),
        );
        hook(&mut machine, HookEvent::TurnEnded, at(3));
        let carried = machine.carry(at(10));

        // Through the bytes the handoff writes, and back at another now.
        let bytes = serde_json::to_vec(&carried).unwrap();
        let carried: Carried = serde_json::from_slice(&bytes).unwrap();
        let later = Instant::now() + Duration::from_secs(100);
        let adopted = SessionMachine::adopt(carried, timing, later);
        assert_eq!(adopted.status(), machine.status());
        assert_eq!(adopted.unseen(), machine.unseen());
        assert_eq!(adopted.agent_session(), Some("s"));
        // Still holding for its subagent, waiting on the person: no deadline.
        assert_eq!(adopted.next_deadline(), machine.next_deadline());
        assert_eq!(adopted.carry(later), machine.carry(at(10)));
    }

    #[test]
    fn a_start_still_awaited_keeps_its_deadline() {
        let timing = Timing::default();
        let start = Instant::now();
        let machine = SessionMachine::new(
            Launch {
                agent_session: None,
                start: Start::Idle,
                unseen: false,
                reports_start: true,
            },
            timing,
            start,
        );
        let carried = machine.carry(start + Duration::from_secs(4));
        assert_eq!(carried.start_in_ms, Some(6_000));
        let now = Instant::now() + Duration::from_secs(50);
        let adopted = SessionMachine::adopt(carried, timing, now);
        assert_eq!(adopted.next_deadline(), Some(now + Duration::from_secs(6)));
    }
}
