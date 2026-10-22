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
use crate::worktree_sync::{Listed, ProjectWorktrees};

impl Supervisor {
    pub(super) async fn create_worktree(
        &mut self,
        project: ProjectId,
        branch: Option<String>,
        on: Option<WorkspaceId>,
    ) -> Result<Reply, ErrorBody> {
        self.repository_root(project, "worktrees")?;
        let project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .cloned()
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no project {project}")))?;
        let branch = match branch.map(|name| name.trim().to_owned()) {
            Some(name) if !name.is_empty() => name,
            _ => {
                let overrides = self
                    .config_dir
                    .join("projects")
                    .join(format!("{}.toml", project.id));
                default_branch(crate::config::muster_prefix(project.root.clone(), overrides).await)
            }
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
        self.share_builds_in(&group).await;
        let path = unused_path(&group, &slug(&branch)).await;
        let exists = git::branch_exists(&project.root, &branch)
            .await
            .map_err(git_failure)?;
        let stacked_on = match on {
            Some(on) => Some(self.stack_on(project.id, on, &branch, exists).await?),
            None => None,
        };
        let start = stacked_on.as_ref().map(|stacked| stacked.at.as_str());
        git::add_worktree(&project.root, &path, &branch, !exists, start)
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
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on,
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
        self.worktree_created(id);
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
        let path = match &found.kind {
            WorkspaceKind::Worktree { path, .. } => path,
            WorkspaceKind::Checkout { .. } => {
                return Err(ErrorBody::new(
                    ErrorCode::BadRequest,
                    "that is the project's own checkout, which Muster never removes; \
                     remove the project instead",
                ));
            }
            WorkspaceKind::Container { .. } => {
                return Err(ErrorBody::new(
                    ErrorCode::BadRequest,
                    "that is a run's container, which goes when the run ends; \
                     cancel the run instead",
                ));
            }
        };
        let root = self
            .state
            .projects
            .iter()
            .find(|project| project.id == found.project)
            .map(|project| project.root.clone())
            .ok_or_else(|| ErrorBody::new(ErrorCode::Internal, "the worktree's project is gone"))?;

        // Its shells and plugins' panels end with it (`PRODUCT.md` §6); its
        // agents' work is the person's to stop.
        let (ending, agents): (Vec<_>, Vec<_>) = self
            .running_in(workspace)
            .into_iter()
            .partition(|(_, ends)| *ends);
        if !agents.is_empty() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!(
                    "stop the worktree's {} running session(s) first",
                    agents.len()
                ),
            ));
        }
        for (session, _) in ending {
            self.stop_session(session).await?;
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

        self.drop_workspace(workspace);
        // The project's last worktree there takes the shared builds along.
        self.settle_shared_builds(false).await;
        Ok(Reply::Done)
    }

    /// Each project's root and where its worktrees are, for the sync.
    pub(super) fn projects_for_sync(&self) -> Vec<ProjectWorktrees> {
        self.state
            .projects
            .iter()
            .map(|project| ProjectWorktrees {
                project: project.id,
                root: project.root.clone(),
                known: self
                    .state
                    .workspaces
                    .iter()
                    .filter(|workspace| workspace.project == project.id)
                    .filter(|workspace| workspace.kind.is_worktree())
                    .filter_map(|workspace| workspace.kind.host_path().map(Path::to_owned))
                    .collect(),
            })
            .collect()
    }

    /// Brings a project's worktrees in line with git's list, which may be a
    /// moment old: a worktree made outside Muster is adopted if its
    /// directory is there and no workspace has its path; one removed
    /// outside Muster is forgotten if its directory is gone and nothing
    /// runs in it. Checking the directories keeps a stale list from undoing
    /// what Muster has just done.
    pub(super) async fn sync_worktrees(&mut self, project: ProjectId, listed: Vec<Listed>) {
        let Some(root) = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .map(|found| found.root.clone())
        else {
            return;
        };
        let present: Vec<PathBuf> = listed
            .iter()
            .filter(|entry| !entry.prunable)
            .map(|entry| entry.path.clone())
            .collect();

        for entry in listed.into_iter().filter(|entry| !entry.prunable) {
            let path = tokio::fs::canonicalize(&entry.path)
                .await
                .unwrap_or(entry.path);
            let taken = path == root
                || self.state.projects.iter().any(|other| other.root == path)
                || self
                    .state
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.kind.host_path() == Some(path.as_path()));
            if taken || !tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_dir()) {
                continue;
            }
            self.adopt_worktree(project, path, entry.branch).await;
        }

        let gone: Vec<(WorkspaceId, PathBuf)> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project && workspace.kind.is_worktree())
            .filter_map(|workspace| Some((workspace.id, workspace.kind.host_path()?.to_owned())))
            .filter(|(_, path)| !present.contains(path))
            .collect();
        for (workspace, path) in gone {
            let exists = tokio::fs::try_exists(&path).await.unwrap_or(true);
            if exists || self.running_sessions_in(workspace) > 0 {
                continue;
            }
            tracing::info!(%workspace, path = %path.display(), "a worktree was removed outside Muster");
            self.forget_worktree(workspace).await;
            // Git still holds the gone worktree's record, which keeps its
            // branch checked out; its own cleanup lets the branch go.
            if let Err(error) = git::prune_worktrees(&root).await {
                tracing::warn!(%error, "could not prune worktrees");
            }
        }
    }

    /// Starts tracking a worktree made outside Muster.
    async fn adopt_worktree(&mut self, project: ProjectId, path: PathBuf, branch: String) {
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project,
            kind: WorkspaceKind::Worktree {
                path: path.clone(),
                branch: muster_core::text::one_line(&branch),
            },
            created_at: self.clock.now(),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
        };
        let stored = workspace.clone();
        match self
            .store
            .call(move |store| store.insert_workspace(&stored))
            .await
        {
            Ok(Ok(())) => {
                tracing::info!(path = %path.display(), "adopted a worktree made outside Muster");
                let id = workspace.id;
                self.state.workspaces.push(workspace.clone());
                self.broadcast(Delta::WorkspaceUpserted(workspace));
                self.worktree_created(id);
            }
            Ok(Err(error)) => tracing::error!(%error, "could not adopt a worktree"),
            Err(_) => tracing::error!("could not adopt a worktree: the store stopped"),
        }
    }

    /// Stops tracking a worktree that is already gone from disk.
    async fn forget_worktree(&mut self, workspace: WorkspaceId) {
        let at = self.clock.now();
        match self
            .store
            .call(move |store| store.remove_workspace(workspace, at))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::error!(%error, "could not forget a worktree");
                return;
            }
            Err(_) => {
                tracing::error!("could not forget a worktree: the store stopped");
                return;
            }
        }
        self.drop_workspace(workspace);
    }

    /// Takes a removed workspace and its sessions out of the state, and
    /// tells clients.
    pub(super) fn drop_workspace(&mut self, workspace: WorkspaceId) {
        self.workspace_going(workspace);
        self.forget_events_of(workspace);
        self.state.workspaces.retain(|kept| kept.id != workspace);
        let released: Vec<_> = self
            .state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace)
            .map(|session| session.id)
            .collect();
        for session in released {
            self.release_terminal(session);
        }
        self.state
            .sessions
            .retain(|session| session.workspace != workspace);
        self.broadcast(Delta::WorkspaceRemoved { id: workspace });
        self.forget_timeline(workspace);
        self.forget_clashes_with(workspace);
        self.forget_tidies();
    }

    /// The live worktree Muster made for `project` on `branch`, if any.
    pub(super) fn worktree_on(&self, project: ProjectId, branch: &str) -> Option<&Workspace> {
        self.state.workspaces.iter().find(|workspace| {
            workspace.project == project
                && matches!(&workspace.kind, WorkspaceKind::Worktree { branch: used, .. } if used == branch)
        })
    }
}

/// `muster/` and six random characters; without `muster/` unless
/// `prefixed`, `worktree-` and six, as six alone can read as a commit.
fn default_branch(prefixed: bool) -> String {
    let random = WorkspaceId::new().ulid().to_string().to_lowercase();
    let prefix = if prefixed { "muster/" } else { "worktree-" };
    format!("{prefix}{}", &random[random.len() - 6..])
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

#[expect(
    clippy::needless_pass_by_value,
    reason = "a converter for .map_err(f), which hands it the error by value"
)]
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
        let branch = default_branch(true);
        assert!(branch.starts_with("muster/"));
        assert_eq!(branch.len(), "muster/".len() + 6);
        assert_ne!(branch, default_branch(true));
        let without = default_branch(false);
        assert!(without.starts_with("worktree-"), "{without}");
        assert_eq!(without.len(), "worktree-".len() + 6);
    }
}
