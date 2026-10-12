//! GitLab merge requests, through the person's `glab`. A merge request is
//! opened as a draft from a branch Muster has pushed with git (`glab mr
//! create` pushes nothing unless told to), and its number is its `iid`.

use std::path::Path;

use muster_core::PullRequest;
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
}
