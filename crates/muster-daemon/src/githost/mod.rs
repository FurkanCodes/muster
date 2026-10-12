//! Pull requests on each project's git host (`ARCHITECTURE.md` §10):
//! GitHub through the person's `gh`, GitLab through their `glab`, each
//! with their own login and the repository found from the checkout's
//! remote; Bitbucket through its API with a token the person gave Muster.
//! Behind a trait, with a fake in tests (`ARC-03`).
//!
//! Muster pushes and opens pull requests as the person, so they are the
//! author, and GitHub lets no author approve or ask for changes on their
//! own pull request, nor be asked to review it: the reviewer agent's
//! verdict is posted as a review comment, and a ready pull request waits
//! in Needs you.

use std::path::Path;

use muster_core::PullRequest;

pub(crate) mod choose;
pub(crate) mod github;
pub(crate) mod gitlab;

pub(crate) use github::GitHub;

use crate::service::{BoxFuture, ServiceError};

/// Where a pull request stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrState {
    Open,
    Merged,
    Closed,
}

/// What runs need from the git host.
pub(crate) trait GitHost: Send + Sync + std::fmt::Debug {
    /// The open pull request for `branch`, or a new draft one into `base`.
    fn open_draft<'a>(
        &'a self,
        root: &'a Path,
        branch: &'a str,
        base: &'a str,
        title: &'a str,
        body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, ServiceError>>;

    fn mark_ready<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>>;

    fn mark_draft<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), ServiceError>>;

    /// Posts a review comment.
    fn review_comment<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), ServiceError>>;

    fn state<'a>(
        &'a self,
        root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<PrState, ServiceError>>;
}
