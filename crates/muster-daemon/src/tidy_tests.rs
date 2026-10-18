//! Which commits a tidy may take, and what it came to, against real
//! repositories (`PRODUCT.md` §6, `PRD-14`).

use std::path::{Path, PathBuf};

use super::*;
use crate::git::tests::{git, repository};

fn write(dir: &Path, file: &str, text: &str) {
    std::fs::write(dir.join(file), text).unwrap();
}

fn commit(dir: &Path, file: &str, text: &str, message: &str) {
    write(dir, file, text);
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", message]);
}

/// A repository on `main` pushed to its remote, and a branch `work`
/// checked out with `pushed` commits pushed and `local` not.
fn setup(pushed: usize, local: usize) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let remote = dir.path().join("remote.git");
    git(
        dir.path(),
        &["init", "--quiet", "--bare", &remote.to_string_lossy()],
    );
    let repo = repository(dir.path(), "app");
    git(
        &repo,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    );
    git(&repo, &["push", "--quiet", "origin", "main"]);
    git(&repo, &["switch", "--quiet", "-c", "work"]);
    for at in 0..pushed {
        commit(&repo, "notes.txt", &format!("pushed {at}\n"), "pushed");
    }
    if pushed > 0 {
        git(&repo, &["push", "--quiet", "origin", "work"]);
    }
    for at in 0..local {
        commit(&repo, "notes.txt", &format!("local {at}\n"), "wip");
    }
    (dir, repo)
}

fn head(dir: &Path) -> String {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .unwrap()
        .stdout;
    String::from_utf8_lossy(&out).trim().to_owned()
}

#[tokio::test]
async fn only_the_commits_not_pushed_are_taken_and_only_from_a_clean_branch() {
    let (_dir, repo) = setup(1, 3);
    let pushed = {
        let out = std::process::Command::new("git")
            .args(["rev-parse", "origin/work"])
            .current_dir(&repo)
            .output()
            .unwrap()
            .stdout;
        String::from_utf8_lossy(&out).trim().to_owned()
    };
    let start = plan(&repo, "origin/main").await.unwrap();
    assert_eq!(start.commits, 3);
    assert_eq!(start.from, pushed);
    assert_eq!(start.head, head(&repo));

    write(&repo, "README.md", "not committed\n");
    assert!(matches!(
        plan(&repo, "origin/main").await,
        Err(TidyError::NotClean)
    ));
}

#[tokio::test]
async fn nothing_to_tidy_or_all_pushed_or_a_merge_is_refused() {
    let (_dir, repo) = setup(0, 0);
    assert!(matches!(
        plan(&repo, "origin/main").await,
        Err(TidyError::Nothing(_))
    ));

    let (_dir, repo) = setup(2, 0);
    let refused = plan(&repo, "origin/main").await.unwrap_err();
    assert!(matches!(refused, TidyError::AllPushed(_)));
    assert!(refused.to_string().contains("force push"), "{refused}");

    let (_dir, repo) = setup(0, 1);
    git(&repo, &["switch", "--quiet", "-c", "side", "main"]);
    commit(&repo, "side.txt", "side\n", "side");
    git(&repo, &["switch", "--quiet", "work"]);
    git(&repo, &["merge", "--quiet", "--no-edit", "--no-ff", "side"]);
    assert!(matches!(
        plan(&repo, "origin/main").await,
        Err(TidyError::Merges)
    ));
}

#[tokio::test]
async fn a_tidy_that_keeps_the_files_is_told_apart_from_one_that_does_not() {
    let (_dir, repo) = setup(1, 3);
    let start = plan(&repo, "origin/main").await.unwrap();
    assert_eq!(check(&repo, &start).await.unwrap(), TidyCheck::Untouched);

    // Squashed into one: the files are as they were.
    git(&repo, &["reset", "--quiet", "--soft", &start.from]);
    git(&repo, &["commit", "--quiet", "-m", "One clear change"]);
    assert_eq!(
        check(&repo, &start).await.unwrap(),
        TidyCheck::Kept { commits: 1 }
    );

    // Left staged, not committed.
    git(&repo, &["reset", "--quiet", "--soft", &start.from]);
    assert!(matches!(
        check(&repo, &start).await.unwrap(),
        TidyCheck::Astray { .. }
    ));

    // A file changed on the way.
    git(&repo, &["commit", "--quiet", "-m", "One clear change"]);
    commit(&repo, "notes.txt", "something else\n", "oops");
    assert_eq!(
        check(&repo, &start).await.unwrap(),
        TidyCheck::FilesChanged { files: 1 }
    );

    // The branch moved off the pushed commit it followed.
    git(&repo, &["reset", "--quiet", "--hard", "main"]);
    commit(&repo, "other.txt", "elsewhere\n", "elsewhere");
    let checked = check(&repo, &start).await.unwrap();
    assert!(
        matches!(&checked, TidyCheck::Astray { reason } if reason.contains("no longer follows")),
        "{checked:?}"
    );
}
