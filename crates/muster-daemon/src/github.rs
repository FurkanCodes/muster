//! The GitHub CLI (`gh`), as Nebula uses it: the person's own `gh` login
//! covers authentication and GitHub Enterprise, and `gh` finds the
//! repository from the checkout's remote. Issues are read through it in
//! `trackers::github`, pull requests in `githost`.

use std::path::Path;
use std::time::Duration;

use tokio::process::Command;

/// How long one `gh` call may take; it talks to GitHub.
const TIME_LIMIT: Duration = Duration::from_secs(30);

/// Why `gh` could not answer.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GitHubError {
    #[error("the GitHub CLI (gh) is not installed; install it and run `gh auth login`")]
    Missing,
    #[error("could not run gh: {0}")]
    Spawn(std::io::Error),
    #[error("gh did not answer within {0:?}")]
    TimedOut(Duration),
    #[error("gh: {0}")]
    Failed(String),
    #[error("gh answered something unexpected: {0}")]
    Unreadable(String),
}

pub(crate) use crate::service::BoxFuture;

/// Runs `gh` in `repo` with `input` on its stdin, if any; its output, or
/// its complaint.
pub(crate) async fn gh_with(
    repo: &Path,
    args: &[&str],
    input: Option<&str>,
) -> Result<String, GitHubError> {
    gh_within(repo, args, input, TIME_LIMIT).await
}

/// [`gh_with`], allowed `limit`: a clone takes as long as the repository is big.
pub(crate) async fn gh_within(
    repo: &Path,
    args: &[&str],
    input: Option<&str>,
    limit: Duration,
) -> Result<String, GitHubError> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .current_dir(repo)
        // Never prompt: there is no one at this terminal.
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1");
    let ran = crate::process::run(command, input.map(str::as_bytes), limit)
        .await
        .map_err(|error| match error {
            crate::process::RunError::TimedOut(limit) => GitHubError::TimedOut(limit),
            crate::process::RunError::NotFound => GitHubError::Missing,
            crate::process::RunError::Spawn(error) => GitHubError::Spawn(error),
        })?;
    if !ran.success() {
        return Err(GitHubError::Failed(ran.stderr_text()));
    }
    Ok(ran.stdout_text())
}
