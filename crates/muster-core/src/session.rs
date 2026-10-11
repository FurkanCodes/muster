//! What a session is doing, as the person sees it.

use serde::{Deserialize, Serialize};

/// The status of one session. `muster-state` decides it; the daemon stores and
/// broadcasts it; clients draw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum SessionStatus {
    /// Launched and waiting for its first piece of work.
    Fresh,
    /// Working on a turn.
    Running,
    /// Blocked until the person answers.
    NeedsYou {
        /// What the agent is waiting for.
        reason: WaitReason,
    },
    /// Ended its turn and is waiting for the next prompt.
    Finished,
    /// The agent process has ended.
    Exited {
        /// Whether it exited cleanly.
        success: bool,
    },
}

impl SessionStatus {
    /// Whether the agent process has ended. Nothing changes the status after this.
    pub const fn is_exited(self) -> bool {
        matches!(self, Self::Exited { .. })
    }
}

/// Why a session needs the person.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitReason {
    /// The agent asked for permission to use a tool.
    Permission,
    /// The agent asked the person a question.
    Question,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_a_state_tag() {
        let status = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, r#"{"state":"needs_you","reason":"question"}"#);
        assert_eq!(
            serde_json::from_str::<SessionStatus>(&json).unwrap(),
            status
        );
        assert_eq!(
            serde_json::to_string(&SessionStatus::Finished).unwrap(),
            r#"{"state":"finished"}"#
        );
    }
}
