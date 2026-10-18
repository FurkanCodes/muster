//! Moving a workspace's branch onto another commit, as the person asked
//! (`PRD-14`, `muster_core::stacks`): onto the newest of the branch it is
//! stacked on, of its base once that has merged, or of its base where it
//! is not stacked. Its own commits are replayed there by `git rebase
//! --onto`, after what is there is kept as an entry of its own, so the
//! move can be undone; one that would conflict is aborted, changing
//! nothing.

use std::ffi::OsStr;
use std::path::Path;

use muster_core::Timestamp;
use muster_core::changes;
use muster_core::stacks::Stacked;
use muster_core::timeline::{Entry, EntryKind, is_object_id};

use super::{Store, TimelineError, head, text};
use crate::git::{self, GitError};

/// What the person's request to move the branch knows.
#[derive(Clone, Debug)]
pub(crate) struct MovePlan {
    /// The base, such as `origin/main`.
    pub(crate) base: String,
    /// The branch it is stacked on, if any.
    pub(crate) stacked: Option<Stacked>,
    /// Whether the pull request from that branch has merged, as its git
    /// host last said.
    pub(crate) parent_merged: bool,
}

/// A move done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MoveDone {
    /// What it moved onto, by name.
    pub(crate) onto: String,
    /// How many of its own commits it has there.
    pub(crate) commits: u64,
    /// The entry keeping what was there.
    pub(crate) entry: u64,
    /// The branch it is stacked on now.
    pub(crate) stacked: Option<Stacked>,
}

impl Store {
    /// Moves the workspace's branch as `plan` says, at `at`.
    pub(crate) async fn move_onto(
        &self,
        plan: &MovePlan,
        at: Timestamp,
    ) -> Result<MoveDone, TimelineError> {
        let worktree = &self.worktree;
        let tracked_changes =
            git::checked(worktree, &["status", "--porcelain", "--untracked-files=no"]).await?;
        if !tracked_changes.is_empty() {
            return Err(TimelineError::NotMoved(
                "some changes are not committed: commit them or put them aside first".to_owned(),
            ));
        }
        let Some(now_head) = head(worktree).await? else {
            return Err(TimelineError::NotMoved(
                "the repository has no commit yet".to_owned(),
            ));
        };
        let to = destination(worktree, plan, &now_head).await?;
        let before = self.snapshot().await?;
        let kept = self.keep(&before, at).await?;
        rebase(worktree, &to).await?;
        let after = self.snapshot().await?;
        let entry = Entry {
            number: self.next_number().await?,
            kind: EntryKind::Moved {
                onto: to.onto.clone(),
            },
            started: at,
            ended: at,
            summary: None,
            head_before: Some(now_head),
            head_after: None,
        };
        let entry = self.record(entry, &kept, &after).await?;
        let commits = git::checked(
            worktree,
            &["rev-list", "--count", &format!("{}..HEAD", to.target), "--"],
        )
        .await?
        .parse()
        .unwrap_or(0);
        Ok(MoveDone {
            onto: to.onto,
            commits,
            entry,
            stacked: to.stacked,
        })
    }
}

/// Where a move goes, and from where.
struct Destination {
    /// What it moves onto, by name.
    onto: String,
    /// That commit.
    target: String,
    /// The commit its own commits follow, which stays behind.
    fork: String,
    /// The branch it is stacked on once moved.
    stacked: Option<Stacked>,
}

/// Where the branch checked out at `head` in the workspace at `worktree`
/// moves as `plan` says: onto the newest of the branch it is stacked on,
/// or of its base once that has merged or where it is not stacked.
async fn destination(
    worktree: &Path,
    plan: &MovePlan,
    head: &str,
) -> Result<Destination, TimelineError> {
    let base = commit_of(worktree, &plan.base).await?.ok_or_else(|| {
        TimelineError::NotMoved(format!("there is no {} to move onto", plan.base))
    })?;
    let to = if let Some(stacked) = &plan.stacked {
        let parent = commit_of(worktree, &format!("refs/heads/{}", stacked.branch)).await?;
        let merged = match &parent {
            Some(parent) => plan.parent_merged || is_ancestor(worktree, parent, &base).await?,
            None => true,
        };
        if let Some(parent) = parent.filter(|_| !merged) {
            Destination {
                onto: stacked.branch.clone(),
                target: parent.clone(),
                fork: stacked.at.clone(),
                stacked: Some(Stacked {
                    branch: stacked.branch.clone(),
                    at: parent,
                }),
            }
        } else {
            Destination {
                onto: plan.base.clone(),
                target: base,
                fork: stacked.at.clone(),
                stacked: None,
            }
        }
    } else {
        let fork = git::checked(worktree, &["merge-base", "--end-of-options", &base, head]).await?;
        Destination {
            onto: plan.base.clone(),
            target: base,
            fork,
            stacked: None,
        }
    };
    if !is_object_id(&to.fork) || !is_ancestor(worktree, &to.fork, head).await? {
        return Err(TimelineError::NotMoved(format!(
            "it no longer follows {}, where it left {}",
            short(&to.fork),
            to.onto
        )));
    }
    if to.fork == to.target {
        return Err(TimelineError::NotMoved(format!(
            "it is on the newest of {} already",
            to.onto
        )));
    }
    Ok(to)
}

/// Replays the commits after `to.fork` onto `to.target`; one that stops
/// is aborted, saying where it would conflict, or, where nothing
/// conflicts, what git said (no name to commit as, a file in the way),
/// so nothing changes.
async fn rebase(worktree: &Path, to: &Destination) -> Result<(), TimelineError> {
    let editor = OsStr::new("true");
    let rebased = git::checked_with(
        worktree,
        &[
            "-c",
            "rebase.updateRefs=false",
            "-c",
            "rebase.autoStash=false",
            "rebase",
            "--quiet",
            "--onto",
            &to.target,
            &to.fork,
        ],
        &[("GIT_EDITOR", editor), ("GIT_SEQUENCE_EDITOR", editor)],
    )
    .await;
    let said = match rebased {
        Ok(_) => return Ok(()),
        Err(GitError::Failed { stderr, .. }) => stderr,
        Err(error) => return Err(error.into()),
    };
    let conflicted = text(
        git::capped(
            worktree,
            &["diff", "-z", "--name-only", "--diff-filter=U"],
            super::MAX_LISTING,
            &[0],
        )
        .await
        .unwrap_or_default(),
    );
    let _ = git::answer(worktree, &["rebase", "--abort"]).await;
    let files: Vec<String> = changes::parse_paths(&conflicted)
        .iter()
        .take(3)
        .map(|path| muster_core::text::one_line(path))
        .collect();
    if files.is_empty() {
        return Err(TimelineError::NotMoved(format!(
            "git could not move it onto {}: {}; nothing changed",
            to.onto,
            muster_core::text::one_line(&said)
        )));
    }
    Err(TimelineError::NotMoved(format!(
        "moving onto {} would conflict in {}; nothing changed",
        to.onto,
        files.join(", ")
    )))
}

/// The commit `name` names in the workspace at `worktree`, if any.
async fn commit_of(worktree: &Path, name: &str) -> Result<Option<String>, GitError> {
    Ok(git::answer(
        worktree,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{name}^{{commit}}"),
        ],
    )
    .await?
    .filter(|id| is_object_id(id)))
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

fn short(id: &str) -> String {
    id.chars().take(7).collect()
}

#[cfg(test)]
#[path = "moving_tests.rs"]
mod tests;
