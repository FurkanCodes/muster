//! What a session is doing, as the person sees it.

use serde::{Deserialize, Serialize};

use crate::time::Timestamp;

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
    /// The agent has not started: it is asking something first, such as
    /// whether to trust the folder or to sign in.
    Startup,
    /// The agent's own display says it waits on the person, without saying
    /// for what: a harness that reports no hooks, such as Codex, whose
    /// window title reads "Action Required" for a permission and a
    /// question alike.
    Action,
}

/// What an agent is doing now, as its hooks tell: `editing view.rs`,
/// `running cargo`, `wants to run npm`, `thinking`. Only what a glance
/// needs: a file's name, never its path; a command's program, never its
/// arguments, which can carry a secret (`PRD-06`). Shown, never stored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Activity {
    pub text: String,
    /// When it began.
    pub since: Timestamp,
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
