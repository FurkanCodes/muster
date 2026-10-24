//! Planning a new worktree. What the loop knows is taken in [`Asked`];
//! git and the file system are asked off the loop (`ARC-13`): the branch's
//! name and whether it exists, the folder it goes in and what it is
//! stacked on; and the plan comes back as [`Planned`], checked again on the
//! loop against what was taken meanwhile before git makes it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muster_core::protocol::{ErrorBody, ErrorCode};
use muster_core::stacks::Stacked;
use muster_core::{Project, ProjectId, WorkspaceId};

use super::{Branch, Plan, Then, default_branch, git_failure, slug};
use crate::git;
use crate::supervisor::{Request, Supervisor};

/// What planning a worktree needs that the loop knows.
pub(super) struct Asked {
    project: Project,
    overrides: PathBuf,
    branch: Branch,
    /// The branches the project's workspaces have, or are being made on.
    taken: HashSet<String>,
    /// The worktrees directory.
    dir: PathBuf,
    /// The folders the project's worktrees are in, and other projects'.
    own_groups: Vec<PathBuf>,
    other_groups: Vec<PathBuf>,
    /// The folders worktrees are being made in.
    making: Vec<PathBuf>,
    shared_builds: bool,
    /// The workspace's branch it is stacked on, and where that is.
    on: Option<(PathBuf, String)>,
}

/// A plan made off the loop, on its way back.
pub(crate) struct Planned {
    plan: Result<Plan, ErrorBody>,
    then: Then,
}

impl Supervisor {
    /// Makes a worktree of `project` on `branch`, stacked on `on`'s branch
    /// if given; `then` follows once it is made.
    pub(in crate::supervisor) fn create_worktree(
        &self,
        project: ProjectId,
        branch: Branch,
        on: Option<WorkspaceId>,
        then: Then,
    ) {
        let asked = match self.worktree_asked(project, branch, on) {
            Ok(asked) => asked,
            Err(error) => return then.refuse(error),
        };
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let planned = Planned {
                plan: plan(asked).await,
                then,
            };
            let _ = requests
                .send(Request::WorktreePlanned(Box::new(planned)))
                .await;
        });
    }

    /// A plan came back: made, unless the branch or the folder was taken
    /// meanwhile.
    pub(in crate::supervisor) fn worktree_planned(&mut self, planned: Planned) {
        let Planned { plan, then } = planned;
        let plan = plan.and_then(|plan| {
            if self.worktree_on(plan.project, &plan.branch).is_some() {
                return Err(ErrorBody::new(
                    ErrorCode::Conflict,
                    format!("{} already has a worktree", plan.branch),
                ));
            }
            if self.worktree_jobs.making_branch(plan.project, &plan.branch)
                || self.worktree_jobs.is_making(&plan.path)
            {
                return Err(ErrorBody::new(
                    ErrorCode::Conflict,
                    format!("a worktree for {} is being made already", plan.branch),
                ));
            }
            Ok(plan)
        });
        match plan {
            Ok(plan) => {
                let ticket = match &then {
                    Then::Ticket(ticket) => Some(ticket.key().clone()),
                    Then::Reply(_) => None,
                };
                self.make_worktree(Plan { ticket, ..plan }, then);
            }
            Err(error) => then.refuse(error),
        }
    }

    fn worktree_asked(
        &self,
        project: ProjectId,
        branch: Branch,
        on: Option<WorkspaceId>,
    ) -> Result<Asked, ErrorBody> {
        self.repository_root(project, "worktrees")?;
        let project = self
            .state
            .projects
            .iter()
            .find(|candidate| candidate.id == project)
            .cloned()
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no project {project}")))?;
        let groups = |of: &dyn Fn(ProjectId) -> bool| -> Vec<PathBuf> {
            self.state
                .workspaces
                .iter()
                .filter(|workspace| workspace.kind.is_worktree() && of(workspace.project))
                .filter_map(|workspace| workspace.kind.host_path()?.parent())
                .chain(
                    self.worktree_jobs
                        .making_of(of)
                        .filter_map(|path| path.parent()),
                )
                .map(Path::to_path_buf)
                .collect()
        };
        let taken = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project.id)
            .map(|workspace| workspace.kind.branch().to_owned())
            .chain(self.worktree_jobs.making_branches(project.id))
            .collect();
        let on = match on {
            Some(on) => Some(self.stack_parent(project.id, on)?),
            None => None,
        };
        Ok(Asked {
            overrides: self.overrides_of(project.id),
            branch,
            taken,
            dir: self.worktrees_dir.clone(),
            own_groups: groups(&|of| of == project.id),
            other_groups: groups(&|of| of != project.id),
            making: self
                .worktree_jobs
                .making_paths()
                .map(Path::to_path_buf)
                .collect(),
            shared_builds: self.state.preferences.shared_builds,
            on,
            project,
        })
    }
}

/// The plan for `asked`, asked of git and the file system.
async fn plan(asked: Asked) -> Result<Plan, ErrorBody> {
    let Asked {
        project, overrides, ..
    } = &asked;
    let prefixed = || crate::config::muster_prefix(project.root.clone(), overrides.clone());
    let branch = match &asked.branch {
        Branch::Named(name) => name.clone(),
        Branch::For(words) => {
            let prefixed = prefixed().await;
            match muster_core::delegation::prompt_branch(words, prefixed) {
                Some(name) => unused(&asked, &name, 1).await?,
                None => default_branch(prefixed),
            }
        }
        Branch::Another(name) => unused(&asked, name, 2).await?,
        Branch::Any => default_branch(prefixed().await),
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
    let group = group_for(&asked).await?;
    if asked.shared_builds
        && let Err(error) = crate::shared_builds::share(&group).await
    {
        tracing::warn!(
            %error,
            folder = %group.display(),
            "could not share the builds of a project's worktrees"
        );
    }
    let path = unused_path(&group, &slug(&branch), &asked.making).await;
    let exists = git::branch_exists(&project.root, &branch)
        .await
        .map_err(git_failure)?;
    let stacked_on = match &asked.on {
        Some((path, parent)) => Some(stacked_at(path, parent, &branch, exists).await?),
        None => None,
    };
    Ok(Plan {
        project: project.id,
        root: project.root.clone(),
        overrides: overrides.clone(),
        path,
        branch,
        exists,
        stacked_on,
        ticket: None,
    })
}

/// `name` from the `from`th (`name` itself the first, then `name-2`,
/// `-3`…): the first no workspace has and the repository does not have
/// yet.
async fn unused(asked: &Asked, name: &str, from: u32) -> Result<String, ErrorBody> {
    for n in from..100 {
        let candidate = if n == 1 {
            name.to_owned()
        } else {
            format!("{name}-{n}")
        };
        if !asked.taken.contains(&candidate)
            && !git::branch_exists(&asked.project.root, &candidate)
                .await
                .map_err(git_failure)?
        {
            return Ok(candidate);
        }
    }
    Err(ErrorBody::new(
        ErrorCode::Conflict,
        format!("{name} already has 99 worktrees"),
    ))
}

/// What a new branch `branch` is stacked on when made on top of `parent`,
/// checked out at `path`: that branch, at its newest commit.
async fn stacked_at(
    path: &Path,
    parent: &str,
    branch: &str,
    exists: bool,
) -> Result<Stacked, ErrorBody> {
    if exists {
        return Err(ErrorBody::new(
            ErrorCode::Conflict,
            format!("{branch} exists already, so it cannot be made on top of {parent}"),
        ));
    }
    let at = git::checked(
        path,
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("refs/heads/{parent}^{{commit}}"),
        ],
    )
    .await
    .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?;
    Ok(Stacked {
        branch: parent.to_owned(),
        at,
    })
}

/// The folder of the worktrees directory the project's worktrees go in,
/// made, by its real path, as git lists worktrees and the sync finds them:
/// the one they are in already, else one named for the project's folder,
/// unless another project's worktrees are there (two repositories both
/// called `app`), whose builds it would share, when its id's end is added.
async fn group_for(asked: &Asked) -> Result<PathBuf, ErrorBody> {
    let made = |folder: PathBuf| async move {
        tokio::fs::create_dir_all(&folder).await.map_err(|error| {
            ErrorBody::new(
                ErrorCode::Internal,
                format!("cannot create {}: {error}", folder.display()),
            )
        })?;
        Ok::<_, ErrorBody>(tokio::fs::canonicalize(&folder).await.unwrap_or(folder))
    };
    let dir = made(asked.dir.clone()).await?;
    let within = |groups: &[PathBuf]| -> Vec<PathBuf> {
        groups
            .iter()
            .filter(|group| group.parent() == Some(dir.as_path()))
            .cloned()
            .collect()
    };
    if let Some(own) = within(&asked.own_groups).into_iter().next() {
        return made(own).await;
    }
    let project = &asked.project;
    let name = slug(&project.root.file_name().map_or_else(
        || project.name.clone(),
        |name| name.to_string_lossy().into_owned(),
    ));
    let plain = dir.join(&name);
    if !within(&asked.other_groups).contains(&plain) {
        return made(plain).await;
    }
    let id = project.id.ulid().to_string().to_lowercase();
    made(dir.join(format!("{name}-{}", &id[id.len() - 6..]))).await
}

/// `group/name`, or `group/name-2`, `-3`… if that is taken on disk or by
/// a worktree being made.
async fn unused_path(group: &Path, name: &str, making: &[PathBuf]) -> PathBuf {
    let mut path = group.join(name);
    let mut suffix = 2;
    while making.contains(&path) || tokio::fs::try_exists(&path).await.unwrap_or(true) {
        path = group.join(format!("{name}-{suffix}"));
        suffix += 1;
    }
    path
}
