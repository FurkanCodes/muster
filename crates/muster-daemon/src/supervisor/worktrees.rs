//! Creating and removing git worktrees (`ARCHITECTURE.md` §5).
//!
//! Worktrees go under the worktrees directory, grouped by project. Removing
//! one deletes its directory but keeps its branch: the work on it is the
//! person's. Git runs while the Supervisor waits, so a slow checkout delays
//! other commands and status updates, but never keystrokes.

use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{ProjectId, Workspace, WorkspaceId, WorkspaceKind};

use super::{Supervisor, store_closed, store_error};
use crate::git::{self, GitError};

impl Supervisor {
    pub(super) async fn create_worktree(
        &mut self,
        project: ProjectId,
        branch: Option<String>,
    ) -> Result<Reply, ErrorBody> {
        let project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .cloned()
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no project {project}")))?;
        let branch = match branch.map(|name| name.trim().to_owned()) {
            Some(name) if !name.is_empty() => name,
            _ => default_branch(),
        };
        if !git::is_valid_branch_name(&project.root, &branch)
            .await
            .map_err(git_failure)?
        {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{branch:?} is not a valid branch name"),
            ));
        }
        if self.worktree_on(project.id, &branch).is_some() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{branch} already has a worktree"),
            ));
        }

        let group = self
            .worktrees_dir
            .join(slug(&project.root.file_name().map_or_else(
                || project.name.clone(),
                |name| name.to_string_lossy().into_owned(),
            )));
        tokio::fs::create_dir_all(&group).await.map_err(|error| {
            ErrorBody::new(
                ErrorCode::Internal,
                format!("cannot create {}: {error}", group.display()),
            )
        })?;
        let path = unused_path(&group, &slug(&branch)).await;
        let exists = git::branch_exists(&project.root, &branch)
            .await
            .map_err(git_failure)?;
        git::add_worktree(&project.root, &path, &branch, !exists)
            .await
            .map_err(|error| branch_clash(&error, &branch).unwrap_or_else(|| git_failure(error)))?;
        let path = tokio::fs::canonicalize(&path).await.unwrap_or(path);

        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: project.id,
            kind: WorkspaceKind::Worktree {
                path: path.clone(),
                branch,
            },
            created_at: self.clock.now(),
        };
        let stored = workspace.clone();
        let inserted = self
            .store
            .call(move |store| store.insert_workspace(&stored))
            .await
            .map_err(store_closed)
            .and_then(|result| result.map_err(store_error));
        if let Err(error) = inserted {
            // Do not leave a worktree nobody knows about.
            let _ = git::remove_worktree(&project.root, &path, true).await;
            return Err(error);
        }

        self.state.workspaces.push(workspace.clone());
        let id = workspace.id;
        self.broadcast(Delta::WorkspaceUpserted(workspace));
        Ok(Reply::WorktreeCreated { id })
    }

    pub(super) async fn remove_worktree(
        &mut self,
        workspace: WorkspaceId,
        force: bool,
    ) -> Result<Reply, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .cloned()
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no worktree {workspace}"))
            })?;
        let WorkspaceKind::Worktree { path, .. } = &found.kind;
        let root = self
            .state
            .projects
            .iter()
            .find(|project| project.id == found.project)
            .map(|project| project.root.clone())
            .ok_or_else(|| ErrorBody::new(ErrorCode::Internal, "the worktree's project is gone"))?;

        let running = self.running_sessions_in(workspace);
        if running > 0 {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("stop the worktree's {running} running session(s) first"),
            ));
        }

        let removed = if tokio::fs::try_exists(path).await.unwrap_or(true) {
            git::remove_worktree(&root, path, force).await
        } else {
            // Its directory was deleted by hand; git only needs to forget it.
            git::prune_worktrees(&root).await
        };
        removed.map_err(|error| match &error {
            GitError::Failed { stderr, .. } if !force && is_dirty(stderr) => ErrorBody::new(
                ErrorCode::Conflict,
                format!(
                    "{} has uncommitted changes; remove it with force to discard them",
                    path.display()
                ),
            ),
            _ => git_failure(error),
        })?;

        let at = self.clock.now();
        self.store
            .call(move |store| store.remove_workspace(workspace, at))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;

        self.state.workspaces.retain(|kept| kept.id != workspace);
        let released: Vec<_> = self
            .state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace)
            .map(|session| session.id)
            .collect();
        for session in &released {
            // Dropping the last handle releases the terminal's output.
            self.live.remove(session);
        }
        self.state
            .sessions
            .retain(|session| session.workspace != workspace);
        self.broadcast(Delta::WorkspaceRemoved { id: workspace });
        Ok(Reply::Done)
    }

    /// The live worktree of `project` on `branch`, if any.
    pub(super) fn worktree_on(&self, project: ProjectId, branch: &str) -> Option<&Workspace> {
        self.state.workspaces.iter().find(|workspace| {
            workspace.project == project
                && matches!(&workspace.kind, WorkspaceKind::Worktree { branch: used, .. } if used == branch)
        })
    }
}

/// `muster/` and six random characters.
fn default_branch() -> String {
    let random = WorkspaceId::new().ulid().to_string().to_lowercase();
    format!("muster/{}", &random[random.len() - 6..])
}

/// A file-name-safe form of `name`: `muster/fix-login` becomes `muster-fix-login`.
fn slug(name: &str) -> String {
    let slug: String = name
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => c,
            _ => '-',
        })
        .collect();
    let slug = slug.trim_matches(|c| c == '-' || c == '.');
    if slug.is_empty() {
        "worktree".to_owned()
    } else {
        slug.to_owned()
    }
}

/// `group/name`, or `group/name-2`, `-3`… if that is taken.
async fn unused_path(group: &Path, name: &str) -> PathBuf {
    let mut path = group.join(name);
    let mut suffix = 2;
    while tokio::fs::try_exists(&path).await.unwrap_or(true) {
        path = group.join(format!("{name}-{suffix}"));
        suffix += 1;
    }
    path
}

/// Git's refusal to create `branch` because another branch's name contains
/// it as a folder (`test` when `test/x` exists, or `test/x` when `test`
/// exists), explained with the branch in the way and a name that would do.
fn branch_clash(error: &GitError, branch: &str) -> Option<ErrorBody> {
    let GitError::Failed { stderr, .. } = error else {
        return None;
    };
    if !(stderr.contains("cannot lock ref") && stderr.contains("exists; cannot create")) {
        return None;
    }
    // "… 'refs/heads/<existing>' exists; cannot create …"
    let before = stderr.split(" exists; cannot create").next()?;
    let existing = before.rsplit("'refs/heads/").next()?.trim_end_matches('\'');
    Some(ErrorBody::new(
        ErrorCode::Conflict,
        format!(
            "git cannot create the branch {branch} because the branch {existing} exists \
             (one branch name cannot be a folder of another); choose another name, \
             such as {branch}-1"
        ),
    ))
}

fn is_dirty(stderr: &str) -> bool {
    stderr.contains("modified or untracked") || stderr.contains("contains modified")
}

fn git_failure(error: GitError) -> ErrorBody {
    let code = match &error {
        GitError::Failed { stderr, .. } if stderr.contains("already") => ErrorCode::Conflict,
        _ => ErrorCode::Internal,
    };
    ErrorBody::new(code, error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_safe_file_names() {
        assert_eq!(slug("muster/fix-login"), "muster-fix-login");
        assert_eq!(slug("feature/ünïcode name"), "feature--n-code-name");
        assert_eq!(slug("../.."), "worktree");
    }

    #[test]
    fn a_clashing_branch_name_is_explained() {
        let error = GitError::Failed {
            args: "worktree add -b test /w/test".to_owned(),
            stderr: "fatal: cannot lock ref 'refs/heads/test': 'refs/heads/test/beta38-candidate' \
                     exists; cannot create 'refs/heads/test'"
                .to_owned(),
        };
        let body = branch_clash(&error, "test").unwrap();
        assert_eq!(body.code, ErrorCode::Conflict);
        assert!(
            body.message
                .contains("the branch test/beta38-candidate exists"),
            "{}",
            body.message
        );
        assert!(body.message.contains("such as test-1"), "{}", body.message);

        let other = GitError::Failed {
            args: String::new(),
            stderr: "fatal: something else".to_owned(),
        };
        assert!(branch_clash(&other, "x").is_none());
    }

    #[test]
    fn default_branches_are_namespaced_and_short() {
        let branch = default_branch();
        assert!(branch.starts_with("muster/"));
        assert_eq!(branch.len(), "muster/".len() + 6);
        assert_ne!(branch, default_branch());
    }
}
