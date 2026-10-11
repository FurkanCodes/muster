//! What each workspace has checked out, kept current as the person or an
//! agent switches branches there (`git switch`, `git checkout`).
//!
//! Every couple of seconds a task asks the Supervisor for the workspaces,
//! reads each one's HEAD on a blocking thread, and hands back those that
//! changed. HEAD is read straight from git's files, which have kept this
//! format for many years, rather than by running git: that would be a
//! process per workspace per look.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use muster_core::WorkspaceId;

use crate::supervisor::SupervisorHandle;

/// What is checked out in the checkout or worktree at `path`: a branch
/// name, another ref, or `detached at <commit>`.
pub(crate) fn checked_out(path: &Path) -> io::Result<String> {
    let head = std::fs::read_to_string(head_file(path)?)?;
    describe(&head).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected HEAD {:?}", head.trim()),
        )
    })
}

/// The HEAD file of the checkout or worktree at `path`. A main checkout
/// has a `.git` directory; a worktree has a `.git` file naming its own git
/// directory, possibly relative to the worktree.
fn head_file(path: &Path) -> io::Result<PathBuf> {
    let dot_git = path.join(".git");
    if dot_git.is_dir() {
        return Ok(dot_git.join("HEAD"));
    }
    let pointer = std::fs::read_to_string(&dot_git)?;
    let git_dir = pointer
        .trim()
        .strip_prefix("gitdir:")
        .map(str::trim)
        .filter(|dir| !dir.is_empty())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("{} names no git directory", dot_git.display()),
            )
        })?;
    Ok(path.join(git_dir).join("HEAD"))
}

/// A HEAD file's contents as the person reads them.
fn describe(head: &str) -> Option<String> {
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref:") {
        let reference = reference.trim();
        let name = reference.strip_prefix("refs/heads/").unwrap_or(reference);
        // Git allows no control characters in a ref; a HEAD with one is not
        // git's, and its text is not shown.
        let valid = !name.is_empty() && !name.contains(char::is_control);
        return valid.then(|| name.to_owned());
    }
    let is_commit = matches!(head.len(), 40 | 64) && head.chars().all(|c| c.is_ascii_hexdigit());
    is_commit.then(|| format!("detached at {}", &head[..7]))
}

/// Looks every `every` until the Supervisor stops.
pub(crate) async fn run(supervisor: SupervisorHandle, every: Duration) {
    let mut ticks = tokio::time::interval(every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(workspaces) = supervisor.workspaces().await else {
            return;
        };
        let read = tokio::task::spawn_blocking(move || changed(workspaces)).await;
        let Ok(changes) = read else {
            continue;
        };
        if !changes.is_empty() && !supervisor.record_branches(changes).await {
            return;
        }
    }
}

/// The workspaces whose checked-out branch is no longer `known`. One that
/// cannot be read, such as a worktree deleted by hand, keeps what it had.
fn changed(workspaces: Vec<(WorkspaceId, PathBuf, String)>) -> Vec<(WorkspaceId, String)> {
    workspaces
        .into_iter()
        .filter_map(|(id, path, known)| {
            let now = checked_out(&path).ok()?;
            (now != known).then_some((id, now))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::{git, repository};

    #[test]
    fn heads_read_as_people_name_them() {
        assert_eq!(
            describe("ref: refs/heads/feature/x\n").as_deref(),
            Some("feature/x")
        );
        assert_eq!(
            describe("ref: refs/remotes/origin/main").as_deref(),
            Some("refs/remotes/origin/main")
        );
        assert_eq!(
            describe("0123456789abcdef0123456789abcdef01234567\n").as_deref(),
            Some("detached at 0123456")
        );
        for nonsense in ["", "ref: ", "hello", "0123456", "ref: refs/heads/a\x1b[2J"] {
            assert_eq!(describe(nonsense), None, "{nonsense:?}");
        }
    }

    #[test]
    fn a_checkout_and_its_worktree_report_their_own_branches() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        assert_eq!(checked_out(&repo).unwrap(), "main");

        let tree = dir.path().join("tree");
        git(
            &repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "side",
                tree.to_str().unwrap(),
            ],
        );
        assert_eq!(checked_out(&tree).unwrap(), "side");

        git(&repo, &["switch", "-q", "-c", "renamed"]);
        assert_eq!(checked_out(&repo).unwrap(), "renamed");
        git(&tree, &["switch", "-q", "--detach"]);
        assert!(checked_out(&tree).unwrap().starts_with("detached at "));
    }

    #[test]
    fn only_changes_are_reported_and_unreadable_workspaces_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let repo = repository(dir.path(), "app");
        let (same, moved, gone) = (WorkspaceId::new(), WorkspaceId::new(), WorkspaceId::new());
        let changes = changed(vec![
            (same, repo.clone(), "main".to_owned()),
            (moved, repo.clone(), "old".to_owned()),
            (gone, dir.path().join("deleted"), "x".to_owned()),
        ]);
        assert_eq!(changes, [(moved, "main".to_owned())]);
    }
}
