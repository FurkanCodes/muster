//! Keeping each folder of the worktrees directory in line with sharing
//! builds (`crate::shared_builds`, `Preferences::shared_builds`): as the
//! daemon starts, as the setting changes, and as a worktree goes.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muster_core::WorkspaceKind;

use super::Supervisor;
use crate::shared_builds;

impl Supervisor {
    /// Muster's Cargo config in each folder of the worktrees directory that
    /// holds a worktree while the person shares builds, and in none
    /// otherwise, the builds no longer shared deleted. `sweep` also deletes
    /// builds a daemon before moved aside and never deleted.
    pub(super) async fn settle_shared_builds(&mut self, sweep: bool) {
        let share = self.state.preferences.shared_builds;
        let holding: HashSet<PathBuf> = self
            .state
            .workspaces
            .iter()
            .filter_map(|workspace| match &workspace.kind {
                WorkspaceKind::Worktree { path, .. } => path.parent().map(Path::to_path_buf),
                _ => None,
            })
            .chain(
                // Those being made count already, so the builds stay.
                self.worktree_jobs
                    .making_paths()
                    .filter_map(|path| path.parent().map(Path::to_path_buf)),
            )
            .collect();
        let Ok(mut entries) = tokio::fs::read_dir(&self.worktrees_dir).await else {
            return;
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            if !entry.file_type().await.is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            let group = entry.path();
            if sweep {
                for aside in shared_builds::leftovers(&group).await {
                    self.tasks.spawn(shared_builds::remove(aside));
                }
            }
            // Worktrees are kept by their real paths.
            let real = tokio::fs::canonicalize(&group)
                .await
                .unwrap_or_else(|_| group.clone());
            let settled = if share && holding.contains(&real) {
                shared_builds::share(&group).await.map(|()| None)
            } else {
                shared_builds::stop_sharing(&group).await
            };
            match settled {
                Ok(Some(aside)) => self.tasks.spawn(shared_builds::remove(aside)),
                Ok(None) => {}
                Err(error) => tracing::warn!(
                    %error,
                    folder = %group.display(),
                    "could not settle the shared builds of a project's worktrees"
                ),
            }
        }
    }

    /// Before a worktree is made in `group`: its builds go to the shared
    /// folder from the first, while the person shares them.
    pub(super) async fn share_builds_in(&self, group: &Path) {
        if !self.state.preferences.shared_builds {
            return;
        }
        if let Err(error) = shared_builds::share(group).await {
            tracing::warn!(
                %error,
                folder = %group.display(),
                "could not share the builds of a project's worktrees"
            );
        }
    }
}
