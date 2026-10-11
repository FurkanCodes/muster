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
