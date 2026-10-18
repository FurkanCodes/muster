//! Why a session has its status (`muster session explain`): what its
//! status machine holds, and the signals it last received. Only kinds of
//! signal and tools' names, never what an agent or the person wrote, a
//! payload's body or terminal output (`PRD-06`).

use serde::{Deserialize, Serialize};

use crate::ids::SessionId;
use crate::session::{Origin, SessionStatus, WaitReason};
use crate::time::Timestamp;

/// The first protocol that asks a daemon to explain a session
/// ([`crate::protocol::Command::ExplainSession`]).
pub const EXPLAIN_PROTOCOL: u32 = 30;

/// A session's status and why it has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Explanation {
    pub session: SessionId,
    pub status: SessionStatus,
    /// When it came to its status.
    pub since: Timestamp,
    pub unseen: bool,
    /// The daemon's time as it explained, which every other time is read
    /// against.
    pub now: Timestamp,
    /// What its machine holds; `None` for a session whose program ended
    /// with an earlier daemon, whose machine this one never had.
    pub machine: Option<MachineView>,
    /// The signals it last received, oldest first, since this daemon
    /// started its program or took it over; a few dozen at most.
    pub signals: Vec<SignalSeen>,
}

/// What a session's status machine holds.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MachineView {
    /// Someone is looking at its terminal, so nothing in it is news.
    pub watched: bool,
    /// The prompts open on the person, oldest first; the newest is shown.
    pub waits: Vec<OpenWait>,
    /// The subagents it knows to be at work.
    pub subagents: Vec<String>,
    /// The main agent's turn has ended, but the session keeps its status
    /// for its subagents.
    pub hold: Option<HoldView>,
    /// It has not yet said it started; silent past a grace, it is taken to
    /// be asking the person something first.
    pub awaiting_start: bool,
    /// When the machine next acts on its own, and what it then does.
    pub next: Option<Deadline>,
}

/// A prompt open on the person: whose, and for what.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenWait {
    pub origin: Origin,
    pub reason: WaitReason,
}

/// A turn held for its subagents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HoldView {
    /// When the main agent ended its turn.
    pub since: Timestamp,
    /// When the last subagent stopped, if none is at work now.
    pub drained_at: Option<Timestamp>,
}

/// What a session's machine does on its own, and when.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deadline {
    pub at: Timestamp,
    pub kind: DeadlineKind,
}

/// What happens at a session machine's deadline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeadlineKind {
    /// Still silent since launch, it is taken to be asking the person
    /// something before it starts.
    Start,
    /// Its last subagent stopped a grace ago: the turn finishes.
    Drain,
    /// Its subagents have been silent so long that they are taken to be
    /// gone: the turn finishes.
    Quiet,
}

/// One signal a session's machine received.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalSeen {
    pub at: Timestamp,
    /// What it was, for people: `hook: permission requested`, `progress:
    /// idle`, `watched`.
    pub what: String,
    /// The tool a hook named, if it named one.
    #[serde(default)]
    pub tool: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::{FrameDecoder, encode};
    use crate::protocol::{Reply, ServerFrame};

    #[test]
    fn an_explanation_reaches_a_client_as_the_daemon_sent_it() {
        let at = Timestamp::from_millis(1_800_000_000_000);
        let frame = ServerFrame::Reply {
            req_id: 3,
            outcome: Ok(Reply::Explanation(Box::new(Explanation {
                session: SessionId::new(),
                status: SessionStatus::NeedsYou {
                    reason: WaitReason::Question,
                },
                since: at,
                unseen: true,
                now: at,
                machine: Some(MachineView {
                    watched: false,
                    waits: vec![OpenWait {
                        origin: Origin::Subagent("a1".to_owned()),
                        reason: WaitReason::Question,
                    }],
                    subagents: vec!["a1".to_owned()],
                    hold: Some(HoldView {
                        since: at,
                        drained_at: None,
                    }),
                    awaiting_start: false,
                    next: Some(Deadline {
                        at,
                        kind: DeadlineKind::Quiet,
                    }),
                }),
                signals: vec![SignalSeen {
                    at,
                    what: "hook: question asked by subagent a1".to_owned(),
                    tool: Some("AskUserQuestion".to_owned()),
                }],
            }))),
        };
        let mut decoder = FrameDecoder::new();
        decoder.push(&encode(&frame).unwrap());
        assert_eq!(decoder.next_frame::<ServerFrame>().unwrap(), Some(frame));
    }
}
