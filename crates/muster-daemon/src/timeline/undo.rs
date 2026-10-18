//! Putting a workspace's files back as they were before an entry of its
//! timeline, as the person asked, and its branch too where it can go
//! (`PRD-14`): first what that would do, then doing it, after keeping what
//! is there now as an entry of its own.

use std::path::Path;

use muster_core::Timestamp;
use muster_core::changes;
use muster_core::timeline::{self, BranchMove, Entry, EntryKind, Timeline, UndoPlan};

use super::{MAX_LISTING, Store, TimelineError, head, text};
use crate::git::{self, GitError};

impl Store {
    /// What putting the files back from entry `number` would do.
    pub(crate) async fn plan(&self, number: u64) -> Result<UndoPlan, TimelineError> {
        let kept = self.kept().await?;
        let timeline = Timeline {
            entries: kept.iter().map(|kept| kept.entry.clone()).collect(),
        };
        let found = kept
            .iter()
            .find(|kept| kept.entry.number == number)
            .ok_or(TimelineError::NoEntry(number))?;
        let branch = match &found.entry.head_before {
            Some(target) => self.branch_move(target).await?,
            None => None,
        };
        Ok(UndoPlan {
            number,
            later: timeline::later_than(&timeline, number),
            branch,
        })
    }

    /// Moving the workspace's branch to `target`, where it is elsewhere
    /// now: back over commits not pushed, or forward again.
    async fn branch_move(&self, target: &str) -> Result<Option<BranchMove>, GitError> {
        let worktree = &self.worktree;
        let Some(now) = head(worktree).await? else {
            return Ok(None);
        };
        if now == target {
            return Ok(None);
        }
        let short = git::checked(worktree, &["rev-parse", "--short", target])
            .await
            .unwrap_or_else(|_| target.chars().take(7).collect());
        let branch = git::answer(worktree, &["symbolic-ref", "--short", "-q", "HEAD"])
            .await?
            .filter(|name| !name.is_empty());
        let mut planned = BranchMove {
            branch: branch.clone().unwrap_or_default(),
            to: short,
            commits: 0,
            forward: false,
            refused: None,
        };
        if branch.is_none() {
            planned.refused = Some("no branch is checked out".to_owned());
            return Ok(Some(planned));
        }
        let exists = git::answer(
            worktree,
            &["cat-file", "-e", &format!("{target}^{{commit}}")],
        )
        .await?
        .is_some();
        if !exists {
            planned.refused = Some(format!("{} is gone from git", planned.to));
            return Ok(Some(planned));
        }
        // Both are whole object ids, so neither can read as an option.
        let count = |range: String, outside_remotes: bool| async move {
            let mut args = vec!["rev-list".to_owned(), "--count".to_owned(), range];
            if outside_remotes {
                args.extend(["--not".to_owned(), "--remotes".to_owned()]);
            }
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            git::checked(worktree, &args)
                .await
                .map(|count| count.parse::<u64>().unwrap_or(0))
        };
        if is_ancestor(worktree, target, &now).await? {
            planned.commits = count(format!("{target}..{now}"), false).await?;
            let unpushed = count(format!("{target}..{now}"), true).await?;
            if unpushed < planned.commits {
                planned.refused = Some(
                    "its commits since are pushed, and moving it back would need a force push"
                        .to_owned(),
                );
            }
        } else if is_ancestor(worktree, &now, target).await? {
            planned.forward = true;
            planned.commits = count(format!("{now}..{target}"), false).await?;
        } else {
            planned.refused = Some("it has moved another way since, as by a rebase".to_owned());
        }
        Ok(Some(planned))
    }

    /// Puts the workspace's files back as they were before entry
    /// `number`, and with `move_branch` its branch too, as the person
    /// asked; what was there is an entry of its own first, so this can be
    /// undone in turn (`PRD-14`). Returns that entry's number, and whether
    /// the branch moved.
    pub(crate) async fn undo(
        &self,
        number: u64,
        move_branch: bool,
        at: Timestamp,
    ) -> Result<(u64, bool), TimelineError> {
        let found = self.find(number).await?;
        let before = found.before.clone().ok_or(TimelineError::NoEntry(number))?;
        let target = self
            .inside(&["rev-parse", &format!("{before}^{{tree}}")])
            .await?;
        let (missing, _) = self
            .inside_raw(
                &["rev-list", "--objects", "--missing=print", &target],
                MAX_LISTING,
            )
            .await?;
        if String::from_utf8_lossy(&missing)
            .lines()
            .any(|line| line.starts_with('?'))
        {
            return Err(TimelineError::Gone(number));
        }
        let branch = match (&found.entry.head_before, move_branch) {
            (Some(head), true) => match self.branch_move(head).await? {
                Some(BranchMove {
                    refused: Some(why), ..
                }) => return Err(TimelineError::Branch(why)),
                planned => planned.map(|_| head.clone()),
            },
            _ => None,
        };
        let now = self.snapshot().await?;
        if now.tree == target && branch.is_none() {
            return Err(TimelineError::AlreadyThere(number));
        }
        if let Some(path) = self.in_the_way(&now.tree, &target).await? {
            return Err(TimelineError::Refused(format!(
                "{path} is there and no snapshot keeps it, as git ignores it or it is too \
                 large; move it away first"
            )));
        }
        let found_from = self.keep(&now, at).await?;
        if now.tree != target {
            self.over_files(&["read-tree", "-m", "-u", &now.tree, &target])
                .await
                .map_err(|error| match error {
                    GitError::Failed { stderr, .. } => TimelineError::Refused(stderr),
                    other => TimelineError::Git(other),
                })?;
        }
        if let Some(head) = &branch {
            git::checked(&self.worktree, &["reset", "--quiet", "--mixed", head]).await?;
        }
        let after = self.snapshot().await?;
        let entry = Entry {
            number: self.next_number().await?,
            kind: EntryKind::Undo { of: number },
            started: at,
            ended: at,
            summary: None,
            head_before: now.head.clone(),
            head_after: None,
        };
        let recorded = self.record(entry, &found_from, &after).await?;
        Ok((recorded, branch.is_some()))
    }

    /// A file putting the files back from `now` to `target` would write
    /// over that no snapshot keeps: one `target` has and `now` does not,
    /// yet there in the workspace, ignored or too large to keep. Git
    /// writes over ignored files, so this is asked first.
    async fn in_the_way(&self, now: &str, target: &str) -> Result<Option<String>, GitError> {
        let listing = text(
            self.inside_raw(
                &[
                    "diff",
                    "-z",
                    "--no-renames",
                    "--name-only",
                    "--diff-filter=A",
                    "--end-of-options",
                    now,
                    target,
                ],
                MAX_LISTING,
            )
            .await?,
        );
        for path in changes::parse_paths(&listing) {
            let file = self.worktree.join(&path);
            if tokio::fs::symlink_metadata(&file).await.is_ok() {
                return Ok(Some(muster_core::text::one_line(&path)));
            }
            // A file where one of its folders must go is in the way too.
            for folder in file.ancestors().skip(1) {
                if folder == self.worktree {
                    break;
                }
                if let Ok(found) = tokio::fs::symlink_metadata(folder).await
                    && !found.is_dir()
                {
                    let shown = folder.strip_prefix(&self.worktree).unwrap_or(folder);
                    return Ok(Some(muster_core::text::one_line(&shown.to_string_lossy())));
                }
            }
        }
        Ok(None)
    }
}

async fn is_ancestor(worktree: &Path, older: &str, newer: &str) -> Result<bool, GitError> {
    Ok(git::answer(
        worktree,
        &[
            "merge-base",
            "--is-ancestor",
            "--end-of-options",
            older,
            newer,
        ],
    )
    .await?
    .is_some())
}
