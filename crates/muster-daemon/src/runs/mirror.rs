//! Muster's own copy of each project's repository (`ARCHITECTURE.md` §8):
//! a bare clone under the data directory, fetched before each run. Run
//! containers clone it from a read-only mount, so they cannot write to the
//! host (`PRD-04`); runs' branches come back into it as bundles and are
//! pushed from it with the person's own git credentials, which never enter
//! a container (`ARC-06`).
//!
//! It fetches branches only, from the checkout's `origin` (or from the
//! checkout itself when it has none), and pushes only the one branch named:
//! it is never a `--mirror` clone, whose pushes would mirror every ref.

use std::path::{Path, PathBuf};

use muster_core::ProjectId;

use crate::git::{self, GitError};

/// A project's mirror.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mirror {
    pub(crate) path: PathBuf,
}

/// Where the mirror of `project` lives.
pub(crate) fn path(data_dir: &Path, project: ProjectId) -> PathBuf {
    data_dir.join("mirrors").join(format!("{project}.git"))
}

/// Brings the mirror of the repository at `root` up to date, making it
/// the first time.
pub(crate) async fn refresh(
    data_dir: &Path,
    project: ProjectId,
    root: &Path,
) -> Result<Mirror, GitError> {
    let path = path(data_dir, project);
    let source = source(root).await?;
    if !path.join("HEAD").exists() {
        let parent = path.parent().unwrap_or(data_dir);
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(GitError::Spawn)?;
        // Cloned beside it and moved in place, so a clone cut short leaves
        // nothing that looks like a mirror.
        let fresh = parent.join(format!(".{project}.cloning"));
        let _ = tokio::fs::remove_dir_all(&fresh).await;
        let fresh_text = fresh.to_string_lossy().into_owned();
        git::checked(
            parent,
            &["clone", "--bare", "--quiet", &source, &fresh_text],
        )
        .await?;
        tokio::fs::rename(&fresh, &path)
            .await
            .map_err(GitError::Spawn)?;
    }
    // The checkout's remote may have moved since.
    git::checked(&path, &["remote", "set-url", "origin", &source]).await?;
    git::checked(
        &path,
        &[
            "fetch",
            "--quiet",
            "--prune",
            "origin",
            "+refs/heads/*:refs/heads/*",
        ],
    )
    .await?;
    Ok(Mirror { path })
}

/// Where the mirror fetches from: the checkout's `origin`, or the checkout.
async fn source(root: &Path) -> Result<String, GitError> {
    Ok(git::answer(root, &["remote", "get-url", "origin"])
        .await?
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().into_owned()))
}

impl Mirror {
    /// The branch runs start from: `wanted`, or the repository's default.
    pub(crate) async fn base_branch(&self, wanted: Option<&str>) -> Result<String, GitError> {
        let base = match wanted {
            Some(branch) => branch.to_owned(),
            None => git::checked(&self.path, &["symbolic-ref", "--short", "HEAD"]).await?,
        };
        let reference = format!("refs/heads/{base}");
        if git::answer(&self.path, &["show-ref", "--verify", "--quiet", &reference])
            .await?
            .is_none()
        {
            return Err(GitError::Failed {
                args: format!("show-ref {reference}"),
                stderr: format!("the repository has no branch {base}"),
            });
        }
        Ok(base)
    }

    /// Takes the person's own `branch` from their repository at `root`, for
    /// a run handed their work: fast-forward only, so commits a run pushed
    /// since are never taken back, and the mirror's own refresh (which
    /// keeps only `origin`'s branches) cannot lose it. Errs if the mirror
    /// then has no such branch.
    pub(crate) async fn take_local(&self, root: &Path, branch: &str) -> Result<(), GitError> {
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        let root = root.to_string_lossy();
        if let Err(error) = git::checked(&self.path, &["fetch", "--quiet", &root, &refspec]).await {
            // Behind what the mirror has, or gone from the repository: what
            // the mirror has stands, if it has the branch at all.
            tracing::debug!(%error, branch, "kept the mirror's copy of a branch");
        }
        let reference = format!("refs/heads/{branch}");
        if git::answer(&self.path, &["show-ref", "--verify", "--quiet", &reference])
            .await?
            .is_none()
        {
            return Err(GitError::Failed {
                args: format!("show-ref {reference}"),
                stderr: format!("the repository has no branch {branch}"),
            });
        }
        Ok(())
    }

    /// Takes `branch` from a bundle a container made, replacing what the
    /// mirror had of it.
    pub(crate) async fn take_bundle(&self, bundle: &Path, branch: &str) -> Result<(), GitError> {
        let bundle = bundle.to_string_lossy();
        let refspec = format!("+refs/heads/{branch}:refs/heads/{branch}");
        git::checked(&self.path, &["fetch", "--quiet", &bundle, &refspec]).await?;
        Ok(())
    }

    /// Pushes `branch`, and only it, to the repository the mirror fetches
    /// from.
    pub(crate) async fn push(&self, branch: &str) -> Result<(), GitError> {
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");
        git::checked(&self.path, &["push", "--quiet", "origin", &refspec]).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::{git, repository};

    #[tokio::test]
    async fn the_mirror_follows_the_checkout_and_pushes_only_the_branch_named() {
        let dir = tempfile::tempdir().unwrap();
        // A "remote" the checkout came from, with two branches.
        let remote = repository(dir.path(), "remote");
        git(&remote, &["branch", "other"]);
        let checkout = dir.path().join("checkout");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        let project = ProjectId::new();
        let data = dir.path().join("data");

        let mirror = refresh(&data, project, &checkout).await.unwrap();
        assert_eq!(mirror.path, path(&data, project));
        assert_eq!(mirror.base_branch(None).await.unwrap(), "main");
        assert_eq!(mirror.base_branch(Some("other")).await.unwrap(), "other");
        assert!(mirror.base_branch(Some("missing")).await.is_err());

        // New work on the remote arrives on the next refresh.
        git(&remote, &["branch", "later"]);
        let mirror = refresh(&data, project, &checkout).await.unwrap();
        assert_eq!(mirror.base_branch(Some("later")).await.unwrap(), "later");

        // A branch made elsewhere comes in as a bundle and goes out alone.
        let work = dir.path().join("work");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                mirror.path.to_str().unwrap(),
                work.to_str().unwrap(),
            ],
        );
        git(&work, &["checkout", "-q", "-b", "muster/issue-1"]);
        std::fs::write(work.join("fix.txt"), "fixed\n").unwrap();
        git(&work, &["add", "fix.txt"]);
        git(&work, &["commit", "-q", "-m", "Fix"]);
        let bundle = dir.path().join("work.bundle");
        git(
            &work,
            &[
                "bundle",
                "create",
                "-q",
                bundle.to_str().unwrap(),
                "muster/issue-1",
                "^origin/main",
            ],
        );
        mirror.take_bundle(&bundle, "muster/issue-1").await.unwrap();
        // The remote must accept pushes to a branch not checked out there.
        mirror.push("muster/issue-1").await.unwrap();
        let pushed = std::process::Command::new("git")
            .args(["-C", remote.to_str().unwrap(), "branch", "--list"])
            .output()
            .unwrap();
        let branches = String::from_utf8_lossy(&pushed.stdout);
        assert!(branches.contains("muster/issue-1"), "{branches}");
        assert!(branches.contains("other") && branches.contains("later"));
    }

    #[tokio::test]
    async fn a_checkout_without_a_remote_is_mirrored_from_itself() {
        let dir = tempfile::tempdir().unwrap();
        let checkout = repository(dir.path(), "solo");
        let mirror = refresh(&dir.path().join("data"), ProjectId::new(), &checkout)
            .await
            .unwrap();
        assert_eq!(mirror.base_branch(None).await.unwrap(), "main");
    }
}

#[cfg(test)]
mod local_tests {
    use super::*;
    use crate::git::tests::{git, repository};

    fn head(repo: &Path, branch: &str) -> String {
        let output = std::process::Command::new("git")
            .args(["-C", repo.to_str().unwrap(), "rev-parse", branch])
            .output()
            .unwrap();
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[tokio::test]
    async fn a_person_s_branch_comes_in_fast_forward_only_and_must_exist() {
        let dir = tempfile::tempdir().unwrap();
        let remote = repository(dir.path(), "remote");
        let checkout = dir.path().join("checkout");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                remote.to_str().unwrap(),
                checkout.to_str().unwrap(),
            ],
        );
        // Work on a branch only the person's repository has.
        git(&checkout, &["checkout", "-q", "-b", "feature"]);
        std::fs::write(checkout.join("a.txt"), "a\n").unwrap();
        git(&checkout, &["add", "a.txt"]);
        git(&checkout, &["commit", "-q", "-m", "A"]);
        let project = ProjectId::new();
        let data = dir.path().join("data");
        let mirror = refresh(&data, project, &checkout).await.unwrap();
        mirror.take_local(&checkout, "feature").await.unwrap();
        assert_eq!(head(&mirror.path, "feature"), head(&checkout, "feature"));

        // The refresh, keeping origin's branches only, drops it; taking it
        // again brings it back.
        let mirror = refresh(&data, project, &checkout).await.unwrap();
        assert_eq!(head(&mirror.path, "feature"), "feature");
        mirror.take_local(&checkout, "feature").await.unwrap();
        assert_eq!(head(&mirror.path, "feature"), head(&checkout, "feature"));

        // Pushed on by a run, the mirror is ahead: the person's older copy
        // does not take it back.
        let work = dir.path().join("work");
        git(
            dir.path(),
            &[
                "clone",
                "-q",
                "-b",
                "feature",
                mirror.path.to_str().unwrap(),
                work.to_str().unwrap(),
            ],
        );
        std::fs::write(work.join("b.txt"), "b\n").unwrap();
        git(&work, &["add", "b.txt"]);
        git(&work, &["commit", "-q", "-m", "B"]);
        git(&work, &["push", "-q", "origin", "feature"]);
        let ahead = head(&mirror.path, "feature");
        mirror.take_local(&checkout, "feature").await.unwrap();
        assert_eq!(head(&mirror.path, "feature"), ahead);

        // A branch neither has is an error.
        assert!(mirror.take_local(&checkout, "missing").await.is_err());
    }
}
