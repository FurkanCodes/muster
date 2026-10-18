//! Moving a branch onto another commit, against real repositories
//! (`PRODUCT.md` §6, `PRD-14`).

use std::path::{Path, PathBuf};

use muster_core::Timestamp;
use muster_core::stacks::Stacked;

use super::*;
use crate::git::tests::{git, repository};

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_millis(seconds * 1000)
}

fn commit(dir: &Path, file: &str, text: &str) {
    std::fs::write(dir.join(file), text).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", file]);
}

fn rev(dir: &Path, name: &str) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", name])
        .current_dir(dir)
        .output()
        .unwrap()
        .stdout;
    String::from_utf8_lossy(&out).trim().to_owned()
}

/// A repository on `main`, a worktree on `parent` with one commit, and a
/// worktree on `child` made on top of it with one of its own; the
/// child's timeline.
struct Setup {
    dir: tempfile::TempDir,
    repo: PathBuf,
    parent: PathBuf,
    child: PathBuf,
    store: Store,
    /// The parent's commit the child was made on.
    at: String,
}

async fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    commit(&repo, "notes.txt", "one\ntwo\nthree\n");
    let worktree = |name: &str, start: &str| {
        let path = dir.path().join(name);
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                name,
                &path.to_string_lossy(),
                start,
            ],
        );
        path.canonicalize().unwrap()
    };
    let parent = worktree("parent", "main");
    commit(&parent, "parent.txt", "the parent's work\n");
    let at = rev(&parent, "HEAD");
    let child = worktree("child", &at);
    commit(&child, "child.txt", "the child's work\n");
    let store = Store::open(&dir.path().join("data/timelines/child"), &child)
        .await
        .unwrap();
    Setup {
        dir,
        repo,
        parent,
        child,
        store,
        at,
    }
}

impl Setup {
    fn plan(&self, parent_merged: bool) -> MovePlan {
        MovePlan {
            base: "main".to_owned(),
            stacked: Some(Stacked {
                branch: "parent".to_owned(),
                at: self.at.clone(),
            }),
            parent_merged,
        }
    }
}

#[tokio::test]
async fn a_stacked_branch_moves_onto_its_parent_s_newest_and_back() {
    let setup = setup().await;
    let before = rev(&setup.child, "HEAD");
    commit(&setup.parent, "more.txt", "more of the parent's work\n");
    let newest = rev(&setup.parent, "HEAD");

    let done = setup
        .store
        .move_onto(&setup.plan(false), at(10))
        .await
        .unwrap();
    assert_eq!(done.onto, "parent");
    assert_eq!(done.commits, 1);
    assert_eq!(
        done.stacked,
        Some(Stacked {
            branch: "parent".to_owned(),
            at: newest.clone(),
        })
    );
    assert_eq!(rev(&setup.child, "HEAD~1"), newest);
    assert!(setup.child.join("more.txt").exists());
    assert!(setup.child.join("child.txt").exists());

    // Kept first: undone, branch and all.
    let (_, moved) = setup.store.undo(done.entry, true, at(20)).await.unwrap();
    assert!(moved);
    assert_eq!(rev(&setup.child, "HEAD"), before);
    assert!(!setup.child.join("more.txt").exists());
}

#[tokio::test]
async fn once_the_parent_merges_it_moves_onto_the_base_without_the_parent_s_commits() {
    let setup = setup().await;
    // The parent's work squashed into main, as a host's merge button does.
    std::fs::write(setup.repo.join("parent.txt"), "the parent's work\n").unwrap();
    git(&setup.repo, &["add", "-A"]);
    git(
        &setup.repo,
        &["commit", "--quiet", "-m", "The parent's work (#7)"],
    );
    let main = rev(&setup.repo, "main");

    let done = setup
        .store
        .move_onto(&setup.plan(true), at(10))
        .await
        .unwrap();
    assert_eq!((done.onto.as_str(), done.commits), ("main", 1));
    assert_eq!(done.stacked, None);
    assert_eq!(rev(&setup.child, "HEAD~1"), main);
    let parents = std::process::Command::new("git")
        .args(["merge-base", "--is-ancestor", &setup.at, "HEAD"])
        .current_dir(&setup.child)
        .status()
        .unwrap();
    assert!(!parents.success(), "the parent's own commit is left behind");
}

#[tokio::test]
async fn a_move_that_would_conflict_changes_nothing() {
    let setup = setup().await;
    commit(&setup.child, "notes.txt", "one\nthe child's two\nthree\n");
    commit(&setup.repo, "notes.txt", "one\nmain's two\nthree\n");
    let before = rev(&setup.child, "HEAD");
    let plan = MovePlan {
        base: "main".to_owned(),
        stacked: None,
        parent_merged: false,
    };

    let refused = setup.store.move_onto(&plan, at(10)).await.unwrap_err();
    let said = refused.to_string();
    assert!(said.contains("would conflict in notes.txt"), "{said}");
    assert!(said.contains("nothing changed"), "{said}");
    assert_eq!(rev(&setup.child, "HEAD"), before);
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&setup.child)
        .output()
        .unwrap()
        .stdout;
    assert!(status.is_empty(), "{}", String::from_utf8_lossy(&status));
    let rebasing = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", "REBASE_HEAD"])
        .current_dir(&setup.child)
        .status()
        .unwrap();
    assert!(!rebasing.success(), "no rebase is left half done");
    drop(setup.dir);
}

#[tokio::test]
async fn a_move_git_refuses_with_nothing_in_conflict_says_why_and_changes_nothing() {
    let setup = setup().await;
    commit(&setup.parent, "more.txt", "more of the parent's work\n");
    // A file not in git where the parent's newest has one.
    std::fs::write(setup.child.join("more.txt"), "the person's own\n").unwrap();
    let before = rev(&setup.child, "HEAD");

    let refused = setup.store.move_onto(&setup.plan(false), at(10)).await;
    let Err(TimelineError::NotMoved(why)) = &refused else {
        panic!("{refused:?}");
    };
    assert!(why.contains("git could not move it onto parent"), "{why}");
    assert!(
        why.contains("untracked working tree files would be overwritten"),
        "git's reason: {why}"
    );
    assert!(!why.contains("conflict"), "nothing conflicts: {why}");
    assert_eq!(rev(&setup.child, "HEAD"), before);
    assert_eq!(
        std::fs::read_to_string(setup.child.join("more.txt")).unwrap(),
        "the person's own\n"
    );
}

#[tokio::test]
async fn changes_not_committed_or_a_branch_on_the_newest_already_are_refused() {
    let setup = setup().await;
    std::fs::write(setup.child.join("child.txt"), "not committed\n").unwrap();
    let refused = setup.store.move_onto(&setup.plan(false), at(10)).await;
    assert!(
        matches!(&refused, Err(TimelineError::NotMoved(why)) if why.contains("not committed")),
        "{refused:?}"
    );
    git(&setup.child, &["checkout", "--", "child.txt"]);
    let refused = setup.store.move_onto(&setup.plan(false), at(10)).await;
    assert!(
        matches!(&refused, Err(TimelineError::NotMoved(why)) if why.contains("newest of parent already")),
        "{refused:?}"
    );
}
