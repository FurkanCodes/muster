//! A run's stage, as the person sees it (`PRODUCT.md` §5).

use serde::{Deserialize, Serialize};

/// Where a run is on its way from ticket to merged pull request.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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

    /// Whether the run waits on the person: it is ready for them, or
    /// stopped for them (`PRODUCT.md` §6, Needs you).
    pub const fn waits_on_person(self) -> bool {
        matches!(self, Self::ReadyForYou | Self::NeedsYou)
    }

    /// Where the stage sorts in a list of runs: what waits on the person
    /// first, then the work in the order it happens back to front, then
    /// what has ended.
    pub const fn list_order(self) -> u8 {
        match self {
            Self::NeedsYou => 0,
            Self::ReadyForYou => 1,
            Self::TakenOver => 2,
            Self::AgentReview => 3,
            Self::OpeningPr => 4,
            Self::Checking => 5,
            Self::Building => 6,
            Self::Provisioning => 7,
            Self::Queued => 8,
            Self::Merged => 9,
            Self::Cancelled => 10,
        }
    }

    /// The stage in a word or two, for lists.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Provisioning => "starting",
            Self::Building => "building",
            Self::Checking => "checking",
            Self::OpeningPr => "opening PR",
            Self::AgentReview => "in review",
            Self::ReadyForYou => "ready for you",
            Self::TakenOver => "taken over",
            Self::NeedsYou => "needs you",
            Self::Merged => "merged",
            Self::Cancelled => "cancelled",
        }
    }

    /// The stage as it reads in a sentence: "while it is …".
    pub const fn phrase(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Provisioning => "starting its container",
            Self::Building => "building",
            Self::Checking => "checking",
            Self::OpeningPr => "opening its pull request",
            Self::AgentReview => "in review",
            Self::ReadyForYou => "ready for you",
            Self::TakenOver => "taken over",
            Self::NeedsYou => "waiting on you",
            Self::Merged => "merged",
            Self::Cancelled => "cancelled",
        }
    }
}

/// The ticket a run works on, as it was when the run picked it up: later
/// edits do not change what the run was asked to do (`PRODUCT.md` §5).
/// Anyone can write it, so it reaches agents only as data (`PRD-07`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
}

/// One ticket on its way to a merged pull request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct RunTransition {
    pub from: RunStage,
    pub to: RunStage,
    pub reason: String,
    pub at: crate::Timestamp,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_say_who_they_wait_on() {
        let all = [
            RunStage::Queued,
            RunStage::Provisioning,
            RunStage::Building,
            RunStage::Checking,
            RunStage::OpeningPr,
            RunStage::AgentReview,
            RunStage::ReadyForYou,
            RunStage::TakenOver,
            RunStage::NeedsYou,
            RunStage::Merged,
            RunStage::Cancelled,
        ];
        for stage in all {
            // A run is at most one of: working alone, waiting on the
            // person, over.
            let kinds = [stage.is_active(), stage.waits_on_person(), stage.is_over()];
            assert!(kinds.iter().filter(|kind| **kind).count() <= 1, "{stage:?}");
            assert!(!stage.label().is_empty() && !stage.phrase().is_empty());
        }
        assert!(RunStage::NeedsYou.waits_on_person());
        assert!(RunStage::ReadyForYou.waits_on_person());
        assert!(!RunStage::TakenOver.waits_on_person());
        assert_eq!(RunStage::Provisioning.phrase(), "starting its container");
    }
}
