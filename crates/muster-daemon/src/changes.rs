//! What a workspace's work has changed (`PRODUCT.md` §6), asked of git in
//! the workspace: what is not committed against its last commit, and the
//! branch's commits since it left its base with what they changed, read
//! by `muster_core::changes`. Only read: nothing in the workspace changes.
//! A diff is the person's code, so it is sent to the client that asked and
//! never stored or logged (`PRD-06`).

use std::path::Path;

use muster_core::changes::{
    self, ChangeKind, ChangePart, Changes, FileChange, FileDiff, MAX_COMMITS, MAX_COUNTED, MAX_DIFF,
};

use crate::git::{self, GitError};

/// The most of a listing read from git, far beyond any real one.
const MAX_LISTING: usize = 16 * 1024 * 1024;

/// The most untracked files whose lines are counted; past these, they are
/// listed without.
const MAX_COUNTED_FILES: usize = 200;

/// Why a file's diff could not be shown.
#[derive(Debug, thiserror::Error)]
pub(crate) enum DiffError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("{0} is no longer changed there")]
    NotChanged(String),
}

/// A workspace's changes, with what they were measured from.
#[derive(Debug)]
struct Measured {
    changes: Changes,
    /// The last commit, or the empty tree in a repository with none yet.
    head: String,
    /// Where the branch left its base, if it can be found.
    merge_base: Option<String>,
}

/// What the work in the workspace at `path` has changed, its branch
/// measured against `base` (such as `origin/main`).
pub(crate) async fn changes(path: &Path, base: &str) -> Result<Changes, GitError> {
    Ok(measure(path, base).await?.changes)
}

/// The diff of the file at `file` in `part` of the change in the workspace
/// at `path`, as git prints it, cut at [`MAX_DIFF`]. Only a file git lists
/// as changed there is diffed, so a path asked for can name nothing else.
pub(crate) async fn file_diff(
    path: &Path,
    base: &str,
    part: ChangePart,
    file: &str,
) -> Result<FileDiff, DiffError> {
    let measured = measure(path, base).await?;
    let found = measured
        .changes
        .find(part, file)
        .ok_or_else(|| DiffError::NotChanged(muster_core::text::one_line(file)))?;
    let mut paths: Vec<&str> = found.from.iter().map(String::as_str).collect();
    paths.push(&found.path);
    let (raw, cut) = match (part, found.kind) {
        (ChangePart::Uncommitted, ChangeKind::Untracked) => {
            let mut args = diff_args(&["--no-index"]);
            args.extend(["--", "/dev/null", &found.path]);
            git::capped(path, &args, MAX_DIFF, &[0, 1]).await?
        }
        (ChangePart::Uncommitted, _) => {
            let mut args = diff_args(&["-M", "--end-of-options", &measured.head, "--"]);
            args.extend(paths);
            git::capped(path, &args, MAX_DIFF, &[0]).await?
        }
        (ChangePart::Branch, _) => {
            let Some(merge_base) = measured.merge_base.as_deref() else {
                return Err(DiffError::NotChanged(muster_core::text::one_line(file)));
            };
            let mut args = diff_args(&["-M", "--end-of-options", merge_base, "HEAD", "--"]);
            args.extend(paths);
            git::capped(path, &args, MAX_DIFF, &[0]).await?
        }
    };
    Ok(FileDiff::cleaned(&raw, cut))
}

/// `git diff` as the person's settings cannot change what it prints: no
/// colours, no external diff program.
fn diff_args<'a>(rest: &[&'a str]) -> Vec<&'a str> {
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    args.extend(rest);
    args
}

async fn measure(path: &Path, base: &str) -> Result<Measured, GitError> {
    let commit = git::answer(path, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
        .await?
        .filter(|id| !id.is_empty());
    let head = match &commit {
        Some(id) => id.clone(),
        // No commit yet: everything there is new against nothing.
        None => git::checked(path, &["hash-object", "-t", "tree", "/dev/null"]).await?,
    };
    let mut uncommitted = listed(path, &[&head]).await?;
    uncommitted.extend(untracked(path).await?);
    uncommitted.sort_by(|one, other| one.path.cmp(&other.path));

    let merge_base = match commit {
        Some(_) => git::answer(path, &["merge-base", "--end-of-options", base, "HEAD"])
            .await?
            .filter(|id| !id.is_empty()),
        None => None,
    };
    let (commits, more_commits, branch) = match &merge_base {
        Some(merge_base) => {
            let range = format!("{merge_base}..HEAD");
            let count: u64 =
                git::checked(path, &["rev-list", "--count", "--end-of-options", &range])
                    .await?
                    .parse()
                    .unwrap_or(0);
            let most = format!("--max-count={MAX_COMMITS}");
            let log = text(
                git::capped(
                    path,
                    &[
                        "log",
                        "-z",
                        "--no-color",
                        "--format=%h%x1f%s",
                        &most,
                        "--end-of-options",
                        &range,
                    ],
                    MAX_LISTING,
                    &[0],
                )
                .await?,
            );
            let commits = changes::parse_commits(&log);
            let listed_count = u64::try_from(commits.len()).unwrap_or(u64::MAX);
            let branch = listed(path, &[merge_base, "HEAD"]).await?;
            (commits, count.saturating_sub(listed_count), branch)
        }
        None => (Vec::new(), 0, Vec::new()),
    };
    Ok(Measured {
        changes: Changes {
            base: merge_base.is_some().then(|| base.to_owned()),
            uncommitted,
            commits,
            more_commits,
            branch,
        },
        head,
        merge_base,
    })
}

/// The files a diff between `revisions` (one: against the work tree)
/// changes, with their lines.
async fn listed(path: &Path, revisions: &[&str]) -> Result<Vec<FileChange>, GitError> {
    let mut args = vec![
        "diff",
        "-z",
        "-M",
        "--no-ext-diff",
        "--name-status",
        "--end-of-options",
    ];
    args.extend(revisions);
    let names = text(git::capped(path, &args, MAX_LISTING, &[0]).await?);
    args[4] = "--numstat";
    let counts = text(git::capped(path, &args, MAX_LISTING, &[0]).await?);
    Ok(changes::file_changes(&names, &counts))
}

/// The files git does not track and does not ignore, the first
/// [`MAX_COUNTED_FILES`] with their lines counted.
async fn untracked(path: &Path) -> Result<Vec<FileChange>, GitError> {
    let listing = text(
        git::capped(
            path,
            &["ls-files", "-z", "--others", "--exclude-standard"],
            MAX_LISTING,
            &[0],
        )
        .await?,
    );
    let mut files = Vec::new();
    for (index, file) in changes::parse_paths(&listing).into_iter().enumerate() {
        let contents = if index < MAX_COUNTED_FILES {
            contents(&path.join(&file)).await
        } else {
            None
        };
        files.push(changes::untracked(file, contents.as_deref()));
    }
    Ok(files)
}

/// A file's bytes, if it is a plain file no larger than [`MAX_COUNTED`];
/// a link is not followed.
async fn contents(file: &Path) -> Option<Vec<u8>> {
    let metadata = tokio::fs::symlink_metadata(file).await.ok()?;
    let small = usize::try_from(metadata.len()).is_ok_and(|len| len <= MAX_COUNTED);
    if !metadata.is_file() || !small {
        return None;
    }
    tokio::fs::read(file).await.ok()
}

fn text((bytes, _cut): (Vec<u8>, bool)) -> String {
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use muster_core::changes::LineCounts;

    use super::*;
    use crate::git::tests::{git, repository};

    fn write(dir: &Path, file: &str, text: &str) {
        std::fs::write(dir.join(file), text).unwrap();
    }

    fn commit(dir: &Path, message: &str) {
        git(dir, &["add", "-A"]);
        git(dir, &["commit", "--quiet", "-m", message]);
    }

    fn lines(added: u64, removed: u64) -> LineCounts {
        LineCounts { added, removed }
    }

    #[tokio::test]
    async fn a_branch_shows_its_commits_and_what_is_not_committed_yet() {
        let dir = tempfile::tempdir().unwrap();
        let root = repository(dir.path(), "app");
        git(&root, &["switch", "--quiet", "-c", "feature"]);
        write(&root, "app.rs", "one\ntwo\n");
        commit(&root, "Add the app");
        write(&root, "app.rs", "one\nthree\n");
        write(&root, "notes.md", "a\nb\nc\n");
        git(&root, &["mv", "README.md", "READ ME.md"]);

        let changes = changes(&root, "main").await.unwrap();
        assert_eq!(changes.base.as_deref(), Some("main"));
        assert_eq!(changes.commits.len(), 1);
        assert_eq!(changes.commits[0].subject, "Add the app");
        assert_eq!(changes.more_commits, 0);
        assert_eq!(changes.branch.len(), 1);
        assert_eq!(changes.branch[0].kind, ChangeKind::Added);
        assert_eq!(changes.branch[0].lines, Some(lines(2, 0)));

        let uncommitted: Vec<_> = changes
            .uncommitted
            .iter()
            .map(|file| (file.path.as_str(), file.kind, file.lines))
            .collect();
        assert_eq!(
            uncommitted,
            [
                ("READ ME.md", ChangeKind::Renamed, Some(lines(0, 0))),
                ("app.rs", ChangeKind::Modified, Some(lines(1, 1))),
                ("notes.md", ChangeKind::Untracked, Some(lines(3, 0))),
            ]
        );
        assert_eq!(changes.uncommitted[0].from.as_deref(), Some("README.md"));
        assert_eq!(changes.summary().unwrap().words(), "+6 −1");
    }

    #[tokio::test]
    async fn each_file_shows_its_own_diff_and_nothing_else_can_be_asked_for() {
        let dir = tempfile::tempdir().unwrap();
        let root = repository(dir.path(), "app");
        git(&root, &["switch", "--quiet", "-c", "feature"]);
        write(&root, "app.rs", "one\n");
        commit(&root, "Add the app");
        write(&root, "app.rs", "one\ntwo\n");
        write(&root, "new.txt", "fresh\n");
        write(dir.path(), "secret.txt", "outside\n");

        let branch = file_diff(&root, "main", ChangePart::Branch, "app.rs")
            .await
            .unwrap();
        assert!(branch.text.contains("+one"), "{}", branch.text);
        assert!(!branch.text.contains("+two"));
        let work = file_diff(&root, "main", ChangePart::Uncommitted, "app.rs")
            .await
            .unwrap();
        assert!(work.text.contains("+two"), "{}", work.text);
        let new = file_diff(&root, "main", ChangePart::Uncommitted, "new.txt")
            .await
            .unwrap();
        assert!(new.text.contains("+fresh"), "{}", new.text);
        assert!(!new.cut);

        for asked in ["../secret.txt", "README.md", "-p"] {
            assert!(matches!(
                file_diff(&root, "main", ChangePart::Uncommitted, asked).await,
                Err(DiffError::NotChanged(_))
            ));
        }
    }

    #[tokio::test]
    async fn a_long_diff_is_cut_at_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let root = repository(dir.path(), "app");
        let long = (0..60_000)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        write(&root, "long.txt", &long);
        let diff = file_diff(&root, "main", ChangePart::Uncommitted, "long.txt")
            .await
            .unwrap();
        assert!(diff.cut);
        assert!(diff.text.len() <= MAX_DIFF);
        assert!(diff.text.ends_with('\n'));
    }

    #[tokio::test]
    async fn without_a_base_or_a_commit_only_the_work_tree_is_shown() {
        let dir = tempfile::tempdir().unwrap();
        let root = repository(dir.path(), "app");
        write(&root, "README.md", "changed\n");
        let changes = changes(&root, "origin/nowhere").await.unwrap();
        assert_eq!(changes.base, None);
        assert!(changes.commits.is_empty() && changes.branch.is_empty());
        assert_eq!(changes.uncommitted.len(), 1);

        let fresh = dir.path().join("fresh");
        std::fs::create_dir(&fresh).unwrap();
        git(&fresh, &["init", "--quiet", "--initial-branch", "main"]);
        write(&fresh, "first.rs", "fn main() {}\n");
        git(&fresh, &["add", "first.rs"]);
        let first = super::changes(&fresh, "main").await.unwrap();
        assert_eq!(first.base, None);
        assert_eq!(first.uncommitted.len(), 1);
        assert_eq!(first.uncommitted[0].kind, ChangeKind::Added);
        let diff = file_diff(&fresh, "main", ChangePart::Uncommitted, "first.rs")
            .await
            .unwrap();
        assert!(diff.text.contains("+fn main() {}"), "{}", diff.text);
    }
}
