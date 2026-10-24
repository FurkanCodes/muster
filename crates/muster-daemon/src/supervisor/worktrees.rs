//! Creating and removing git worktrees (`ARCHITECTURE.md` §5).
//!
//! Worktrees go under the worktrees directory, grouped by project. A new
//! branch starts from the newest of the branch it will merge into, here or
//! on the remote, never from whatever the project's checkout has checked
//! out. Removing one deletes its directory but keeps its branch: the work
//! on it is the person's.
//!
//! What the Supervisor's loop knows is checked on it; git's own work,
//! which takes seconds in a large repository, runs off it (`ARC-13`): the
//! plan, which comes back as [`Planned`] (`planning`), and the making or
//! removing, which come back as [`Made`] or [`Removed`]. Meanwhile [`Jobs`] holds the branch and the
//! folder, so nothing else takes them, the sync neither adopts nor forgets
//! them, and nothing starts in a worktree going.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::stacks::Stacked;
use muster_core::{ProjectId, Workspace, WorkspaceId, WorkspaceKind};
use tokio::sync::oneshot;

use super::{Request, Supervisor, by_hand, issues, store_closed, store_error};
use crate::git::{self, GitError};

/// Worktrees git is making or removing off the loop.
#[derive(Debug, Default)]
pub(super) struct Jobs {
    making: Vec<Making>,
    removing: HashSet<WorkspaceId>,
}

/// A worktree being made: its project, branch and folder.
#[derive(Debug)]
struct Making {
    project: ProjectId,
    branch: String,
    path: PathBuf,
}

impl Jobs {
    fn making_branch(&self, project: ProjectId, branch: &str) -> bool {
        self.making
            .iter()
            .any(|making| making.project == project && making.branch == branch)
    }

    /// The branches `project`'s worktrees are being made on.
    fn making_branches(&self, project: ProjectId) -> impl Iterator<Item = String> + '_ {
        self.making
            .iter()
            .filter(move |making| making.project == project)
            .map(|making| making.branch.clone())
    }

    /// How many of `project`'s worktrees are being made.
    pub(super) fn making_in(&self, project: ProjectId) -> usize {
        self.making
            .iter()
            .filter(|making| making.project == project)
            .count()
    }

    /// Whether a worktree is being made at `path`.
    pub(super) fn is_making(&self, path: &Path) -> bool {
        self.making.iter().any(|making| making.path == path)
    }

    /// The folders worktrees are being made in for the projects `of` takes.
    fn making_of<'a>(
        &'a self,
        of: &'a dyn Fn(ProjectId) -> bool,
    ) -> impl Iterator<Item = &'a Path> {
        self.making
            .iter()
            .filter(move |making| of(making.project))
            .map(|making| making.path.as_path())
    }

    /// The folders worktrees are being made in.
    pub(super) fn making_paths(&self) -> impl Iterator<Item = &Path> {
        self.making.iter().map(|making| making.path.as_path())
    }

    /// Whether `workspace`'s worktree is being removed.
    pub(super) fn is_removing(&self, workspace: WorkspaceId) -> bool {
        self.removing.contains(&workspace)
    }
}

/// What follows a worktree made off the loop.
pub(super) enum Then {
    /// Answer the command that asked for it.
    Reply(oneshot::Sender<Result<Reply, ErrorBody>>),
    /// Start a ticket's agent in it.
    Ticket(Box<issues::TicketThen>),
}

impl Then {
    fn refuse(self, error: ErrorBody) {
        match self {
            Self::Reply(reply) => {
                tracing::info!(command = "create worktree", %error, "command refused");
                let _ = reply.send(Err(error));
            }
            Self::Ticket(ticket) => ticket.refuse(error),
        }
    }
}

/// A worktree to make, as the checks in the loop left it.
#[derive(Debug)]
struct Plan {
    project: ProjectId,
    root: PathBuf,
    /// The project's own settings, for the base its branch starts from.
    overrides: PathBuf,
    path: PathBuf,
    branch: String,
    /// Whether the branch exists already, to be checked out as it is.
    exists: bool,
    stacked_on: Option<Stacked>,
    /// The ticket it is made for, if any.
    ticket: Option<muster_core::TicketKey>,
}

/// A worktree git made off the loop, or could not, on its way back.
pub(super) struct Made {
    plan: Plan,
    /// Its folder by its real path.
    outcome: Result<PathBuf, ErrorBody>,
    then: Then,
}

/// A worktree git removed off the loop, or could not, on its way back.
pub(super) struct Removed {
    workspace: WorkspaceId,
    outcome: Result<(), ErrorBody>,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

/// The branch a worktree is asked for.
#[derive(Debug)]
pub(super) enum Branch {
    /// This one: the branch if it exists, else a new one of the name.
    Named(String),
    /// A new branch named from a prompt's words, never one that exists.
    For(String),
    /// A new branch after this one, which is taken: `name-2`, `-3`…
    Another(String),
    /// A new branch of Muster's own name.
    Any,
}

impl Branch {
    /// The branch `CreateWorktree` asks for: its `branch`, else one named
    /// for `named_for`, else any.
    pub(super) fn new(branch: Option<String>, named_for: Option<String>) -> Self {
        match (branch.map(|name| name.trim().to_owned()), named_for) {
            (Some(name), _) if !name.is_empty() => Self::Named(name),
            (_, Some(words)) => Self::For(words),
            _ => Self::Any,
        }
    }
}

impl Supervisor {
    fn make_worktree(&mut self, plan: Plan, then: Then) {
        self.worktree_jobs.making.push(Making {
            project: plan.project,
            branch: plan.branch.clone(),
            path: plan.path.clone(),
        });
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let outcome = add(&plan).await;
            let made = Made {
                plan,
                outcome,
                then,
            };
            let _ = requests.send(Request::WorktreeMade(Box::new(made))).await;
        });
    }

    /// A worktree git made, or could not: kept, told and followed.
    pub(super) async fn worktree_made(&mut self, made: Made) {
        let Made {
            plan,
            outcome,
            then,
        } = made;
        self.worktree_jobs
            .making
            .retain(|making| making.path != plan.path);
        let path = match outcome {
            Ok(path) => path,
            Err(error) => return then.refuse(error),
        };
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: plan.project,
            kind: WorkspaceKind::Worktree {
                path: path.clone(),
                branch: plan.branch,
            },
            created_at: self.clock.now(),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: plan.stacked_on,
            ticket: plan.ticket,
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
            let root = plan.root;
            self.tasks.spawn(async move {
                if let Err(error) = git::remove_worktree(&root, &path, true).await {
                    tracing::warn!(%error, "could not remove a worktree the store refused");
                }
            });
            return then.refuse(error);
        }

        self.state.workspaces.push(workspace.clone());
        let id = workspace.id;
        self.broadcast(Delta::WorkspaceUpserted(workspace));
        self.worktree_created(id);
        match then {
            Then::Reply(reply) => {
                let _ = reply.send(Ok(Reply::WorktreeCreated { id }));
            }
            Then::Ticket(ticket) => self.start_ticket_agent(id, *ticket).await,
        }
    }

    pub(super) async fn remove_worktree(
        &mut self,
        workspace: WorkspaceId,
        force: bool,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let (root, path) = match self.removal(workspace).await {
            Ok(found) => found,
            Err(error) => {
                tracing::info!(command = "remove worktree", %error, "command refused");
                let _ = reply.send(Err(error));
                return;
            }
        };
        self.worktree_jobs.removing.insert(workspace);
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let outcome = remove(&root, &path, force).await;
            let removed = Removed {
                workspace,
                outcome,
                reply,
            };
            let _ = requests
                .send(Request::WorktreeRemoved(Box::new(removed)))
                .await;
        });
    }

    /// Where `workspace`'s worktree and its repository are, once what
    /// ends with it has stopped; refused where it is no worktree, an agent
    /// works there, or it is going already.
    async fn removal(&mut self, workspace: WorkspaceId) -> Result<(PathBuf, PathBuf), ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .cloned()
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no worktree {workspace}"))
            })?;
        let path = match found.kind {
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
        if self.worktree_jobs.is_removing(workspace) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is being removed already", path.display()),
            ));
        }
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
        Ok((root, path))
    }

    /// A worktree git removed, or could not: forgotten and answered.
    pub(super) async fn worktree_removed(&mut self, removed: Removed) {
        let Removed {
            workspace,
            outcome,
            reply,
        } = removed;
        self.worktree_jobs.removing.remove(&workspace);
        let outcome = match outcome {
            Ok(()) => self.forget_removed(workspace).await,
            Err(error) => Err(error),
        };
        if let Err(error) = &outcome {
            tracing::info!(command = "remove worktree", %error, "command refused");
        }
        let _ = reply.send(outcome);
    }

    async fn forget_removed(&mut self, workspace: WorkspaceId) -> Result<Reply, ErrorBody> {
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

/// Git's work for `plan`, off the loop: the worktree's folder by its real
/// path.
async fn add(plan: &Plan) -> Result<PathBuf, ErrorBody> {
    let start = match (&plan.stacked_on, plan.exists) {
        (Some(stacked), _) => Some(stacked.at.clone()),
        // An existing branch is checked out as it is.
        (None, true) => None,
        (None, false) => {
            let configured = match by_hand::load_settings(plan.root.clone(), plan.overrides.clone())
                .await
            {
                Ok(settings) => settings.base_branch,
                Err(error) => {
                    tracing::warn!(%error, "a new worktree starts from the remote's default branch");
                    None
                }
            };
            start_point(&plan.root, configured.as_deref()).await
        }
    };
    git::add_worktree(
        &plan.root,
        &plan.path,
        &plan.branch,
        !plan.exists,
        start.as_deref(),
    )
    .await
    .map_err(|error| branch_clash(&error, &plan.branch).unwrap_or_else(|| git_failure(error)))?;
    Ok(tokio::fs::canonicalize(&plan.path)
        .await
        .unwrap_or_else(|_| plan.path.clone()))
}

/// Git's work removing the worktree at `path` of the repository at
/// `root`, off the loop.
async fn remove(root: &Path, path: &Path, force: bool) -> Result<(), ErrorBody> {
    let removed = if tokio::fs::try_exists(path).await.unwrap_or(true) {
        git::remove_worktree(root, path, force).await
    } else {
        // Its directory was deleted by hand; git only needs to forget it.
        git::prune_worktrees(root).await
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
    })
}

/// The commit a new branch of the repository at `root` starts from: the
/// newest of the branch it will merge into (`configured`, or the remote's
/// default), as the repository has it and as the remote last said. Where
/// the two have parted, the repository's own, which may hold the person's
/// work not pushed yet. `None`, for git's current commit, where neither is
/// found.
pub(super) async fn start_point(root: &Path, configured: Option<&str>) -> Option<String> {
    let base = by_hand::base_of(root, configured).await;
    let name = base.strip_prefix("origin/").unwrap_or(&base);
    let here = commit(root, &format!("refs/heads/{name}")).await;
    let remote = commit(root, &format!("refs/remotes/origin/{name}")).await;
    match (here, remote) {
        (Some(here), Some(remote)) => {
            let behind = here != remote
                && git::answer(
                    root,
                    &[
                        "merge-base",
                        "--is-ancestor",
                        "--end-of-options",
                        &here,
                        &remote,
                    ],
                )
                .await
                .ok()
                .flatten()
                .is_some();
            Some(if behind { remote } else { here })
        }
        (here, remote) => here.or(remote),
    }
}

/// The commit `reference` names in the repository at `root`, if it names one.
async fn commit(root: &Path, reference: &str) -> Option<String> {
    git::answer(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            "--end-of-options",
            &format!("{reference}^{{commit}}"),
        ],
    )
    .await
    .ok()
    .flatten()
    .filter(|commit| !commit.is_empty())
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

#[path = "worktree_planning.rs"]
mod planning;
pub(super) use planning::Planned;

#[cfg(test)]
#[path = "worktrees_tests.rs"]
mod tests;
