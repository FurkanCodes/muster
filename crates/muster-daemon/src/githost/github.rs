//! GitHub pull requests, through the person's `gh`.

use std::path::Path;

use muster_core::PullRequest;
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
}
