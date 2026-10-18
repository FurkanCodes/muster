//! Signals as people read them, for a person asking why a session has its
//! status: each one's kind and whose it was, never what it carried
//! (`PRD-06`).

use super::signal::{HookEvent, Origin, Signal};

impl Signal {
    /// What the signal was: `hook: permission requested`, `progress:
    /// idle`, `watched`.
    pub fn describe(&self) -> String {
        let text = match self {
            // Only the agent's own records tell it, never a hook.
            Self::Hook {
                event: HookEvent::TurnInterrupted,
                ..
            } => "its records: turn cut short by the person",
            Self::Hook { event, .. } => return format!("hook: {}", event.describe()),
            Self::Progress { busy: true } => "progress: busy",
            Self::Progress { busy: false } => "progress: idle",
            Self::Waiting { waiting: true } => "its display: waiting on the person",
            Self::Waiting { waiting: false } => "its display: no longer waiting",
            Self::Exited {
                requested: true, ..
            } => "exited, stopped as the person asked",
            Self::Exited { success: true, .. } => "exited",
            Self::Exited { success: false, .. } => "exited with an error",
            Self::Watched { watching: true } => "watched",
            Self::Watched { watching: false } => "no longer watched",
            Self::Tick => "a deadline passed",
        };
        text.to_owned()
    }
}

impl HookEvent {
    /// What the hook said: `permission requested by subagent a1`.
    pub fn describe(&self) -> String {
        let by = |what: &str, origin: &Origin| match origin {
            Origin::Main => what.to_owned(),
            Origin::Subagent(id) => format!("{what} by subagent {id}"),
        };
        match self {
            Self::SessionStarted => "session started".to_owned(),
            Self::PromptSubmitted => "prompt submitted".to_owned(),
            Self::TurnEnded => "turn ended".to_owned(),
            Self::TurnInterrupted => "turn cut short by the person".to_owned(),
            Self::PermissionRequested {
                origin,
                deferred: false,
            } => by("permission requested", origin),
            Self::PermissionRequested {
                origin,
                deferred: true,
            } => by("permission notice", origin),
            Self::QuestionAsked {
                origin,
                deferred: false,
            } => by("question asked", origin),
            Self::QuestionAsked {
                origin,
                deferred: true,
            } => by("question notice", origin),
            Self::ToolStarted { origin } => by("tool started", origin),
            Self::ToolFinished {
                origin,
                question: false,
            } => by("tool finished", origin),
            Self::ToolFinished {
                origin,
                question: true,
            } => by("question answered", origin),
            Self::Idle => "idle at its prompt".to_owned(),
            Self::SubagentStarted { id } => format!("subagent {id} started"),
            Self::SubagentStopped { id } => format!("subagent {id} stopped"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signals_read_as_their_kind_and_whose_they_were() {
        let hook = |event| Signal::Hook {
            agent_session: Some("s".to_owned()),
            event,
        };
        assert_eq!(
            hook(HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: false
            })
            .describe(),
            "hook: permission requested"
        );
        assert_eq!(
            hook(HookEvent::ToolFinished {
                origin: Origin::Subagent("a1".to_owned()),
                question: true
            })
            .describe(),
            "hook: question answered by subagent a1"
        );
        assert_eq!(
            hook(HookEvent::TurnInterrupted).describe(),
            "its records: turn cut short by the person"
        );
        assert_eq!(
            Signal::Progress { busy: false }.describe(),
            "progress: idle"
        );
        assert_eq!(
            Signal::Exited {
                success: true,
                requested: true
            }
            .describe(),
            "exited, stopped as the person asked"
        );
        assert_eq!(Signal::Tick.describe(), "a deadline passed");
    }
}
