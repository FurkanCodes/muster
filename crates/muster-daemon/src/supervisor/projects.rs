//! Adding and removing projects, their own checkouts, and keeping every
//! workspace's checked-out branch current.
//!
//! A project is a folder: a git repository, or a plain folder. A plain
//! folder has sessions and checks in the folder itself; what needs git
//! (worktrees, tickets, reviews, pull requests, runs) is refused there,
//! and becomes possible as soon as the folder is made a repository.

use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::spaces::Member;
use muster_core::text::one_line;
use muster_core::{Project, ProjectId, SpaceId, Workspace, WorkspaceId, WorkspaceKind};

use super::{Supervisor, store_closed, store_error};
use crate::branches;

impl Supervisor {
    pub(super) async fn add_project(
        &mut self,
        root: PathBuf,
        name: Option<String>,
        space: Option<SpaceId>,
    ) -> Result<Reply, ErrorBody> {
        self.space_to_join(space)?;
        let root = folder_root(&root).await?;
        if self
            .state
            .projects
            .iter()
            .any(|project| project.root == root)
        {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is already a project", root.display()),
            ));
        }
        // A name given, else the directory's, each only if something is left
        // once control characters are gone.
        let usable = |name: &str| Some(one_line(name).trim().to_owned()).filter(|n| !n.is_empty());
        let name = name
            .as_deref()
            .and_then(usable)
            .or_else(|| {
                root.file_name()
                    .and_then(|dir| usable(&dir.to_string_lossy()))
            })
            .unwrap_or_else(|| one_line(&root.display().to_string()));
        let project = Project {
            id: ProjectId::new(),
            name,
            root,
            created_at: self.clock.now(),
        };
        let checkout = self.checkout_of(&project).await;

        let (stored, stored_checkout) = (project.clone(), checkout.clone());
        self.store
            .call(move |store| store.add_project(&stored, &stored_checkout))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;

        self.state.projects.push(project.clone());
        self.state.workspaces.push(checkout.clone());
        let id = project.id;
        self.broadcast(Delta::ProjectUpserted(project));
        self.broadcast(Delta::WorkspaceUpserted(checkout));
        if let Some(space) = space
            && let Err(error) = self.join(space, Member::Project(id)).await
        {
            tracing::warn!(error = %error.message, "a project did not join its space");
        }
        Ok(Reply::ProjectAdded { id })
    }

    /// Stops tracking a project: Muster forgets it and what it stored about
    /// it, and leaves the repository alone. Worktrees must go first, since
    /// forgetting them would leave their directories behind unknown, and
    /// sessions must have ended.
    pub(super) async fn remove_project(&mut self, project: ProjectId) -> Result<Reply, ErrorBody> {
        let name = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .map(|found| found.name.clone())
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no project {project}")))?;
        let workspaces: Vec<WorkspaceId> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project)
            .map(|workspace| workspace.id)
            .collect();
        let worktrees = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project && workspace.kind.is_worktree())
            .count()
            + self.worktree_jobs.making_in(project);
        if worktrees > 0 {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{name} has {worktrees} worktree(s); remove them first"),
            ));
        }
        let runs = self
            .active_runs
            .values()
            .filter(|active| active.context.project == project)
            .count();
        if runs > 0 {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{name} has {runs} run(s) going; cancel them first"),
            ));
        }
        let running: usize = workspaces
            .iter()
            .map(|workspace| self.running_sessions_in(*workspace))
            .sum();
        if running > 0 {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("stop {name}'s {running} running session(s) first"),
            ));
        }

        self.store
            .call(move |store| store.delete_project(project))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;

        let sessions: Vec<_> = self
            .state
            .sessions
            .iter()
            .filter(|session| {
                session
                    .workspace
                    .is_some_and(|workspace| workspaces.contains(&workspace))
            })
            .map(|session| session.id)
            .collect();
        for session in &sessions {
            self.release_terminal(*session);
        }
        self.state
            .sessions
            .retain(|session| !sessions.contains(&session.id));
        self.state
            .workspaces
            .retain(|workspace| workspace.project != project);
        self.state.projects.retain(|kept| kept.id != project);
        self.broadcast(Delta::ProjectRemoved { id: project });
        self.forget_members(|member| match member {
            Member::Project(id) => id == project,
            Member::Session(id) => sessions.contains(&id),
        });
        for workspace in workspaces {
            self.forget_timeline(workspace);
        }
        self.forget_clashes_of(project);
        self.forget_tidies();
        Ok(Reply::Done)
    }

    /// Gives every project its own checkout as a workspace, for projects
    /// added before checkouts were workspaces.
    pub(super) async fn ensure_checkouts(&mut self) {
        let missing: Vec<Project> = self
            .state
            .projects
            .iter()
            .filter(|project| {
                !self.state.workspaces.iter().any(|workspace| {
                    workspace.project == project.id && workspace.kind.is_checkout()
                })
            })
            .cloned()
            .collect();
        for project in missing {
            let checkout = self.checkout_of(&project).await;
            let stored = checkout.clone();
            match self
                .store
                .call(move |store| store.insert_workspace(&stored))
                .await
            {
                Ok(Ok(())) => self.state.workspaces.push(checkout),
                Ok(Err(error)) => {
                    tracing::error!(project = %project.id, %error, "could not add its checkout");
                }
                Err(_) => tracing::error!("could not add a checkout: the store stopped"),
            }
        }
    }

    /// The root of `project` if it is a git repository, which `what` needs.
    pub(super) fn repository_root(
        &self,
        project: ProjectId,
        what: &str,
    ) -> Result<PathBuf, ErrorBody> {
        let found = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no project {project}")))?;
        if !crate::git::is_repository(&found.root) {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!(
                    "{} is a plain folder, not a git repository; {what} need one",
                    found.name
                ),
            ));
        }
        Ok(found.root.clone())
    }

    /// A workspace for the project's own checkout, on whatever it has
    /// checked out now: nothing, in a plain folder.
    async fn checkout_of(&self, project: &Project) -> Workspace {
        let root = project.root.clone();
        let branch = tokio::task::spawn_blocking(move || branches::checked_out(&root))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_else(|| "?".to_owned());
        Workspace {
            id: WorkspaceId::new(),
            project: project.id,
            kind: WorkspaceKind::Checkout {
                path: project.root.clone(),
                branch,
            },
            created_at: self.clock.now(),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
            ticket: None,
        }
    }

    /// Every host workspace's place and what it was last seen to have
    /// checked out. A run's container keeps the run's branch.
    pub(super) fn workspace_heads(&self) -> Vec<(WorkspaceId, PathBuf, String)> {
        self.state
            .workspaces
            .iter()
            .filter_map(|workspace| {
                Some((
                    workspace.id,
                    workspace.kind.host_path()?.to_owned(),
                    workspace.kind.branch().to_owned(),
                ))
            })
            .collect()
    }

    /// Stores and announces branches that changed under workspaces.
    pub(super) async fn record_branches(&mut self, changes: Vec<(WorkspaceId, String)>) {
        for (workspace, branch) in changes {
            let Some(found) = self
                .state
                .workspaces
                .iter_mut()
                .find(|candidate| candidate.id == workspace)
            else {
                // Removed since it was looked at.
                continue;
            };
            match &mut found.kind {
                WorkspaceKind::Worktree { branch: known, .. }
                | WorkspaceKind::Checkout { branch: known, .. }
                | WorkspaceKind::Container { branch: known, .. } => known.clone_from(&branch),
            }
            // The last branch's; the next look finds this one's.
            found.pull_request = None;
            found.changes = None;
            found.clashes = Vec::new();
            let project = found.project;
            let updated = found.clone();
            self.write(move |store| store.update_branch(workspace, &branch))
                .await;
            self.broadcast(Delta::WorkspaceUpserted(updated));
            // What changed on the branch before is not this one's.
            self.look_at_changes(Some(workspace));
            self.check_clashes(project);
        }
    }
}

/// The canonical path of the folder at `path`: a git repository or not.
async fn folder_root(path: &Path) -> Result<PathBuf, ErrorBody> {
    if !path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not an absolute path", path.display()),
        ));
    }
    let root = tokio::fs::canonicalize(path).await.map_err(|error| {
        ErrorBody::new(
            ErrorCode::BadRequest,
            format!("cannot open {}: {error}", path.display()),
        )
    })?;
    if !root.is_dir() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not a folder", root.display()),
        ));
    }
    Ok(root)
}
