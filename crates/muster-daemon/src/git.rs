//! Running git. Commands run without a shell, with a time limit, and without
//! the git variables of whatever started the daemon, so a daemon launched
//! from inside a git hook still talks to the repository it was asked about.

use std::ffi::OsStr;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt};
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

/// Whether the folder at `root` is a git repository: it has a `.git`
/// directory, or a `.git` file naming one, as a worktree does. A project
/// without one is a plain folder, also when it sits inside another
/// repository, whose branches and worktrees are not its own.
pub(crate) fn is_repository(root: &Path) -> bool {
    root.join(".git").try_exists().unwrap_or(false)
}

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

/// Variables one command sets for git, after the scrubbed ones are gone,
/// such as an index of Muster's own.
pub(crate) type Vars<'a> = &'a [(&'a str, &'a OsStr)];

/// Git run in `cwd` with `args`, its output piped, its variables scrubbed
/// and then `vars` set.
fn command(cwd: &Path, args: &[&str], vars: Vars<'_>) -> Command {
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
    for (name, value) in vars {
        command.env(name, value);
    }
    command
}

async fn run(cwd: &Path, args: &[&str]) -> Result<Outcome, GitError> {
    run_with(cwd, args, &[]).await
}

async fn run_with(cwd: &Path, args: &[&str], vars: Vars<'_>) -> Result<Outcome, GitError> {
    let mut command = command(cwd, args, vars);
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
    checked_with(cwd, args, &[]).await
}

/// [`checked`], with `vars` set for git.
pub(crate) async fn checked_with(
    cwd: &Path,
    args: &[&str],
    vars: Vars<'_>,
) -> Result<String, GitError> {
    let outcome = run_with(cwd, args, vars).await?;
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

/// The most of git's error output kept from a command read with
/// [`capped`].
const MAX_STDERR: usize = 64 * 1024;

/// What a git command printed, as it printed it (not trimmed: a listing
/// with NUL separators can start with a space), read only up to `limit`
/// bytes, with whether there was more; the command is stopped there. An
/// exit code in `ok` is success, as `git diff --no-index` exits 1 when
/// the files differ.
pub(crate) async fn capped(
    cwd: &Path,
    args: &[&str],
    limit: usize,
    ok: &[i32],
) -> Result<(Vec<u8>, bool), GitError> {
    capped_with(cwd, args, limit, ok, &[]).await
}

/// [`capped`], with `vars` set for git.
pub(crate) async fn capped_with(
    cwd: &Path,
    args: &[&str],
    limit: usize,
    ok: &[i32],
    vars: Vars<'_>,
) -> Result<(Vec<u8>, bool), GitError> {
    let mut child = command(cwd, args, vars).spawn().map_err(GitError::Spawn)?;
    let (stdout, stderr) = (child.stdout.take(), child.stderr.take());
    let work = async {
        let printed = read_up_to(stdout, limit);
        let complained = read_up_to(stderr, MAX_STDERR);
        tokio::pin!(printed, complained);
        let mut errors = None;
        let (out, cut) = loop {
            tokio::select! {
                read = &mut printed => break read?,
                read = &mut complained, if errors.is_none() => errors = Some(read?.0),
            }
        };
        if cut {
            // The rest is not wanted: git stops rather than wait to write it.
            let _ = child.start_kill();
        }
        let errors = match errors {
            Some(errors) => errors,
            None => complained.await?.0,
        };
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((out, cut, status, errors))
    };
    let (out, cut, status, errors) = tokio::time::timeout(TIME_LIMIT, work)
        .await
        .map_err(|_| GitError::TimedOut(args.join(" ")))?
        .map_err(GitError::Spawn)?;
    if cut || status.code().is_some_and(|code| ok.contains(&code)) {
        return Ok((out, cut));
    }
    Err(GitError::Failed {
        args: args.join(" "),
        stderr: reason(String::from_utf8_lossy(&errors).trim()),
    })
}

/// Reads `pipe` to its end, keeping at most `limit` bytes, and says
/// whether there was more; it stops reading there.
async fn read_up_to(
    pipe: Option<impl AsyncRead + Unpin>,
    limit: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut read = Vec::new();
    if let Some(pipe) = pipe {
        let wanted = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
        pipe.take(wanted).read_to_end(&mut read).await?;
    }
    let cut = read.len() > limit;
    read.truncate(limit);
    Ok((read, cut))
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
    answer_with(cwd, args, &[]).await
}

/// [`answer`], with `vars` set for git.
pub(crate) async fn answer_with(
    cwd: &Path,
    args: &[&str],
    vars: Vars<'_>,
) -> Result<Option<String>, GitError> {
    let outcome = run_with(cwd, args, vars).await?;
    Ok(outcome.success.then_some(outcome.stdout))
}

/// The address of the repository's remote: `origin`'s, or else the first
/// remote's; `None` with no remote, or outside a repository.
pub(crate) async fn remote_url(repo: &Path) -> Option<String> {
    if let Ok(Some(url)) = answer(repo, &["remote", "get-url", "origin"]).await {
        return Some(url).filter(|url| !url.is_empty());
    }
    let remotes = answer(repo, &["remote"]).await.ok()??;
    let first = remotes.lines().next()?.trim().to_owned();
    answer(repo, &["remote", "get-url", &first])
        .await
        .ok()?
        .filter(|url| !url.is_empty())
}

/// What is changed or untracked in the work tree at `path`, one line per
/// path as git's porcelain status lists it (the whole output trimmed);
/// nothing when all is committed.
pub(crate) async fn uncommitted(path: &Path) -> Result<Vec<String>, GitError> {
    Ok(checked(path, &["status", "--porcelain"])
        .await?
        .lines()
        .map(str::to_owned)
        .filter(|line| !line.trim().is_empty())
        .collect())
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
    async fn uncommitted_work_is_listed_and_committed_work_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        assert!(uncommitted(&repo).await.unwrap().is_empty());
        std::fs::write(repo.join("README.md"), "changed\n").unwrap();
        std::fs::write(repo.join("new.txt"), "new\n").unwrap();
        let listed = uncommitted(&repo).await.unwrap();
        assert_eq!(listed.len(), 2, "{listed:?}");
        assert!(listed[0].ends_with("README.md") && listed[1] == "?? new.txt");
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
