//! A run's stage, as the person sees it (`PRODUCT.md` §5).

use serde::{Deserialize, Serialize};

/// Where a run is on its way from ticket to merged pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStage {
    /// Picked up; waiting for room to run.
    Queued,
    /// Its container is starting and the code is being checked out.
    Provisioning,
    /// The building agent is working on the ticket.
    Building,
    /// The project's checks are running on the agent's work.
    Checking,
    /// The branch is being pushed and its draft pull request opened.
    OpeningPr,
    /// A reviewer agent is reviewing the pull request.
    AgentReview,
    /// The pull request is ready and waits on the person's review.
    ReadyForYou,
    /// A person is driving the building agent by hand.
    TakenOver,
    /// Stopped at a limit or a failure, waiting on the person.
    NeedsYou,
    /// The pull request was merged.
    Merged,
    /// The person cancelled the run.
    Cancelled,
}

impl RunStage {
    /// Whether the run has ended; nothing moves it after this.
    pub const fn is_over(self) -> bool {
        matches!(self, Self::Merged | Self::Cancelled)
    }

    /// Whether Muster is working on the run on its own, which is what its
    /// time limit counts (`PRD-12`); waiting on a person is not spending.
    pub const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Provisioning
                | Self::Building
                | Self::Checking
                | Self::OpeningPr
                | Self::AgentReview
        )
    }
}

/// The ticket a run works on, as it was when the run picked it up: later
/// edits do not change what the run was asked to do (`PRODUCT.md` §5).
/// Anyone can write it, so it reaches agents only as data (`PRD-07`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub number: u64,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub author: Option<String>,
}

/// A pull request a run opened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
}

/// One ticket on its way to a merged pull request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: crate::RunId,
    pub project: crate::ProjectId,
    pub ticket: Ticket,
    /// The branch its work goes on: `muster/issue-<n>-<slug>`.
    pub branch: String,
    pub stage: RunStage,
    /// Building attempts started in all.
    pub attempt: u32,
    /// Attempts allowed per budget.
    pub max_attempts: u32,
    /// Why it needs the person, while it does.
    pub reason: Option<String>,
    pub pr: Option<PullRequest>,
    /// What it has cost, where the harness reports it, in millionths of a
    /// dollar.
    pub cost_micros: Option<u64>,
    pub created_at: crate::Timestamp,
    pub ended_at: Option<crate::Timestamp>,
}

/// One step in a run's timeline, with its reason.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunTransition {
    pub from: RunStage,
    pub to: RunStage,
    pub reason: String,
    pub at: crate::Timestamp,
}
