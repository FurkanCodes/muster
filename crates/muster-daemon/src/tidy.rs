//! Tidying a branch's commits (`PRODUCT.md` §6, `PRD-14`), asked of git
//! in the workspace: which commits an agent may tidy (those not pushed
//! yet, after its base, in a line), and once it stops, whether the files
//! are as they began. Only read: the agent does the tidying, as the
//! person asked.

use std::path::Path;

use muster_core::tidy::TidyCheck;
use muster_core::timeline::is_object_id;

use crate::git::{self, GitError};

/// Why a branch's commits cannot be tidied.
#[derive(Debug, thiserror::Error)]
pub(crate) enum TidyError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(
        "some changes are not committed: commit them or put them aside first, as a tidy moves \
         only commits"
    )]
    NotClean,
    #[error("there are no commits since {0} to tidy")]
    Nothing(String),
    #[error("every commit since {0} is pushed, and tidying them would need a force push")]
    AllPushed(String),
    #[error("its commits not pushed include a merge: tidy them by hand")]
    Merges,
}

/// What a tidy starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    /// The commit the commits to tidy follow.
    pub(crate) from: String,
    /// How many there are.
    pub(crate) commits: u64,
    /// The files as they are committed now, which must stay.
    pub(crate) tree: String,
    /// What is checked out now.
    pub(crate) head: String,
}

/// What a tidy of the branch checked out in the workspace at `path`,
/// whose base is `base` (such as `origin/main`), would start from: the
/// commits on no remote and not in the base, which must be the newest in
/// a line. Refused while anything tracked is not committed.
pub(crate) async fn plan(path: &Path, base: &str) -> Result<Start, TidyError> {
    if !clean(path).await? {
        return Err(TidyError::NotClean);
    }
    let head = git::checked(path, &["rev-parse", "--verify", "HEAD^{commit}"]).await?;
    let mut args = vec!["rev-list", "--count", "HEAD", "--not", "--remotes"];
    let has_base = git::answer(
        path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{base}^{{commit}}"),
        ],
    )
    .await?
    .is_some();
    if has_base {
        args.push(base);
    }
    let unpushed: u64 = git::checked(path, &args).await?.parse().unwrap_or(0);
    if unpushed == 0 {
        let since: u64 = if has_base {
            git::checked(
                path,
                &["rev-list", "--count", &format!("{base}..HEAD"), "--"],
            )
            .await?
            .parse()
            .unwrap_or(0)
        } else {
            0
        };
        return Err(if since == 0 {
            TidyError::Nothing(base.to_owned())
        } else {
            TidyError::AllPushed(base.to_owned())
        });
    }
    // The commits not pushed must be the newest, one after another.
    let from = git::answer(
        path,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("HEAD~{unpushed}^{{commit}}"),
        ],
    )
    .await?
    .filter(|id| is_object_id(id))
    .ok_or(TidyError::Merges)?;
    let range = format!("{from}..HEAD");
    let in_line: u64 = git::checked(path, &["rev-list", "--count", &range, "--"])
        .await?
        .parse()
        .unwrap_or(0);
    let merges: u64 = git::checked(path, &["rev-list", "--count", "--merges", &range, "--"])
        .await?
        .parse()
        .unwrap_or(0);
    if in_line != unpushed || merges > 0 {
        return Err(TidyError::Merges);
    }
    let tree = git::checked(path, &["rev-parse", "HEAD^{tree}"]).await?;
    Ok(Start {
        from,
        commits: unpushed,
        tree,
        head,
    })
}

/// What the tidy in the workspace at `path` that began at `start` came to.
pub(crate) async fn check(path: &Path, start: &Start) -> Result<TidyCheck, GitError> {
    let head = git::checked(path, &["rev-parse", "--verify", "HEAD^{commit}"]).await?;
    if head == start.head && clean(path).await? {
        return Ok(TidyCheck::Untouched);
    }
    let follows = git::answer(path, &["merge-base", "--is-ancestor", &start.from, &head])
        .await?
        .is_some();
    if !follows {
        return Ok(TidyCheck::Astray {
            reason: "the branch no longer follows the commit its tidy began after".to_owned(),
        });
    }
    // The files as they are now, committed or not, against how they were.
    let changed = git::checked(
        path,
        &[
            "diff",
            "--no-ext-diff",
            "--name-only",
            "-z",
            &start.tree,
            "--",
        ],
    )
    .await?;
    let files = changed.split('\0').filter(|path| !path.is_empty()).count();
    if files > 0 {
        return Ok(TidyCheck::FilesChanged {
            files: u64::try_from(files).unwrap_or(u64::MAX),
        });
    }
    if !clean(path).await? {
        return Ok(TidyCheck::Astray {
            reason: "the tidy left changes not committed".to_owned(),
        });
    }
    let commits = git::checked(
        path,
        &[
            "rev-list",
            "--count",
            &format!("{}..{head}", start.from),
            "--",
        ],
    )
    .await?
    .parse()
    .unwrap_or(0);
    Ok(TidyCheck::Kept { commits })
}

/// Whether nothing git tracks in the workspace at `path` differs from
/// what is committed.
async fn clean(path: &Path) -> Result<bool, GitError> {
    Ok(
        git::checked(path, &["status", "--porcelain", "--untracked-files=no"])
            .await?
            .is_empty(),
    )
}

#[cfg(test)]
#[path = "tidy_tests.rs"]
mod tests;
