//! Where workspaces' work meets, against real repositories (`PRODUCT.md`
//! §6, `PRD-14`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use muster_core::clashes::Clash;
use muster_core::{Timestamp, WorkspaceId};

use super::*;
use crate::git::tests::{git, repository};
use crate::timeline::tests::repository_state;

fn write(dir: &Path, file: &str, text: &str) {
    std::fs::write(dir.join(file), text).unwrap();
}

const NOTES: &str = "one\ntwo\nthree\nfour\nfive\n";

/// A repository with notes and a guide, and a store for its clashes.
struct Setup {
    dir: tempfile::TempDir,
    repo: PathBuf,
    store: PathBuf,
}

fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    write(&repo, "notes.txt", NOTES);
    std::fs::create_dir(repo.join("docs")).unwrap();
    write(&repo, "docs/guide.md", "how\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "notes"]);
    let store = dir.path().join("data/clashes/p1");
    Setup { dir, repo, store }
}

impl Setup {
    /// A worktree on a new branch `name`, from what the checkout has.
    fn worktree(&self, name: &str) -> Side {
        let path = self.dir.path().join(name);
        git(
            &self.repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                name,
                &path.to_string_lossy(),
            ],
        );
        Side {
            workspace: WorkspaceId::new(),
            path: path.canonicalize().unwrap(),
            base: "main".to_owned(),
        }
    }
}

fn of(found: &[(WorkspaceId, Vec<Clash>)], side: &Side) -> Vec<Clash> {
    found
        .iter()
        .find(|(workspace, _)| *workspace == side.workspace)
        .map(|(_, clashes)| clashes.clone())
        .unwrap()
}

/// Each file of `clash` with whether it would conflict.
fn files(clash: &Clash) -> Vec<(&str, bool)> {
    clash
        .shown
        .iter()
        .map(|file| (file.path.as_str(), file.conflict))
        .collect()
}

fn status(dir: &Path) -> Vec<u8> {
    std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .unwrap()
        .stdout
}

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_millis(seconds * 1000)
}

#[tokio::test]
async fn two_agents_changing_one_line_would_conflict_and_nothing_of_theirs_changes() {
    let setup = setup();
    let one = setup.worktree("one");
    let two = setup.worktree("two");
    let apart = setup.worktree("apart");
    write(
        &one.path,
        "notes.txt",
        "one\nTWO by one\nthree\nfour\nfive\n",
    );
    write(
        &two.path,
        "notes.txt",
        "one\nTWO by two\nthree\nfour\nfive\n",
    );
    // Files not committed yet, nor added, count too.
    write(&one.path, "todo.txt", "one's\n");
    write(&two.path, "todo.txt", "two's\n");
    write(&apart.path, "other.txt", "elsewhere\n");
    let before = repository_state(&setup.repo);
    let statuses: Vec<Vec<u8>> = [&one, &two, &apart]
        .iter()
        .map(|side| status(&side.path))
        .collect();

    let sides = [one.clone(), two.clone(), apart.clone()];
    let keep: Vec<WorkspaceId> = sides.iter().map(|side| side.workspace).collect();
    let found = check(&setup.store, &sides, &keep, at(10)).await.unwrap();

    let mine = of(&found, &one);
    assert_eq!(mine.len(), 1, "{mine:?}");
    assert_eq!(mine[0].with, two.workspace);
    assert_eq!((mine[0].files, mine[0].conflicts), (2, 2));
    assert_eq!(files(&mine[0]), [("notes.txt", true), ("todo.txt", true)]);
    let theirs = of(&found, &two);
    assert_eq!(theirs.len(), 1);
    assert_eq!(theirs[0].with, one.workspace);
    assert!(of(&found, &apart).is_empty(), "it changes nothing alike");

    // Only read: the person's repository and each worktree are as they
    // were.
    assert_eq!(repository_state(&setup.repo), before);
    let after: Vec<Vec<u8>> = [&one, &two, &apart]
        .iter()
        .map(|side| status(&side.path))
        .collect();
    assert_eq!(after, statuses);
    let parent = std::fs::metadata(setup.store.parent().unwrap()).unwrap();
    assert_eq!(parent.permissions().mode() & 0o777, 0o700);
}

#[tokio::test]
async fn changes_apart_in_one_file_overlap_without_conflict_committed_or_not() {
    let setup = setup();
    let one = setup.worktree("one");
    let two = setup.worktree("two");
    write(&one.path, "notes.txt", "ONE\ntwo\nthree\nfour\nfive\n");
    git(&one.path, &["commit", "--quiet", "-am", "one's first line"]);
    write(&two.path, "notes.txt", "one\ntwo\nthree\nfour\nFIVE\n");

    let sides = [one.clone(), two.clone()];
    let keep = [one.workspace, two.workspace];
    let found = check(&setup.store, &sides, &keep, at(10)).await.unwrap();

    let mine = of(&found, &one);
    assert_eq!(files(&mine[0]), [("notes.txt", false)]);
    assert!(!mine[0].would_conflict());
}

#[tokio::test]
async fn a_file_where_another_keeps_a_folder_would_conflict() {
    let setup = setup();
    let one = setup.worktree("one");
    let two = setup.worktree("two");
    std::fs::remove_dir_all(one.path.join("docs")).unwrap();
    write(&one.path, "docs", "a file now\n");
    write(&two.path, "docs/new.md", "more\n");

    let sides = [one.clone(), two.clone()];
    let keep = [one.workspace, two.workspace];
    let found = check(&setup.store, &sides, &keep, at(10)).await.unwrap();

    assert_eq!(files(&of(&found, &one)[0]), [("docs", true)]);
    assert_eq!(files(&of(&found, &two)[0]), [("docs", true)]);
}

#[tokio::test]
async fn a_conflict_with_what_the_base_has_since_is_not_counted_against_the_other() {
    let setup = setup();
    let one = setup.worktree("one");
    write(
        &one.path,
        "notes.txt",
        "one\nTWO by one\nthree\nfour\nfive\n",
    );
    write(&one.path, "README.md", "test\nby one\n");
    // The base moves on, changing the line one changes, before two
    // leaves from it.
    write(
        &setup.repo,
        "notes.txt",
        "one\nTWO on main\nthree\nfour\nfive\n",
    );
    git(
        &setup.repo,
        &["commit", "--quiet", "-am", "main's second line"],
    );
    let two = setup.worktree("two");
    write(&two.path, "README.md", "by two\ntest\n");

    let sides = [one.clone(), two.clone()];
    let keep = [one.workspace, two.workspace];
    let found = check(&setup.store, &sides, &keep, at(10)).await.unwrap();

    let mine = of(&found, &one);
    assert_eq!(files(&mine[0]), [("README.md", false)], "{mine:?}");
}

#[tokio::test]
async fn snapshots_of_workspaces_gone_are_let_go_and_the_store_with_its_project() {
    let setup = setup();
    let one = setup.worktree("one");
    let two = setup.worktree("two");
    write(&one.path, "notes.txt", "ONE\n");
    write(&two.path, "notes.txt", "TWO\n");
    let sides = [one.clone(), two.clone()];
    check(
        &setup.store,
        &sides,
        &[one.workspace, two.workspace],
        at(10),
    )
    .await
    .unwrap();
    let kept = |side: &Side| {
        setup
            .store
            .join("indexes")
            .join(side.workspace.to_string())
            .exists()
    };
    assert!(kept(&one) && kept(&two));

    // Two went: only one is compared, and two's snapshot goes.
    let found = check(
        &setup.store,
        std::slice::from_ref(&one),
        &[one.workspace],
        at(20),
    )
    .await
    .unwrap();
    assert_eq!(found, [(one.workspace, Vec::new())]);
    assert!(kept(&one) && !kept(&two));
    let refs = std::process::Command::new("git")
        .arg(format!("--git-dir={}", setup.store.display()))
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .unwrap()
        .stdout;
    assert_eq!(
        String::from_utf8_lossy(&refs).trim(),
        format!("refs/snapshots/{}", one.workspace)
    );

    remove(&setup.store).await;
    assert!(!setup.store.exists());
}
