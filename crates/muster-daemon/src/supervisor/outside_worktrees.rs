//! Worktrees made or removed outside Muster (`ARCHITECTURE.md` §5), by
//! hand or by an agent running `git worktree add`: adopted or forgotten as
//! git's list, read by `crate::worktree_sync`, says. Those Muster itself is
//! making or removing off the loop are left to that work.

use std::path::{Path, PathBuf};

use muster_core::protocol::Delta;
use muster_core::{ProjectId, Workspace, WorkspaceId, WorkspaceKind};

use super::Supervisor;
use crate::git;
use crate::worktree_sync::{Listed, ProjectWorktrees};

impl Supervisor {
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
    /// what Muster has just done, and one Muster is making or removing is
    /// left to that work.
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
                || self.worktree_jobs.is_making(&path)
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
            .filter(|workspace| !self.worktree_jobs.is_removing(workspace.id))
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
}
