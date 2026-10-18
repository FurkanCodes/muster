//! What a session machine holds, for a person asking why a session has
//! its status (`muster session explain`): its open prompts, its hold for
//! subagents and its next deadline, each moment on the wall clock.

use std::time::{Duration, Instant};

use muster_core::Timestamp;
use muster_core::explain::{Deadline, HoldView, MachineView, OpenWait};

use super::SessionMachine;
use crate::session::{HookEvent, Signal};

/// The moment `at` on the wall clock, `now` being `wall` there.
fn on_wall(at: Instant, now: Instant, wall: Timestamp) -> Timestamp {
    let millis = |span: Duration| i64::try_from(span.as_millis()).unwrap_or(i64::MAX);
    let wall = wall.as_millis();
    Timestamp::from_millis(if at >= now {
        wall.saturating_add(millis(at - now))
    } else {
        wall.saturating_sub(millis(now - at))
    })
}

impl SessionMachine {
    /// What it holds at `now`, which is `wall` on the wall clock.
    pub fn explain(&self, now: Instant, wall: Timestamp) -> MachineView {
        let wall_of = |at| on_wall(at, now, wall);
        MachineView {
            watched: self.watched,
            waits: self
                .waits
                .iter()
                .map(|(origin, reason)| OpenWait {
                    origin: origin.clone(),
                    reason: *reason,
                })
                .collect(),
            subagents: self.subagents.iter().cloned().collect(),
            hold: self.hold.map(|hold| HoldView {
                since: wall_of(hold.since),
                drained_at: hold.drained_at.map(wall_of),
            }),
            awaiting_start: self.start_by.is_some(),
            next: self.next_deadline_and_kind().map(|(at, kind)| Deadline {
                at: wall_of(at),
                kind,
            }),
        }
    }

    /// Whether `signal` would count: a hook from another agent session
    /// (but its start) is ignored, as is everything but being watched once
    /// the program has exited.
    pub fn heeds(&self, signal: &Signal) -> bool {
        match signal {
            Signal::Watched { .. } => true,
            _ if self.status.is_exited() => false,
            Signal::Hook {
                agent_session: Some(id),
                event,
            } => {
                *event == HookEvent::SessionStarted
                    || self
                        .agent_session
                        .as_ref()
                        .is_none_or(|current| current == id)
            }
            _ => true,
        }
    }
}

#[cfg(test)]
#[expect(clippy::disallowed_methods, reason = "tests time themselves")]
mod tests {
    use muster_core::explain::DeadlineKind;
    use muster_core::{SessionStatus, WaitReason};

    use super::*;
    use crate::session::{Launch, Origin, Start, Timing};

    const WALL: Timestamp = Timestamp::from_millis(1_800_000_000_000);

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
    fn a_held_turn_with_a_subagent_waiting_says_whose_wait_and_no_deadline() {
        let start = Instant::now();
        let at = |secs| start + Duration::from_secs(secs);
        let mut machine = SessionMachine::new(
            Launch {
                agent_session: Some("s".to_owned()),
                start: Start::WithPrompt,
                unseen: false,
                reports_start: false,
            },
            Timing::default(),
            start,
        );
        hook(
            &mut machine,
            HookEvent::SubagentStarted { id: "a1".into() },
            at(1),
        );
        hook(
            &mut machine,
            HookEvent::PermissionRequested {
                origin: Origin::Subagent("a1".into()),
                deferred: false,
            },
            at(2),
        );
        hook(&mut machine, HookEvent::TurnEnded, at(3));
        let view = machine.explain(at(10), WALL);
        assert_eq!(
            view.waits,
            [OpenWait {
                origin: Origin::Subagent("a1".into()),
                reason: WaitReason::Permission,
            }]
        );
        assert_eq!(view.subagents, ["a1"]);
        assert_eq!(
            view.hold,
            Some(HoldView {
                since: Timestamp::from_millis(WALL.as_millis() - 7_000),
                drained_at: None,
            })
        );
        assert_eq!(view.next, None, "waiting on the person is never quiet");
        assert!(!view.watched && !view.awaiting_start);

        // The subagent answered and stopped: the turn finishes a grace on.
        hook(
            &mut machine,
            HookEvent::SubagentStopped { id: "a1".into() },
            at(20),
        );
        let view = machine.explain(at(20), WALL);
        assert!(view.waits.is_empty() && view.subagents.is_empty());
        assert_eq!(
            view.next,
            Some(Deadline {
                at: Timestamp::from_millis(WALL.as_millis() + 180_000),
                kind: DeadlineKind::Drain,
            })
        );
    }

    #[test]
    fn a_start_awaited_has_its_deadline_until_it_passes() {
        let start = Instant::now();
        let mut machine = SessionMachine::new(
            Launch {
                agent_session: None,
                start: Start::Idle,
                unseen: false,
                reports_start: true,
            },
            Timing::default(),
            start,
        );
        let view = machine.explain(start + Duration::from_secs(4), WALL);
        assert!(view.awaiting_start);
        assert_eq!(
            view.next,
            Some(Deadline {
                at: Timestamp::from_millis(WALL.as_millis() + 6_000),
                kind: DeadlineKind::Start,
            })
        );
        machine.step(Signal::Tick, start + Duration::from_secs(10));
        assert_eq!(
            machine.status(),
            SessionStatus::NeedsYou {
                reason: WaitReason::Startup
            }
        );
        let view = machine.explain(start + Duration::from_secs(11), WALL);
        assert!(view.awaiting_start && view.next.is_none());
    }

    #[test]
    fn hooks_from_another_agent_session_and_after_an_exit_are_not_heeded() {
        let start = Instant::now();
        let mut machine = SessionMachine::new(
            Launch {
                agent_session: Some("ours".to_owned()),
                start: Start::Idle,
                unseen: false,
                reports_start: false,
            },
            Timing::default(),
            start,
        );
        let from = |id: &str, event| Signal::Hook {
            agent_session: Some(id.to_owned()),
            event,
        };
        assert!(machine.heeds(&from("ours", HookEvent::PromptSubmitted)));
        assert!(!machine.heeds(&from("theirs", HookEvent::PromptSubmitted)));
        assert!(machine.heeds(&from("theirs", HookEvent::SessionStarted)));
        assert!(machine.heeds(&Signal::Progress { busy: true }));
        machine.step(
            Signal::Exited {
                success: true,
                requested: false,
            },
            start,
        );
        assert!(!machine.heeds(&Signal::Progress { busy: true }));
        assert!(machine.heeds(&Signal::Watched { watching: true }));
    }
}
