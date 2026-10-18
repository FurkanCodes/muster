//! The timeline store against real repositories (`PRODUCT.md` §6,
//! `PRD-14`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use muster_core::timeline::EntryKind;
use muster_core::{SessionId, Timestamp};

use super::*;
use crate::git::tests::{git, repository};

fn at(seconds: i64) -> Timestamp {
    Timestamp::from_millis(seconds * 1000)
}

fn write(dir: &Path, file: &str, text: &str) {
    std::fs::write(dir.join(file), text).unwrap();
}

fn read(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok()
}

/// A repository with a worktree on `feature`, and a timeline for it.
struct Setup {
    _dir: tempfile::TempDir,
    repo: PathBuf,
    worktree: PathBuf,
    store: Store,
}

async fn setup() -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let repo = repository(dir.path(), "app");
    write(&repo, "notes.txt", "one\ntwo\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "notes"]);
    let worktree = dir.path().join("feature");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "feature",
            &worktree.to_string_lossy(),
        ],
    );
    let worktree = worktree.canonicalize().unwrap();
    let store = Store::open(&dir.path().join("data/timelines/w1"), &worktree)
        .await
        .unwrap();
    Setup {
        _dir: dir,
        repo,
        worktree,
        store,
    }
}

/// What the person's repository holds, but for object files' times.
pub(crate) fn repository_state(repo: &Path) -> Vec<String> {
    let run = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    let mut objects: Vec<String> = walk(&repo.join(".git/objects"));
    objects.sort();
    let mut files: Vec<String> = walk(&repo.join(".git"))
        .into_iter()
        .filter(|path| !path.contains("/objects/"))
        .collect();
    files.sort();
    vec![
        run(&["for-each-ref"]),
        run(&["worktree", "list", "--porcelain"]),
        run(&["config", "--local", "--list"]),
        objects.join("\n"),
        files
            .iter()
            .map(|file| {
                let bytes = std::fs::read(file).unwrap_or_default();
                format!("{file} {}", bytes.len())
            })
            .collect::<Vec<_>>()
            .join("\n"),
    ]
}

fn walk(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else {
            found.push(path.to_string_lossy().into_owned());
        }
    }
    found
}

#[tokio::test]
async fn a_turn_s_changes_are_an_entry_and_the_repository_is_left_alone() {
    let Setup {
        _dir,
        repo,
        worktree,
        store,
    } = setup().await;
    let session = SessionId::new();
    store.began(session, &[session], at(10)).await.unwrap();
    write(&worktree, "notes.txt", "one\ntwo\nthree\n");
    write(&worktree, "new.txt", "fresh\n");
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&worktree)
        .output()
        .unwrap()
        .stdout;
    let before = repository_state(&repo);

    let recorded = store.ended(session, "Claude Code", at(70)).await.unwrap();

    assert_eq!(recorded, Some(1));
    let timeline = store.timeline().await.unwrap();
    let [entry] = timeline.entries.as_slice() else {
        panic!("{timeline:?}");
    };
    assert_eq!(
        entry.kind,
        EntryKind::Turn {
            session,
            agent: "Claude Code".to_owned()
        }
    );
    assert_eq!((entry.started, entry.ended), (at(10), at(70)));
    let summary = entry.summary.unwrap();
    assert_eq!((summary.files, summary.added, summary.removed), (2, 2, 0));
    assert!(entry.head_before.is_some());
    assert_eq!(entry.head_before, entry.head_after);
    let diff = store.diff(1).await.unwrap();
    assert!(diff.text.contains("+three"), "{}", diff.text);
    assert!(diff.text.contains("+fresh"), "{}", diff.text);

    // Nothing of the person's changed: refs, worktrees, config, objects,
    // index; and git says what it said.
    assert_eq!(repository_state(&repo), before);
    let after = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&worktree)
        .output()
        .unwrap()
        .stdout;
    assert_eq!(after, status);
    assert_eq!(store.newest().await.unwrap(), Some(1));
}

#[tokio::test]
async fn a_turn_that_changed_nothing_is_no_entry_and_edits_between_turns_are_one() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let (one, two) = (SessionId::new(), SessionId::new());
    store.began(one, &[one], at(1)).await.unwrap();
    assert_eq!(store.ended(one, "Codex", at(2)).await.unwrap(), None);
    // A turn ending whose start was not seen is let go.
    assert_eq!(store.ended(two, "Codex", at(3)).await.unwrap(), None);

    store.began(one, &[one], at(4)).await.unwrap();
    write(&worktree, "notes.txt", "one\n");
    assert_eq!(store.ended(one, "Codex", at(5)).await.unwrap(), Some(1));
    // The person's own edit, then another turn.
    write(&worktree, "mine.txt", "by hand\n");
    assert_eq!(store.began(two, &[two], at(6)).await.unwrap(), Some(2));
    write(&worktree, "theirs.txt", "by the agent\n");
    assert_eq!(store.ended(two, "Codex", at(7)).await.unwrap(), Some(3));

    let timeline = store.timeline().await.unwrap();
    let kinds: Vec<_> = timeline
        .entries
        .iter()
        .map(|entry| (entry.number, entry.kind.clone()))
        .collect();
    assert!(matches!(kinds[0], (3, EntryKind::Turn { session, .. }) if session == two));
    assert_eq!(kinds[1], (2, EntryKind::Between));
    assert!(matches!(kinds[2], (1, EntryKind::Turn { session, .. }) if session == one));
    let between = &timeline.entries[1];
    assert_eq!((between.started, between.ended), (at(5), at(6)));
    assert!(store.diff(2).await.unwrap().text.contains("+by hand"));
    let turn = store.diff(3).await.unwrap().text;
    assert!(
        turn.contains("+by the agent") && !turn.contains("by hand"),
        "{turn}"
    );
}

#[tokio::test]
async fn while_another_agent_works_there_no_edit_is_taken_for_the_person_s() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let (one, two) = (SessionId::new(), SessionId::new());
    store.began(one, &[one], at(1)).await.unwrap();
    write(&worktree, "one.txt", "first agent\n");
    // The second begins while the first works: what it finds is the
    // first's work under way, not an edit between turns.
    assert_eq!(store.began(two, &[one, two], at(2)).await.unwrap(), None);
    write(&worktree, "two.txt", "second agent\n");
    assert_eq!(
        store.ended(one, "Claude Code", at(3)).await.unwrap(),
        Some(1)
    );
    assert_eq!(store.ended(two, "Codex", at(4)).await.unwrap(), Some(2));
    let second = store.diff(2).await.unwrap().text;
    assert!(second.contains("+second agent"), "{second}");
    assert!(!second.contains("first agent"), "{second}");
}

#[tokio::test]
async fn undo_puts_the_files_back_and_can_itself_be_undone() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let session = SessionId::new();
    write(&worktree, "draft.txt", "the person's, not committed\n");
    store.began(session, &[session], at(1)).await.unwrap();
    write(&worktree, "notes.txt", "rewritten\n");
    write(&worktree, "new.txt", "made by the turn\n");
    std::fs::remove_file(worktree.join("README.md")).unwrap();
    store.ended(session, "Claude Code", at(2)).await.unwrap();

    let plan = store.plan(1).await.unwrap();
    assert_eq!((plan.number, plan.later, plan.branch), (1, 0, None));
    let (undo, moved) = store.undo(1, false, at(3)).await.unwrap();
    assert_eq!((undo, moved), (2, false));
    assert_eq!(read(&worktree, "notes.txt").as_deref(), Some("one\ntwo\n"));
    assert_eq!(read(&worktree, "README.md").as_deref(), Some("test\n"));
    assert_eq!(read(&worktree, "new.txt"), None);
    assert_eq!(
        read(&worktree, "draft.txt").as_deref(),
        Some("the person's, not committed\n")
    );
    let timeline = store.timeline().await.unwrap();
    assert_eq!(timeline.entries[0].kind, EntryKind::Undo { of: 1 });

    // Undoing the undo brings the turn's work back.
    store.undo(2, false, at(4)).await.unwrap();
    assert_eq!(read(&worktree, "notes.txt").as_deref(), Some("rewritten\n"));
    assert_eq!(
        read(&worktree, "new.txt").as_deref(),
        Some("made by the turn\n")
    );
    assert_eq!(read(&worktree, "README.md"), None);
    // Nothing left to put back is said, not done.
    assert!(matches!(
        store.undo(2, false, at(5)).await,
        Err(TimelineError::AlreadyThere(2))
    ));
    assert!(matches!(
        store.undo(9, false, at(5)).await,
        Err(TimelineError::NoEntry(9))
    ));
}

#[tokio::test]
async fn undo_never_writes_over_an_ignored_file_and_then_changes_nothing() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let session = SessionId::new();
    store.began(session, &[session], at(1)).await.unwrap();
    std::fs::remove_file(worktree.join("notes.txt")).unwrap();
    write(&worktree, ".gitignore", "notes.txt\n");
    write(&worktree, "README.md", "changed\n");
    store.ended(session, "Claude Code", at(2)).await.unwrap();
    // Something writes the file again, now ignored.
    write(&worktree, "notes.txt", "a build's output\n");

    let error = store.undo(1, false, at(3)).await.unwrap_err();
    assert!(matches!(error, TimelineError::Refused(_)), "{error}");
    assert!(error.to_string().contains("notes.txt"), "{error}");
    assert_eq!(
        read(&worktree, "notes.txt").as_deref(),
        Some("a build's output\n")
    );
    assert_eq!(read(&worktree, "README.md").as_deref(), Some("changed\n"));
    assert_eq!(
        read(&worktree, ".gitignore").as_deref(),
        Some("notes.txt\n")
    );
    assert_eq!(store.newest().await.unwrap(), Some(1));
}

#[tokio::test]
async fn undo_never_writes_over_an_ignored_file_where_a_folder_must_go() {
    let Setup {
        _dir,
        repo,
        worktree,
        ..
    } = setup().await;
    std::fs::create_dir(repo.join("docs")).unwrap();
    write(&repo, "docs/guide.md", "how\n");
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "--quiet", "-m", "docs"]);
    git(&worktree, &["merge", "--quiet", "--ff-only", "main"]);
    let store = Store::open(&worktree.join("../data/timelines/w2"), &worktree)
        .await
        .unwrap();
    let session = SessionId::new();
    store.began(session, &[session], at(1)).await.unwrap();
    std::fs::remove_dir_all(worktree.join("docs")).unwrap();
    write(&worktree, ".gitignore", "docs\n");
    store.ended(session, "Claude Code", at(2)).await.unwrap();
    write(&worktree, "docs", "an ignored file\n");

    let error = store.undo(1, false, at(3)).await.unwrap_err();
    assert!(error.to_string().contains("docs"), "{error}");
    assert_eq!(
        read(&worktree, "docs").as_deref(),
        Some("an ignored file\n")
    );
}

#[tokio::test]
async fn the_branch_moves_back_only_over_commits_not_pushed() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let session = SessionId::new();
    let head = |dir: &Path| {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dir)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    let start = head(&worktree);
    store.began(session, &[session], at(1)).await.unwrap();
    write(&worktree, "notes.txt", "committed by the agent\n");
    git(&worktree, &["commit", "--quiet", "-am", "agent's work"]);
    store.ended(session, "Claude Code", at(2)).await.unwrap();

    let planned = store.plan(1).await.unwrap().branch.unwrap();
    assert_eq!(
        (
            planned.branch.as_str(),
            planned.commits,
            planned.forward,
            &planned.refused
        ),
        ("feature", 1, false, &None)
    );

    // Pushed, it stays.
    let pushed = head(&worktree);
    git(
        &worktree,
        &["update-ref", "refs/remotes/origin/feature", &pushed],
    );
    let refused = store.plan(1).await.unwrap().branch.unwrap();
    assert!(refused.refused.unwrap().contains("pushed"));
    assert!(matches!(
        store.undo(1, true, at(3)).await,
        Err(TimelineError::Branch(_))
    ));
    assert_eq!(head(&worktree), pushed, "nothing moved");
    git(
        &worktree,
        &["update-ref", "-d", "refs/remotes/origin/feature"],
    );

    let (undo, moved) = store.undo(1, true, at(4)).await.unwrap();
    assert!(moved);
    assert_eq!(head(&worktree), start);
    assert_eq!(read(&worktree, "notes.txt").as_deref(), Some("one\ntwo\n"));
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&worktree)
        .output()
        .unwrap()
        .stdout;
    assert!(status.is_empty(), "{}", String::from_utf8_lossy(&status));

    // And forward again, from the undo's own entry.
    let forward = store.plan(undo).await.unwrap().branch.unwrap();
    assert!(forward.forward && forward.refused.is_none(), "{forward:?}");
    store.undo(undo, true, at(5)).await.unwrap();
    assert_eq!(head(&worktree), pushed);
    assert_eq!(
        read(&worktree, "notes.txt").as_deref(),
        Some("committed by the agent\n")
    );
}

#[tokio::test]
async fn a_large_untracked_file_is_left_out_of_snapshots() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let large = std::fs::File::create(worktree.join("dump.bin")).unwrap();
    large.set_len(MAX_UNTRACKED + 1).unwrap();
    write(&worktree, "small.txt", "kept\n");
    let snapshot = store.snapshot().await.unwrap();
    let listing = store
        .inside(&["ls-tree", "-r", "--name-only", &snapshot.tree])
        .await
        .unwrap();
    assert!(listing.lines().any(|line| line == "small.txt"), "{listing}");
    assert!(!listing.lines().any(|line| line == "dump.bin"), "{listing}");
}

#[tokio::test]
async fn old_entries_go_and_the_store_is_private_and_removed_with_its_workspace() {
    let Setup {
        _dir,
        worktree,
        store,
        ..
    } = setup().await;
    let session = SessionId::new();
    for turn in 0..5 {
        store
            .began(session, &[session], at(turn * 2))
            .await
            .unwrap();
        write(&worktree, "notes.txt", &format!("turn {turn}\n"));
        store
            .ended(session, "Claude Code", at(turn * 2 + 1))
            .await
            .unwrap();
    }
    store.tidy_to(2, 1).await.unwrap();
    let numbers: Vec<u64> = store
        .timeline()
        .await
        .unwrap()
        .entries
        .iter()
        .map(|entry| entry.number)
        .collect();
    assert_eq!(numbers, [5, 4]);
    // The kept ones still read in full.
    assert!(store.diff(4).await.unwrap().text.contains("+turn 3"));

    let timelines = store.dir.parent().unwrap();
    let mode = std::fs::metadata(timelines).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o700);
    remove(&store.dir).await;
    assert!(!store.dir.exists());
    remove(&store.dir).await;
}
