//! What the session machine consumes and produces.

use muster_core::SessionStatus;

/// Who inside an agent session raised an event.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Origin {
    /// The main agent, the one the person talks to.
    Main,
    /// A subagent, by the id its harness gave it.
    Subagent(String),
}

/// A hook event, already translated from the harness's own format by its
/// driver in the daemon. The machine never sees harness-specific names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HookEvent {
    /// The agent started a session: at launch, on resume, or after `/clear`.
    SessionStarted,
    /// The person submitted a prompt.
    PromptSubmitted,
    /// The main agent ended its turn.
    TurnEnded,
    /// The agent asked permission to use a tool. `deferred` marks a notice
    /// that trails the request itself (Claude Code's permission notification),
    /// which can arrive after the person has already answered.
    PermissionRequested { origin: Origin, deferred: bool },
    /// The agent asked the person a question through its question tool.
    QuestionAsked { origin: Origin },
    /// A tool started. After a permission prompt, this means it was granted.
    ToolStarted { origin: Origin },
    /// A tool finished. `question` marks the question tool's own completion,
    /// which is the only thing that answers a question.
    ToolFinished { origin: Origin, question: bool },
    /// The harness reports it has sat idle at its prompt.
    Idle,
    /// A subagent started.
    SubagentStarted { id: String },
    /// A subagent stopped.
    SubagentStopped { id: String },
}

/// Everything the session machine reacts to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    /// A hook from the agent. `agent_session` is the harness's own session id
    /// when the payload carries one.
    Hook {
        agent_session: Option<String>,
        event: HookEvent,
    },
    /// The terminal progress sequence (OSC 9;4) turned busy or idle. Edges only.
    Progress { busy: bool },
    /// The agent process exited. `requested` is true when Muster stopped it
    /// on the person's behalf.
    Exited { success: bool, requested: bool },
    /// The person looked at the session.
    Viewed,
    /// The deadline from [`SessionMachine::next_deadline`](super::SessionMachine::next_deadline) has passed.
    Tick,
}

/// What the daemon must do after a step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// The status or the unseen flag changed: store it and tell clients.
    Status { status: SessionStatus, unseen: bool },
    /// The agent session to resume after a daemon restart changed.
    SaveResumeId(String),
}
