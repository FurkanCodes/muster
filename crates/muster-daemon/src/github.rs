//! The GitHub CLI (`gh`), as Nebula uses it: the person's own `gh` login
//! covers authentication and GitHub Enterprise, and `gh` finds the
//! repository from the checkout's remote. Issues are read through it in
//! `trackers::github`, pull requests in `githost`.

use std::path::Path;
use std::process::Stdio;
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
    #[error("gh did not answer within {TIME_LIMIT:?}")]
    TimedOut,
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
    use tokio::io::AsyncWriteExt;
    let mut command = Command::new("gh");
    command
        .args(args)
        .current_dir(repo)
        // Never prompt: there is no one at this terminal.
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let work = async {
        let mut child = command.spawn()?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            let _ = stdin.write_all(input.as_bytes()).await;
            drop(stdin);
        }
        child.wait_with_output().await
    };
    let output = match tokio::time::timeout(TIME_LIMIT, work).await {
        Err(_) => return Err(GitHubError::TimedOut),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(GitHubError::Missing);
        }
        Ok(Err(error)) => return Err(GitHubError::Spawn(error)),
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(GitHubError::Failed(stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
