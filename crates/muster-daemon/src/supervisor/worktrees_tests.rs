//! Naming worktrees, and where a new one's branch starts, against real
//! repositories.

use std::path::Path;

use super::*;
use crate::git::tests::{git, repository};

#[test]
fn slugs_are_safe_file_names() {
    assert_eq!(slug("muster/fix-login"), "muster-fix-login");
    assert_eq!(slug("feature/ünïcode name"), "feature--n-code-name");
    assert_eq!(slug("../.."), "worktree");
}

#[test]
fn a_clashing_branch_name_is_explained() {
    let error = GitError::Failed {
        args: "worktree add -b test /w/test".to_owned(),
        stderr: "fatal: cannot lock ref 'refs/heads/test': 'refs/heads/test/beta38-candidate' \
                 exists; cannot create 'refs/heads/test'"
            .to_owned(),
    };
    let body = branch_clash(&error, "test").unwrap();
    assert_eq!(body.code, ErrorCode::Conflict);
    assert!(
        body.message
            .contains("the branch test/beta38-candidate exists"),
        "{}",
        body.message
    );
    assert!(body.message.contains("such as test-1"), "{}", body.message);

    let other = GitError::Failed {
        args: String::new(),
        stderr: "fatal: something else".to_owned(),
    };
    assert!(branch_clash(&other, "x").is_none());
}

#[test]
fn default_branches_are_namespaced_and_short() {
    let branch = default_branch(true);
    assert!(branch.starts_with("muster/"));
    assert_eq!(branch.len(), "muster/".len() + 6);
    assert_ne!(branch, default_branch(true));
    let without = default_branch(false);
    assert!(without.starts_with("worktree-"), "{without}");
    assert_eq!(without.len(), "worktree-".len() + 6);
}

fn commit(repo: &Path, file: &str) -> String {
    std::fs::write(repo.join(file), file).unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "--quiet", "-m", file]);
    rev(repo, "HEAD")
}

fn rev(repo: &Path, name: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", name])
        .current_dir(repo)
        .output()
        .unwrap()
        .stdout;
    String::from_utf8_lossy(&out).trim().to_owned()
}

/// The remote's `main` as last fetched, at `commit`, and its default
/// branch, as a clone has them.
fn remote_main_at(repo: &Path, commit: &str) {
    git(repo, &["update-ref", "refs/remotes/origin/main", commit]);
    git(
        repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
}

#[tokio::test]
async fn a_new_branch_starts_from_the_base_not_from_the_branch_checked_out() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    let main = rev(&repo, "main");
    // The person works on a branch of their own in the checkout.
    git(&repo, &["switch", "--quiet", "-c", "feature/mine"]);
    commit(&repo, "mine.txt");
    assert_eq!(start_point(&repo, None).await.as_deref(), Some(&*main));
}

#[tokio::test]
async fn the_base_here_wins_while_it_is_ahead_of_the_remote_s() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    remote_main_at(&repo, &rev(&repo, "main"));
    // Merged here and not pushed yet.
    let merged = commit(&repo, "merged.txt");
    assert_eq!(start_point(&repo, None).await.as_deref(), Some(&*merged));
}

#[tokio::test]
async fn the_remote_s_base_wins_while_it_is_ahead_of_the_one_here() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    let old = rev(&repo, "main");
    let newer = commit(&repo, "theirs.txt");
    remote_main_at(&repo, &newer);
    // Fetched, not pulled.
    git(&repo, &["reset", "--quiet", "--hard", &old]);
    assert_eq!(start_point(&repo, None).await.as_deref(), Some(&*newer));
}

#[tokio::test]
async fn where_the_two_have_parted_the_base_here_wins() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    let first = rev(&repo, "main");
    let theirs = commit(&repo, "theirs.txt");
    remote_main_at(&repo, &theirs);
    git(&repo, &["reset", "--quiet", "--hard", &first]);
    let mine = commit(&repo, "mine.txt");
    assert_eq!(start_point(&repo, None).await.as_deref(), Some(&*mine));
}

#[tokio::test]
async fn the_base_the_settings_name_is_the_one_started_from() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    git(&repo, &["switch", "--quiet", "-c", "develop"]);
    let develop = commit(&repo, "develop.txt");
    git(&repo, &["switch", "--quiet", "main"]);
    assert_eq!(
        start_point(&repo, Some("develop")).await.as_deref(),
        Some(&*develop)
    );
}

#[tokio::test]
async fn with_no_base_found_git_s_current_commit_is_used() {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    git(&repo, &["branch", "--quiet", "-m", "main", "trunk"]);
    assert_eq!(start_point(&repo, None).await, None);
}
