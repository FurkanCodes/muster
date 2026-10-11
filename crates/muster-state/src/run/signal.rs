//! What the run machine consumes and produces.

use muster_core::RunStage;

/// How a building agent's turn ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentOutcome {
    /// It says it is done.
    Done,
    /// It stopped on an error of its own.
    Failed { reason: String },
}

/// What the reviewer agent decided.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Approve,
    RequestChanges { comments: String },
}

/// Why an attempt is being made, handed to the building agent with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Feedback {
    ChecksFailed {
        report: String,
    },
    ReviewerRequestedChanges {
        comments: String,
    },
    PersonRequestedChanges {
        comments: String,
    },
    /// Notes the person gave when sending the run back to work.
    PersonNotes {
        notes: String,
    },
}

/// Everything the run machine reacts to. Reports on work the machine asked
/// for carry the generation it was asked in; a report from an earlier
/// generation (work overtaken by a cancel, a take-over or a new attempt) is
/// ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEvent {
    /// There is room for the run (`PRD-05`).
    SlotGranted,
    ContainerReady {
        generation: u64,
    },
    AgentFinished {
        generation: u64,
        outcome: AgentOutcome,
    },
    ChecksFinished {
        generation: u64,
        passed: bool,
        /// The failing output, for the building agent.
        report: String,
    },
    PrOpened {
        generation: u64,
        number: u64,
    },
    ReviewSubmitted {
        generation: u64,
        verdict: Verdict,
    },
    /// Something other than the agent's work failed: the container, a push,
    /// GitHub, or the reviewer's output twice.
    InfraFailed {
        generation: u64,
        reason: String,
        /// The container is gone, and the work in it.
        container_lost: bool,
    },
    /// What the run has cost so far, where the harness reports it, in
    /// millionths of a dollar.
    CostReported {
        micros: u64,
    },
    /// The person asked for changes on the pull request.
    PersonRequestedChanges {
        comments: String,
    },
    PrMerged,
    /// The pull request was closed without merging.
    PrClosed,
    Cancel,
    /// Send a run that needs the person back to work, with a fresh budget.
    Retry {
        notes: Option<String>,
    },
    /// The person drives the building agent by hand.
    TakeOver,
    /// The person is done; their work goes to the checks.
    HandBack,
    /// The deadline from [`RunMachine::next_deadline`](super::RunMachine::next_deadline) passed.
    Tick,
}

/// What the daemon must do after a step, in order. A transition comes
/// first and is stored before the rest is carried out (`ARC-10`). Each is
/// safe to carry out again after a restart: starting a container first
/// looks for the run's, opening a pull request first looks for one on the
/// branch.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunEffect {
    Transition {
        from: RunStage,
        to: RunStage,
        reason: String,
    },
    StartContainer {
        generation: u64,
    },
    StartBuilder {
        generation: u64,
        attempt: u32,
        feedback: Option<Feedback>,
    },
    RunChecks {
        generation: u64,
    },
    /// Push the branch and open a draft pull request if there is none.
    PublishBranch {
        generation: u64,
    },
    StartReviewer {
        generation: u64,
    },
    /// Mark the pull request ready and ask the person to review it.
    MarkReady,
    /// Put the pull request back to draft while work goes on.
    MarkDraft,
    /// Stop whatever Muster runs for the run: an agent or the checks.
    StopWork,
    /// Start the building agent interactively, resuming its conversation.
    StartInteractive,
    RemoveContainer,
}
