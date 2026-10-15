//! GitLab merge requests, through the person's `glab`. A merge request is
//! opened as a draft from a branch Muster has pushed with git (`glab mr
//! create` pushes nothing unless told to), and its number is its `iid`.

use std::path::Path;

use muster_core::{Checks, PullRequest, PullRequestState, PullRequestStatus, Review};
use serde::Deserialize;

use super::{GitHost, PrState};
use crate::glab::{glab, parse};
use crate::service::{BoxFuture, ServiceError};

/// GitLab, through `glab`.
#[derive(Debug, Default)]
pub(crate) struct GitLab;

/// `glab`'s JSON for one merge request: GitLab's API fields.
#[derive(Debug, Deserialize)]
struct Listed {
    iid: u64,
    web_url: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    draft: bool,
    /// What stands between it and a merge: `not_approved` and
    /// `requested_changes` speak of its reviews (GitLab 15.6 and later).
    #[serde(default)]
    detailed_merge_status: Option<String>,
    /// Its latest pipeline; in a merge request's view, not in a list.
    #[serde(default)]
    head_pipeline: Option<Pipeline>,
}

#[derive(Debug, Deserialize)]
struct Pipeline {
    #[serde(default)]
    status: String,
}

impl Listed {
    fn pull_request(self) -> PullRequest {
        PullRequest {
            number: self.iid,
            url: self.web_url,
        }
    }
}

/// The open merge request from `branch`, if there is one.
async fn open_from(root: &Path, branch: &str) -> Result<Option<PullRequest>, ServiceError> {
    let json = glab(
        root,
        &[
            "mr",
            "list",
            "--source-branch",
            branch,
            "--output",
            "json",
            "--per-page",
            "5",
        ],
    )
    .await?;
    let open: Vec<Listed> = parse(&json)?;
    Ok(open.into_iter().next().map(Listed::pull_request))
}

impl GitHost for GitLab {
    fn open_draft<'a>(
        &'a self,
        root: &'a Path,
        branch: &'a str,
        base: &'a str,
        title: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, ServiceError>> {
        Box::pin(async move {
            if let Some(found) = open_from(root, branch).await? {
                return Ok(found);
            }
            glab(
                root,
                &[
                    "mr",
                    "create",
                    "--draft",
                    "--source-branch",
                    branch,
                    "--target-branch",
                    base,
                    "--title",
                    title,
                    "--description",
                    body,
                    "--yes",
                ],
            )
            .await?;
            // Its number and address, as GitLab has them now.
            open_from(root, branch).await?.ok_or_else(|| {
                ServiceError::Unreadable(format!(
                    "glab opened a merge request from {branch}, but none is listed"
                ))
            })
        })
    }

    fn mark_ready<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            glab(
                root,
                &["mr", "update", &number.to_string(), "--ready", "--yes"],
            )
            .await?;
            Ok(())
        })
    }

    fn mark_draft<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            glab(
                root,
                &["mr", "update", &number.to_string(), "--draft", "--yes"],
            )
            .await?;
            Ok(())
        })
    }

    fn review_comment<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            glab(
                root,
                &["mr", "note", &number.to_string(), "--message", body],
            )
            .await?;
            Ok(())
        })
    }

    fn state<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<PrState, ServiceError>> {
        Box::pin(async move {
            let json = glab(
                root,
                &["mr", "view", &number.to_string(), "--output", "json"],
            )
            .await?;
            let found: Listed = parse(&json)?;
            Ok(state(&found.state))
        })
    }

    fn pull_request_of<'a>(
        &'a self,
        root: &'a Path,
        branch: &'a str,
    ) -> BoxFuture<'a, Result<Option<PullRequestStatus>, ServiceError>> {
        Box::pin(async move {
            // Every state, newest first.
            let json = glab(
                root,
                &[
                    "mr",
                    "list",
                    "--source-branch",
                    branch,
                    "--all",
                    "--output",
                    "json",
                    "--per-page",
                    "1",
                ],
            )
            .await?;
            let listed: Vec<Listed> = parse(&json)?;
            let Some(found) = listed.into_iter().next() else {
                return Ok(None);
            };
            if state(&found.state) != PrState::Open {
                return Ok(Some(found.status(None)));
            }
            // Its pipeline is only in its view; without it, no checks said.
            let viewed = glab(
                root,
                &["mr", "view", &found.iid.to_string(), "--output", "json"],
            )
            .await
            .and_then(|json| parse::<Listed>(&json));
            let pipeline = match &viewed {
                Ok(view) => view.head_pipeline.as_ref().map(|run| run.status.clone()),
                Err(error) => {
                    tracing::debug!(%error, "a merge request's pipeline could not be read");
                    None
                }
            };
            Ok(Some(found.status(pipeline.as_deref())))
        })
    }
}

impl Listed {
    /// Where it stands; its checks from `pipeline`, its latest pipeline's
    /// status, which only a merge request's view carries.
    fn status(self, pipeline: Option<&str>) -> PullRequestStatus {
        let state = match (state(&self.state), self.draft) {
            (PrState::Merged, _) => PullRequestState::Merged,
            (PrState::Closed, _) => PullRequestState::Closed,
            (PrState::Open, true) => PullRequestState::Draft,
            (PrState::Open, false) => PullRequestState::Open,
        };
        let review = match self.detailed_merge_status.as_deref() {
            Some("not_approved") => Some(Review::Required),
            Some("requested_changes") => Some(Review::ChangesRequested),
            _ => None,
        };
        let checks = match pipeline {
            Some("success") => Some(Checks::Passing),
            Some("failed" | "canceled") => Some(Checks::Failing),
            Some(
                "created"
                | "waiting_for_resource"
                | "preparing"
                | "pending"
                | "running"
                | "manual"
                | "scheduled",
            ) => Some(Checks::Pending),
            _ => None,
        };
        PullRequestStatus {
            number: self.iid,
            url: self.web_url,
            state,
            checks,
            review,
        }
    }
}

/// GitLab's word for a merge request's state.
fn state(word: &str) -> PrState {
    match word {
        "merged" => PrState::Merged,
        "closed" | "locked" => PrState::Closed,
        _ => PrState::Open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glab_json_is_read() {
        let json = r#"[{"id":5501,"iid":17,"title":"Draft: Fix it","state":"opened","draft":true,"source_branch":"muster/issue-12-fix","target_branch":"main","web_url":"https://gitlab.example.com/g/app/-/merge_requests/17"}]"#;
        let listed: Vec<Listed> = parse(json).unwrap();
        let pr = listed.into_iter().next().unwrap().pull_request();
        assert_eq!(pr.number, 17);
        assert_eq!(
            pr.url,
            "https://gitlab.example.com/g/app/-/merge_requests/17"
        );
        assert_eq!(state("opened"), PrState::Open);
        assert_eq!(state("merged"), PrState::Merged);
        assert_eq!(state("closed"), PrState::Closed);
        assert_eq!(state("locked"), PrState::Closed);
    }

    #[test]
    fn a_merge_request_s_state_pipeline_and_reviews_are_read() {
        // GitLab's API fields, as its documentation shows them.
        let json = r#"{"id":5501,"iid":17,"state":"opened","draft":false,"detailed_merge_status":"not_approved","web_url":"https://gitlab.example.com/g/app/-/merge_requests/17","head_pipeline":{"id":9,"status":"running"}}"#;
        let viewed: Listed = parse(json).unwrap();
        let pipeline = viewed.head_pipeline.as_ref().map(|run| run.status.clone());
        let status = viewed.status(pipeline.as_deref());
        assert_eq!(
            status,
            PullRequestStatus {
                number: 17,
                url: "https://gitlab.example.com/g/app/-/merge_requests/17".to_owned(),
                state: PullRequestState::Open,
                checks: Some(Checks::Pending),
                review: Some(Review::Required),
            }
        );
        let listed = |state: &str, draft: bool, merge: &str| {
            let json = format!(
                r#"{{"iid":3,"web_url":"u","state":"{state}","draft":{draft},"detailed_merge_status":"{merge}"}}"#
            );
            parse::<Listed>(&json).unwrap()
        };
        let draft = listed("opened", true, "draft_status").status(Some("failed"));
        assert_eq!(draft.state, PullRequestState::Draft);
        assert_eq!((draft.checks, draft.review), (Some(Checks::Failing), None));
        let asked = listed("opened", false, "requested_changes").status(Some("success"));
        assert_eq!(asked.review, Some(Review::ChangesRequested));
        assert_eq!(asked.checks, Some(Checks::Passing));
        assert_eq!(
            listed("merged", false, "not_open").status(None).state,
            PullRequestState::Merged
        );
        assert_eq!(
            listed("closed", true, "not_open")
                .status(Some("skipped"))
                .checks,
            None
        );
    }
}
