//! GitHub pull requests, through the person's `gh`.

use std::path::Path;

use muster_core::{Checks, PullRequest, PullRequestState, PullRequestStatus, Review};
use serde::Deserialize;

use super::{GitHost, PrState};
use crate::github::gh_with;
use crate::service::{BoxFuture, ServiceError};

/// GitHub, through `gh`.
#[derive(Debug, Default)]
pub(crate) struct GitHub;

#[derive(Deserialize)]
struct Listed {
    number: u64,
    url: String,
}

impl GitHost for GitHub {
    fn open_draft<'a>(
        &'a self,
        root: &'a Path,
        branch: &'a str,
        base: &'a str,
        title: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, ServiceError>> {
        Box::pin(async move {
            let listed = gh_with(
                root,
                &[
                    "pr",
                    "list",
                    "--head",
                    branch,
                    "--state",
                    "open",
                    "--json",
                    "number,url",
                    "--limit",
                    "1",
                ],
                None,
            )
            .await?;
            let open: Vec<Listed> = serde_json::from_str(&listed)
                .map_err(|error| ServiceError::Unreadable(error.to_string()))?;
            if let Some(found) = open.into_iter().next() {
                return Ok(PullRequest {
                    number: found.number,
                    url: found.url,
                });
            }
            let created = gh_with(
                root,
                &[
                    "pr",
                    "create",
                    "--draft",
                    "--base",
                    base,
                    "--head",
                    branch,
                    "--title",
                    title,
                    "--body-file",
                    "-",
                ],
                Some(body),
            )
            .await?;
            let url = created
                .lines()
                .rev()
                .find(|line| line.contains("/pull/"))
                .unwrap_or_default()
                .trim()
                .to_owned();
            let number = url
                .rsplit('/')
                .next()
                .and_then(|tail| tail.parse().ok())
                .ok_or_else(|| {
                    ServiceError::Unreadable(format!("no pull request in {created:?}"))
                })?;
            Ok(PullRequest { number, url })
        })
    }

    fn mark_ready<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            gh_with(root, &["pr", "ready", &number.to_string()], None)
                .await
                .map(drop)
                .map_err(ServiceError::from)
        })
    }

    fn mark_draft<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            gh_with(root, &["pr", "ready", &number.to_string(), "--undo"], None)
                .await
                .map(drop)
                .map_err(ServiceError::from)
        })
    }

    fn review_comment<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            gh_with(
                root,
                &[
                    "pr",
                    "review",
                    &number.to_string(),
                    "--comment",
                    "--body-file",
                    "-",
                ],
                Some(body),
            )
            .await
            .map(drop)
            .map_err(ServiceError::from)
        })
    }

    fn state<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<PrState, ServiceError>> {
        Box::pin(async move {
            #[derive(Deserialize)]
            struct View {
                state: String,
            }
            let json = gh_with(
                root,
                &["pr", "view", &number.to_string(), "--json", "state"],
                None,
            )
            .await?;
            let view: View = serde_json::from_str(&json)
                .map_err(|error| ServiceError::Unreadable(error.to_string()))?;
            Ok(match view.state.as_str() {
                "MERGED" => PrState::Merged,
                "CLOSED" => PrState::Closed,
                _ => PrState::Open,
            })
        })
    }

    fn pull_request_of<'a>(
        &'a self,
        root: &'a Path,
        branch: &'a str,
    ) -> BoxFuture<'a, Result<Option<PullRequestStatus>, ServiceError>> {
        Box::pin(async move {
            // Newest first, so a branch's latest pull request wins.
            let json = gh_with(
                root,
                &[
                    "pr",
                    "list",
                    "--head",
                    branch,
                    "--state",
                    "all",
                    "--json",
                    "number,url,state,isDraft,reviewDecision,statusCheckRollup",
                    "--limit",
                    "1",
                ],
                None,
            )
            .await?;
            let found: Vec<Found> = serde_json::from_str(&json)
                .map_err(|error| ServiceError::Unreadable(error.to_string()))?;
            Ok(found.into_iter().next().map(Found::status))
        })
    }
}

/// `gh pr list --json`'s pull request, as it says where it stands.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Found {
    number: u64,
    url: String,
    /// `OPEN`, `CLOSED` or `MERGED`.
    #[serde(default)]
    state: String,
    #[serde(default)]
    is_draft: bool,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED`, or empty when
    /// no review is asked for.
    #[serde(default)]
    review_decision: Option<String>,
    #[serde(default)]
    status_check_rollup: Option<Vec<Check>>,
}

/// One check on the pull request's latest commit: a check run (GitHub
/// Actions and other apps), or a commit status (older integrations).
#[derive(Debug, Deserialize)]
struct Check {
    #[serde(rename = "__typename", default)]
    kind: String,
    /// A check run's: `COMPLETED`, or `QUEUED`, `IN_PROGRESS`… while it
    /// is not done.
    #[serde(default)]
    status: Option<String>,
    /// A check run's, once completed.
    #[serde(default)]
    conclusion: Option<String>,
    /// A commit status's: `SUCCESS`, `FAILURE`, `ERROR`, `PENDING`,
    /// `EXPECTED`.
    #[serde(default)]
    state: Option<String>,
}

impl Found {
    fn status(self) -> PullRequestStatus {
        let state = match (self.state.as_str(), self.is_draft) {
            ("MERGED", _) => PullRequestState::Merged,
            ("CLOSED", _) => PullRequestState::Closed,
            (_, true) => PullRequestState::Draft,
            (_, false) => PullRequestState::Open,
        };
        let review = match self.review_decision.as_deref() {
            Some("APPROVED") => Some(Review::Approved),
            Some("CHANGES_REQUESTED") => Some(Review::ChangesRequested),
            Some("REVIEW_REQUIRED") => Some(Review::Required),
            _ => None,
        };
        PullRequestStatus {
            number: self.number,
            url: self.url,
            state,
            checks: Checks::fold(
                self.status_check_rollup
                    .unwrap_or_default()
                    .iter()
                    .map(Check::outcome),
            ),
            review,
        }
    }
}

impl Check {
    /// How this one check stands, as GitHub's own page reads it: skipped
    /// and neutral ones pass, cancelled and timed-out ones fail.
    fn outcome(&self) -> Checks {
        if self.kind == "StatusContext" || (self.status.is_none() && self.state.is_some()) {
            return match self.state.as_deref() {
                Some("SUCCESS") => Checks::Passing,
                Some("FAILURE" | "ERROR") => Checks::Failing,
                _ => Checks::Pending,
            };
        }
        match (self.status.as_deref(), self.conclusion.as_deref()) {
            (Some("COMPLETED"), Some("SUCCESS" | "NEUTRAL" | "SKIPPED")) => Checks::Passing,
            (
                Some("COMPLETED"),
                Some("FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE"),
            ) => Checks::Failing,
            _ => Checks::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(json: &str) -> PullRequestStatus {
        let listed: Vec<Found> = serde_json::from_str(json).unwrap();
        listed.into_iter().next().unwrap().status()
    }

    #[test]
    fn a_pull_request_reads_as_gh_lists_it() {
        // As gh 2.96 answered for a public repository, trimmed.
        let merged = found(
            r#"[{"isDraft":false,"number":14697,"reviewDecision":"APPROVED","state":"MERGED","statusCheckRollup":[{"__typename":"CheckRun","completedAt":"2026-10-14T20:56:42Z","conclusion":"SKIPPED","detailsUrl":"https://github.com/cli/cli/actions/runs/1/job/2","name":"label-external","startedAt":"2026-10-14T20:56:43Z","status":"COMPLETED","workflowName":"PR Triaging"},{"__typename":"CheckRun","conclusion":"SUCCESS","name":"build","status":"COMPLETED"}],"url":"https://github.com/cli/cli/pull/14697"}]"#,
        );
        assert_eq!(
            merged,
            PullRequestStatus {
                number: 14697,
                url: "https://github.com/cli/cli/pull/14697".to_owned(),
                state: PullRequestState::Merged,
                checks: Some(Checks::Passing),
                review: Some(Review::Approved),
            }
        );
        let draft = found(
            r#"[{"isDraft":true,"number":5,"reviewDecision":"","state":"OPEN","statusCheckRollup":[],"url":"https://github.com/o/app/pull/5"}]"#,
        );
        assert_eq!(draft.state, PullRequestState::Draft);
        assert_eq!((draft.checks, draft.review), (None, None));
        // What an older stand-in, or a trimmed answer, gives.
        let bare = found(r#"[{"number":17,"url":"https://github.com/o/app/pull/17"}]"#);
        assert_eq!(bare.state, PullRequestState::Open);
        let closed = found(
            r#"[{"isDraft":true,"number":6,"reviewDecision":"CHANGES_REQUESTED","state":"CLOSED","statusCheckRollup":null,"url":"u"}]"#,
        );
        assert_eq!(closed.state, PullRequestState::Closed);
        assert_eq!(closed.review, Some(Review::ChangesRequested));
    }

    #[test]
    fn checks_fail_on_any_failure_and_wait_on_any_still_going() {
        let checks = |rollup: &str| {
            found(&format!(
                r#"[{{"number":1,"url":"u","state":"OPEN","reviewDecision":"REVIEW_REQUIRED","statusCheckRollup":{rollup}}}]"#
            ))
            .checks
        };
        let run = |status: &str, conclusion: &str| {
            format!(
                r#"{{"__typename":"CheckRun","status":"{status}","conclusion":"{conclusion}"}}"#
            )
        };
        let context = |state: &str| {
            format!(r#"{{"__typename":"StatusContext","context":"ci","state":"{state}"}}"#)
        };
        let passing = run("COMPLETED", "SUCCESS");
        assert_eq!(checks(&format!("[{passing}]")), Some(Checks::Passing));
        for failed in [
            "FAILURE",
            "TIMED_OUT",
            "CANCELLED",
            "ACTION_REQUIRED",
            "STARTUP_FAILURE",
        ] {
            let failing = run("COMPLETED", failed);
            assert_eq!(
                checks(&format!("[{passing},{failing}]")),
                Some(Checks::Failing),
                "{failed}"
            );
        }
        let going = run("IN_PROGRESS", "");
        assert_eq!(
            checks(&format!("[{passing},{going}]")),
            Some(Checks::Pending)
        );
        assert_eq!(
            checks(&format!("[{going},{}]", run("COMPLETED", "FAILURE"))),
            Some(Checks::Failing)
        );
        assert_eq!(
            checks(&format!("[{}]", context("SUCCESS"))),
            Some(Checks::Passing)
        );
        assert_eq!(
            checks(&format!("[{}]", context("PENDING"))),
            Some(Checks::Pending)
        );
        assert_eq!(
            checks(&format!("[{}]", context("EXPECTED"))),
            Some(Checks::Pending)
        );
        assert_eq!(
            checks(&format!("[{passing},{}]", context("ERROR"))),
            Some(Checks::Failing)
        );
    }
}
