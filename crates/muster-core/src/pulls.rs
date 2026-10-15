//! Where a branch's pull request stands on its project's git host, as the
//! daemon last looked (`PRODUCT.md` §6): shown beside each worktree and on
//! the tickets whose agents work there. Looked at now and then, only read
//! from the host, and never stored.

use serde::{Deserialize, Serialize};

/// A branch's pull request (a merge request, on GitLab), as last looked at.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestStatus {
    pub number: u64,
    /// Its page on the host.
    pub url: String,
    pub state: PullRequestState,
    /// How its checks stand; `None` when none ran, or the host does not
    /// say.
    #[serde(default)]
    pub checks: Option<Checks>,
    /// What its reviews come to; `None` when none is asked for, or the
    /// host does not say.
    #[serde(default)]
    pub review: Option<Review>,
}

impl PullRequestStatus {
    /// Where it stands, in words: `#42 · open · checks failed · changes
    /// requested`. Its checks and reviews only while it is open.
    pub fn summary(&self) -> String {
        let mut parts = vec![format!("#{}", self.number), self.state.word().to_owned()];
        if self.state.is_open() {
            parts.extend(self.checks.map(|checks| checks.words().to_owned()));
            parts.extend(self.review.map(|review| review.words().to_owned()));
        }
        parts.join(" · ")
    }
}

/// Where a pull request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PullRequestState {
    /// Open, and marked as a draft.
    Draft,
    Open,
    Merged,
    /// Closed without being merged (declined, on Bitbucket).
    Closed,
}

impl PullRequestState {
    /// Whether it is still open, a draft or not.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Draft | Self::Open)
    }

    pub fn word(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Merged => "merged",
            Self::Closed => "closed",
        }
    }
}

/// How a pull request's checks stand, all taken together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Checks {
    /// Some still run or wait, and none has failed.
    Pending,
    /// Every one passed, or was skipped.
    Passing,
    /// At least one failed, errored, timed out or was cancelled.
    Failing,
}

impl Checks {
    pub fn words(self) -> &'static str {
        match self {
            Self::Pending => "checks running",
            Self::Passing => "checks passed",
            Self::Failing => "checks failed",
        }
    }

    /// Every check together: one failing fails them all, then one still
    /// going keeps them pending. `None` when there are none.
    pub fn fold(each: impl IntoIterator<Item = Self>) -> Option<Self> {
        each.into_iter().fold(None, |all, one| match (all, one) {
            (Some(Self::Failing), _) | (_, Self::Failing) => Some(Self::Failing),
            (Some(Self::Pending), _) | (_, Self::Pending) => Some(Self::Pending),
            _ => Some(Self::Passing),
        })
    }
}

/// What a pull request's reviews come to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Review {
    /// A review is asked for, and nobody has approved it yet.
    Required,
    Approved,
    ChangesRequested,
}

impl Review {
    pub fn words(self) -> &'static str {
        match self {
            Self::Required => "review required",
            Self::Approved => "approved",
            Self::ChangesRequested => "changes requested",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_failing_check_fails_them_all_and_one_going_keeps_them_pending() {
        use Checks::{Failing, Passing, Pending};
        assert_eq!(Checks::fold([]), None);
        assert_eq!(Checks::fold([Passing, Passing]), Some(Passing));
        assert_eq!(Checks::fold([Passing, Pending, Passing]), Some(Pending));
        assert_eq!(Checks::fold([Pending, Failing, Passing]), Some(Failing));
        assert_eq!(Checks::fold([Failing, Pending]), Some(Failing));
    }

    #[test]
    fn a_summary_says_checks_and_reviews_only_while_it_is_open() {
        let mut status = PullRequestStatus {
            number: 42,
            url: "https://github.com/o/app/pull/42".to_owned(),
            state: PullRequestState::Open,
            checks: Some(Checks::Failing),
            review: Some(Review::ChangesRequested),
        };
        assert_eq!(
            status.summary(),
            "#42 · open · checks failed · changes requested"
        );
        status.state = PullRequestState::Draft;
        status.review = None;
        assert_eq!(status.summary(), "#42 · draft · checks failed");
        status.state = PullRequestState::Merged;
        assert_eq!(status.summary(), "#42 · merged");
    }

    #[test]
    fn a_status_from_a_daemon_that_does_not_say_its_checks_or_review_reads() {
        // Named fields, as frames are written; the optional ones left out.
        #[derive(Serialize)]
        struct Older {
            number: u64,
            url: String,
            state: PullRequestState,
        }
        let older = Older {
            number: 42,
            url: "https://github.com/o/app/pull/42".to_owned(),
            state: PullRequestState::Draft,
        };
        let bytes = rmp_serde::to_vec_named(&older).unwrap();
        let read: PullRequestStatus = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read.number, 42);
        assert_eq!(read.state, PullRequestState::Draft);
        assert_eq!((read.checks, read.review), (None, None));
        assert!(read.state.is_open() && !PullRequestState::Merged.is_open());
    }
}
