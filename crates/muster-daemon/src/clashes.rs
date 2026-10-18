//! Where the work in a project's workspaces meets (`PRODUCT.md` §6), asked
//! of git without changing anything of the person's (`PRD-14`).
//!
//! Each workspace's files are kept as a snapshot in a store of Muster's
//! own for the project, `<data>/clashes/<project>`: the timeline's kind of
//! store (`timeline::Store`, borrowing the repository's objects), with an
//! index for each workspace. What each workspace's work changes since it
//! left its base is listed from its snapshot, and each two that change
//! files alike are merged there by `git merge-tree`, which says where they
//! would conflict and writes only in the store. Each snapshot is kept
//! under `refs/snapshots/<workspace>`, so tidying the store keeps what its
//! index names; the store goes with its project (`ARC-14`).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use muster_core::clashes::{self, Clash};
use muster_core::{ProjectId, Timestamp, WorkspaceId, changes};

use crate::git::{self, GitError};
use crate::timeline::{Store, TimelineError};

/// The most of a listing read from git.
const MAX_LISTING: usize = 16 * 1024 * 1024;

/// How many loose objects the store gathers before what no snapshot holds
/// goes.
const MAX_LOOSE: u64 = 2000;

/// Why where the work meets could not be told.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ClashError {
    #[error(transparent)]
    Store(#[from] TimelineError),
    #[error(transparent)]
    Git(#[from] GitError),
}

/// Where the store of `project` is kept.
pub(crate) fn dir(data_dir: &Path, project: ProjectId) -> PathBuf {
    data_dir.join("clashes").join(project.to_string())
}

/// A workspace whose work is compared.
#[derive(Clone, Debug)]
pub(crate) struct Side {
    pub(crate) workspace: WorkspaceId,
    pub(crate) path: PathBuf,
    /// What its branch is to merge into, such as `origin/main`.
    pub(crate) base: String,
}

/// A side's work, as found.
#[derive(Debug)]
struct Work {
    workspace: WorkspaceId,
    /// What its work changes since it left its base.
    paths: BTreeSet<String>,
    /// Its files as a commit on what is checked out, to merge; `None`
    /// before the repository's first commit.
    commit: Option<String>,
}

/// Where the work of `sides`, all of one project, meets: each side found,
/// with its clashes. Snapshots of workspaces not in `keep` are let go.
pub(crate) async fn check(
    dir: &Path,
    sides: &[Side],
    keep: &[WorkspaceId],
    at: Timestamp,
) -> Result<Vec<(WorkspaceId, Vec<Clash>)>, ClashError> {
    let mut found = Vec::new();
    let mut store = None;
    for side in sides {
        let index = index_of(dir, side.workspace);
        let opened = Store::open_with_index(dir, &side.path, index.clone()).await?;
        match work(&opened, side, at).await {
            Ok(work) => found.push(work),
            Err(error) => {
                // An index naming what the store no longer has is let go,
                // and made again the next time.
                tracing::debug!(%error, "could not take a workspace's snapshot to compare");
                let _ = tokio::fs::remove_file(&index).await;
            }
        }
        store = Some(opened);
    }
    let Some(store) = store else {
        return Ok(Vec::new());
    };
    let mut clashes: Vec<(WorkspaceId, Vec<Clash>)> = found
        .iter()
        .map(|work| (work.workspace, Vec::new()))
        .collect();
    for (at_one, one) in found.iter().enumerate() {
        for (at_other, other) in found.iter().enumerate().skip(at_one + 1) {
            let shared = clashes::overlap(&one.paths, &other.paths);
            if shared.is_empty() {
                continue;
            }
            let conflicted = match (&one.commit, &other.commit) {
                (Some(mine), Some(theirs)) => conflicts(&store, mine, theirs).await,
                _ => BTreeSet::new(),
            };
            if let Some(clash) = clashes::clash(other.workspace, &shared, &conflicted) {
                clashes[at_one].1.push(clash);
            }
            if let Some(clash) = clashes::clash(one.workspace, &shared, &conflicted) {
                clashes[at_other].1.push(clash);
            }
        }
    }
    tidy(&store, dir, keep).await?;
    Ok(clashes)
}

/// Lets go of a project's store, as the project goes.
pub(crate) async fn remove(dir: &Path) {
    match tokio::fs::remove_dir_all(dir).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => tracing::warn!(%error, "could not remove a project's store of snapshots"),
    }
}

fn index_of(dir: &Path, workspace: WorkspaceId) -> PathBuf {
    dir.join("indexes").join(workspace.to_string())
}

fn snapshot_ref(workspace: WorkspaceId) -> String {
    format!("refs/snapshots/{workspace}")
}

/// The work in `side`'s workspace now.
async fn work(store: &Store, side: &Side, at: Timestamp) -> Result<Work, ClashError> {
    let snapshot = store.snapshot().await?;
    store
        .inside(&["update-ref", &snapshot_ref(side.workspace), &snapshot.tree])
        .await?;
    let from = match &snapshot.head {
        Some(head) => git::answer(
            &side.path,
            &["merge-base", "--end-of-options", &side.base, head],
        )
        .await?
        .filter(|id| muster_core::timeline::is_object_id(id))
        // No base found: only what is not committed.
        .unwrap_or_else(|| head.clone()),
        None => git::checked(&side.path, &["hash-object", "-t", "tree", "/dev/null"]).await?,
    };
    let (listing, _) = store
        .inside_allowing(
            &[
                "diff",
                "-z",
                "--no-renames",
                "--name-only",
                "--end-of-options",
                &from,
                &snapshot.tree,
            ],
            MAX_LISTING,
            &[0],
        )
        .await?;
    let paths = changes::parse_paths(&String::from_utf8_lossy(&listing))
        .into_iter()
        .collect();
    let commit = match &snapshot.head {
        Some(head) => Some(
            store
                .commit(&snapshot.tree, Some(head), "muster snapshot to compare", at)
                .await?,
        ),
        None => None,
    };
    Ok(Work {
        workspace: side.workspace,
        paths,
        commit,
    })
}

/// The paths where merging `mine` and `theirs` would conflict; none where
/// git cannot tell, as with histories that share nothing.
async fn conflicts(store: &Store, mine: &str, theirs: &str) -> BTreeSet<String> {
    let merged = store
        .inside_allowing(
            &[
                "merge-tree",
                "--write-tree",
                "--name-only",
                "-z",
                "--no-messages",
                mine,
                theirs,
            ],
            MAX_LISTING,
            // 1: it merged, with conflicts.
            &[0, 1],
        )
        .await;
    match merged {
        Ok((out, _)) => clashes::parse_conflicts(&String::from_utf8_lossy(&out), [mine, theirs]),
        Err(error) => {
            tracing::debug!(%error, "git could not merge two workspaces' work to compare");
            BTreeSet::new()
        }
    }
}

/// Lets go of snapshots of workspaces not in `keep`, and once enough has
/// gathered, of what no snapshot holds.
async fn tidy(store: &Store, dir: &Path, keep: &[WorkspaceId]) -> Result<(), ClashError> {
    let listing = store
        .inside(&[
            "for-each-ref",
            "--format=%(refname:lstrip=2)",
            "refs/snapshots/",
        ])
        .await?;
    for gone in listing
        .lines()
        .filter_map(|name| name.trim().parse::<WorkspaceId>().ok())
        .filter(|workspace| !keep.contains(workspace))
    {
        store
            .inside(&["update-ref", "-d", &snapshot_ref(gone)])
            .await?;
        let _ = tokio::fs::remove_file(index_of(dir, gone)).await;
    }
    let counted = store.inside(&["count-objects"]).await?;
    let loose: u64 = counted
        .split_whitespace()
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or(0);
    if loose > MAX_LOOSE {
        store.inside(&["gc", "--quiet", "--prune=now"]).await?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "clashes_tests.rs"]
mod tests;
