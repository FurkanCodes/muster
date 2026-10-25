//! A sample of what Muster says of how a session stands
//! (`muster_core::explain`), every part of it filled in, for
//! `protocol_round_trip`.

use muster_core::explain::{
    Deadline, DeadlineKind, Explanation, HoldView, MachineView, OpenWait, SignalSeen,
};
use muster_core::session::Origin;
use muster_core::time::Timestamp;
use muster_core::{SessionId, SessionStatus, WaitReason};

fn at(offset: i64) -> Timestamp {
    Timestamp::from_millis(1_760_000_000_000 + offset)
}

pub(crate) fn explanation() -> Explanation {
    Explanation {
        session: SessionId::new(),
        status: SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        },
        since: at(0),
        unseen: true,
        now: at(9),
        machine: Some(MachineView {
            watched: false,
            waits: vec![OpenWait {
                origin: Origin::Subagent("a1".to_owned()),
                reason: WaitReason::Question,
            }],
            subagents: vec!["a1".to_owned()],
            hold: Some(HoldView {
                since: at(1),
                drained_at: Some(at(2)),
            }),
            awaiting_start: false,
            next: Some(Deadline {
                at: at(3),
                kind: DeadlineKind::Quiet,
            }),
        }),
        signals: vec![SignalSeen {
            at: at(4),
            what: "hook: question asked by subagent a1".to_owned(),
            tool: Some("AskUserQuestion".to_owned()),
        }],
    }
}
