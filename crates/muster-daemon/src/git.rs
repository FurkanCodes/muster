//! Running git. Commands run without a shell, with a time limit, and without
//! the git variables of whatever started the daemon, so a daemon launched
//! from inside a git hook still talks to the repository it was asked about.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

/// How long one git command may take: a worktree checkout of a large
/// repository can take a while, but never this long.
const TIME_LIMIT: Duration = Duration::from_secs(120);

/// Variables that would point git somewhere other than `cwd`.
const SCRUBBED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_PREFIX",
];

/// Why a git command failed.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GitError {
    #[error("could not run git: {0}")]
    Spawn(std::io::Error),
    #[error("`git {args}` failed: {stderr}")]
    Failed { args: String, stderr: String },
    #[error("`git {0}` did not finish within {TIME_LIMIT:?}")]
    TimedOut(String),
}

/// What a git command printed, and whether it succeeded.
#[derive(Debug)]
struct Outcome {
    success: bool,
    stdout: String,
    stderr: String,
}

async fn run(cwd: &Path, args: &[&str]) -> Result<Outcome, GitError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for variable in SCRUBBED {
        command.env_remove(variable);
    }
    let output = tokio::time::timeout(TIME_LIMIT, command.output())
        .await
        .map_err(|_| GitError::TimedOut(args.join(" ")))?
        .map_err(GitError::Spawn)?;
    Ok(Outcome {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    })
}

/// Runs a git command that must succeed and returns what it printed.
pub(crate) async fn checked(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    let outcome = run(cwd, args).await?;
    if outcome.success {
        Ok(outcome.stdout)
    } else {
        Err(GitError::Failed {
            args: args.join(" "),
            stderr: reason(&outcome.stderr),
        })
    }
}

/// The part of git's stderr that says what went wrong. Git mixes progress
/// ("Preparing worktree…") with its errors; the `fatal:` and `error:` lines
/// are the reason, so they come alone when there are any, on one line.
fn reason(stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let errors: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| line.starts_with("fatal:") || line.starts_with("error:"))
        .collect();
    let chosen = if errors.is_empty() { lines } else { errors };
    if chosen.is_empty() {
        "git gave no reason".to_owned()
    } else {
        chosen.join(" · ")
    }
}

/// Whether `name` is acceptable as a new branch's name.
pub(crate) async fn is_valid_branch_name(repo: &Path, name: &str) -> Result<bool, GitError> {
    Ok(run(repo, &["check-ref-format", "--branch", name])
        .await?
        .success)
}

/// Whether the repository has a local branch called `name`.
pub(crate) async fn branch_exists(repo: &Path, name: &str) -> Result<bool, GitError> {
    let reference = format!("refs/heads/{name}");
    Ok(run(repo, &["show-ref", "--verify", "--quiet", &reference])
        .await?
        .success)
}

/// Adds a worktree at `path`: on the existing `branch`, or on a new branch
/// of that name from the repository's current commit.
pub(crate) async fn add_worktree(
    repo: &Path,
    path: &Path,
    branch: &str,
    create_branch: bool,
) -> Result<(), GitError> {
    let path = path.to_string_lossy();
    let args: Vec<&str> = if create_branch {
        vec!["worktree", "add", "-b", branch, &path]
    } else {
        vec!["worktree", "add", &path, branch]
    };
    checked(repo, &args).await.map(drop)
}

/// Removes the worktree at `path`; with `force`, even if it has changes.
pub(crate) async fn remove_worktree(repo: &Path, path: &Path, force: bool) -> Result<(), GitError> {
    let path = path.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path);
    checked(repo, &args).await.map(drop)
}

/// Whether a git command succeeded, and what it printed; for questions
/// whose "no" is a failure exit.
pub(crate) async fn answer(cwd: &Path, args: &[&str]) -> Result<Option<String>, GitError> {
    let outcome = run(cwd, args).await?;
    Ok(outcome.success.then_some(outcome.stdout))
}

/// Git's own list of the repository's worktrees, in its stable porcelain
/// form with NUL separators (see `worktree_sync::parse`).
pub(crate) async fn list_worktrees(repo: &Path) -> Result<String, GitError> {
    checked(repo, &["worktree", "list", "--porcelain", "-z"]).await
}

/// Forgets worktrees whose directories no longer exist.
pub(crate) async fn prune_worktrees(repo: &Path) -> Result<(), GitError> {
    checked(repo, &["worktree", "prune"]).await.map(drop)
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};
    use std::process::Command;

    use super::*;

    /// Runs git in `dir`, ignoring the user's own git configuration so tests
    /// behave the same everywhere; panics if it fails.
    pub(crate) fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// Creates a git repository with one commit on `main`.
    pub(crate) fn repository(parent: &Path, name: &str) -> PathBuf {
        let root = parent.join(name);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(root.join("README.md"), "test\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "--quiet", "--message", "first"]);
        root.canonicalize().unwrap()
    }

    #[tokio::test]
    async fn branch_names_are_checked_by_git() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        assert!(
            is_valid_branch_name(&repo, "muster/fix-login")
                .await
                .unwrap()
        );
        assert!(!is_valid_branch_name(&repo, "bad..name").await.unwrap());
        assert!(!is_valid_branch_name(&repo, "-leading-dash").await.unwrap());
    }

    #[tokio::test]
    async fn worktrees_are_added_on_new_or_existing_branches_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        let fresh = dir.path().join("fresh");
        let existing = dir.path().join("existing");

        assert!(!branch_exists(&repo, "feature").await.unwrap());
        add_worktree(&repo, &fresh, "feature", true).await.unwrap();
        assert!(fresh.join("README.md").exists());
        assert!(branch_exists(&repo, "feature").await.unwrap());

        checked(&repo, &["branch", "other"]).await.unwrap();
        add_worktree(&repo, &existing, "other", false)
            .await
            .unwrap();
        let error = add_worktree(&repo, &dir.path().join("again"), "other", false)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("already"), "{error}");

        std::fs::write(fresh.join("README.md"), "changed\n").unwrap();
        assert!(remove_worktree(&repo, &fresh, false).await.is_err());
        remove_worktree(&repo, &fresh, true).await.unwrap();
        assert!(!fresh.exists());
        assert!(
            branch_exists(&repo, "feature").await.unwrap(),
            "the branch is kept"
        );
    }

    #[test]
    fn the_reason_is_gits_error_without_its_progress() {
        let stderr = "Preparing worktree (new branch 'test')\nfatal: could not create directory\n";
        assert_eq!(reason(stderr), "fatal: could not create directory");
        assert_eq!(
            reason("error: one\nhint: try this\nfatal: two\n"),
            "error: one · fatal: two"
        );
        assert_eq!(
            reason("something odd\nand more\n"),
            "something odd · and more"
        );
        assert_eq!(reason("\n  \n"), "git gave no reason");
    }

    #[tokio::test]
    async fn a_failed_command_reports_git_stderr() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        let error = checked(&repo, &["rev-parse", "does-not-exist"])
            .await
            .unwrap_err();
        assert!(matches!(error, GitError::Failed { .. }));
        assert!(error.to_string().contains("rev-parse does-not-exist"));
    }
}
