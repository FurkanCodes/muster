//! The GitLab CLI (`glab`), as `gh` is used for GitHub: the person's own
//! `glab auth login` covers authentication, gitlab.com and self-managed
//! hosts alike, and `glab` finds the project from the checkout's remote.
//! Issues are read through it in `trackers::gitlab`, merge requests in
//! `githost::gitlab`.
//!
//! It never prompts: there is no one at its terminal. Checked against
//! glab 1.122: `--output json` on lists and views, `GLAB_NO_PROMPT`, and
//! `mr create` pushing nothing without `--push` or `--fill`.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::service::ServiceError;

/// How long one `glab` call may take; it talks to GitLab.
const TIME_LIMIT: Duration = Duration::from_secs(30);

/// Runs `glab` in `repo`; its output, or its complaint.
pub(crate) async fn glab(repo: &Path, args: &[&str]) -> Result<String, ServiceError> {
    let mut command = Command::new("glab");
    command
        .args(args)
        .current_dir(repo)
        .env("GLAB_NO_PROMPT", "true")
        .env("NO_PROMPT", "true")
        .env("NO_COLOR", "1")
        .env("GLAB_CHECK_UPDATE", "false")
        .env("GLAB_SHOW_WHATS_NEW", "false")
        .env("GLAB_PAGER", "cat")
        .env("PAGER", "cat")
        // A stray editor fails rather than waits.
        .env("GLAB_EDITOR", "false")
        .env("VISUAL", "false")
        .env("EDITOR", "false")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = match tokio::time::timeout(TIME_LIMIT, command.output()).await {
        Err(_) => {
            return Err(ServiceError::Unavailable(format!(
                "glab did not answer within {TIME_LIMIT:?}"
            )));
        }
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ServiceError::Unavailable(
                "the GitLab CLI (glab) is not installed; install it and run `glab auth login`"
                    .to_owned(),
            ));
        }
        Ok(Err(error)) => {
            return Err(ServiceError::Unavailable(format!(
                "could not run glab: {error}"
            )));
        }
        Ok(Ok(output)) => output,
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        return Err(complaint(&stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// What `glab`'s complaint means.
fn complaint(stderr: &str) -> ServiceError {
    let message = format!("glab: {stderr}");
    let lower = stderr.to_ascii_lowercase();
    if crate::service::says_unreachable(stderr) {
        ServiceError::Unavailable(format!("{message}. {}", crate::service::REACH_HINT))
    } else if lower.contains("auth login")
        || lower.contains("401")
        || lower.contains("unauthorized")
    {
        ServiceError::NotSignedIn(message)
    } else if lower.contains("404") || lower.contains("not found") {
        ServiceError::NotFound(message)
    } else {
        ServiceError::Refused(message)
    }
}

/// `text` as JSON of type `T`, or what was wrong with it.
pub(crate) fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, ServiceError> {
    serde_json::from_str(text).map_err(|error| {
        ServiceError::Unreadable(format!("glab answered something unexpected: {error}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complaints_say_what_went_wrong() {
        assert!(matches!(
            complaint(
                "none of the git remotes configured for this repository point to a known GitLab host. Please use `glab auth login`"
            ),
            ServiceError::NotSignedIn(_)
        ));
        assert!(matches!(
            complaint("GET …/issues/9: 404 Not Found"),
            ServiceError::NotFound(_)
        ));
        assert!(matches!(
            complaint("403 Forbidden"),
            ServiceError::Refused(_)
        ));
        // A host on a company's network, its VPN off.
        let unreachable = complaint(
            "Get \"https://10.10.10.5/api/v4/projects\": dial tcp 10.10.10.5:443: i/o timeout",
        );
        assert!(matches!(unreachable, ServiceError::Unavailable(_)));
        assert!(unreachable.to_string().contains("VPN"), "{unreachable}");
    }
}
