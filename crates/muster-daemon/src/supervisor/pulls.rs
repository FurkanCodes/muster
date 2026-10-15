//! Where each workspace's branch's pull request stands (`PRODUCT.md` §6):
//! looked at on the pull request timer, off the loop, through each
//! project's git host (`ARC-03`), and kept on the workspace for clients,
//! never stored. Only read: nothing is changed on the host (`PRD-01`).
//!
//! A look that fails keeps what the last one found, as a network that
//! comes and goes should not blank what the person sees. While no client
//! is subscribed, only every tenth tick looks, so a closed UI costs the
//! git host little and an opened one starts from something recent.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use muster_core::protocol::Delta;
use muster_core::{ProjectId, PullRequestStatus, WorkspaceId};

use super::{Request, Supervisor, by_hand};
use crate::githost::choose::Hosts;
use crate::service::ProjectPlace;

/// While nobody watches, one tick in this many looks.
const UNWATCHED_EVERY: u32 = 10;

/// A look still not back after this many ticks is taken as lost, so one
/// that never came back cannot stop every look after it.
const LOST_AFTER: u32 = 10;

/// The Supervisor's part: whether a look is under way, and what the last
/// ones could not do.
#[derive(Debug, Default)]
pub(super) struct Looks {
    /// The tick a look at every branch started on, until it is back.
    under_way: Option<u32>,
    ticks: u32,
    /// Each project's last failure, logged once until it changes.
    failures: HashMap<ProjectId, String>,
}

/// One project's branches to look at.
#[derive(Debug)]
struct ProjectBranches {
    project: ProjectId,
    place: ProjectPlace,
    branches: Vec<Branch>,
}

#[derive(Debug)]
struct Branch {
    workspace: WorkspaceId,
    name: String,
    path: PathBuf,
}

/// What a look found.
#[derive(Debug, Default)]
pub(super) struct Looked {
    /// Every workspace's, not one's.
    whole: bool,
    /// Each branch looked at, with its pull request or none.
    found: Vec<(WorkspaceId, String, Option<PullRequestStatus>)>,
    /// The projects whose host answered, and those whose did not, with why.
    answered: Vec<ProjectId>,
    failed: Vec<(ProjectId, String)>,
}

impl Supervisor {
    /// The pull request timer's tick: a look at every branch's, unless one
    /// is under way, or nobody watches and it is not that tick's turn.
    pub(super) fn tick_pull_requests(&mut self) {
        self.pulls.ticks = self.pulls.ticks.wrapping_add(1);
        let watched = self.deltas.receiver_count() > 0;
        let waiting = self
            .pulls
            .under_way
            .is_some_and(|started| self.pulls.ticks.wrapping_sub(started) < LOST_AFTER);
        if waiting || !(watched || self.pulls.ticks.is_multiple_of(UNWATCHED_EVERY)) {
            return;
        }
        self.look_at_pull_requests(None);
    }

    /// Looks at the pull request of `only` that workspace's branch, or of
    /// every workspace's, off the loop.
    pub(super) fn look_at_pull_requests(&mut self, only: Option<WorkspaceId>) {
        let projects = self.branches_to_look_at(only);
        if projects.is_empty() {
            return;
        }
        let whole = only.is_none();
        if whole {
            self.pulls.under_way = Some(self.pulls.ticks);
        }
        let (requests, hosts) = (self.requests.clone(), self.hosts.clone());
        self.tasks.spawn(async move {
            let mut looked = look(hosts, projects).await;
            looked.whole = whole;
            let _ = requests
                .send(Request::PullRequestsLooked(Box::new(looked)))
                .await;
        });
    }

    /// Keeps what a look found on each workspace still on the branch it
    /// was looked at for, and tells clients of each change.
    pub(super) fn pull_requests_looked(&mut self, looked: Looked) {
        if looked.whole {
            self.pulls.under_way = None;
        }
        for project in looked.answered {
            self.pulls.failures.remove(&project);
        }
        for (project, error) in looked.failed {
            if self.pulls.failures.get(&project) != Some(&error) {
                tracing::info!(%project, %error, "could not look at pull requests");
                self.pulls.failures.insert(project, error);
            }
        }
        for (workspace, branch, found) in looked.found {
            let Some(stored) = self
                .state
                .workspaces
                .iter_mut()
                .find(|candidate| candidate.id == workspace)
            else {
                continue;
            };
            if stored.kind.branch() != branch || stored.pull_request == found {
                continue;
            }
            stored.pull_request = found;
            let updated = stored.clone();
            self.broadcast(Delta::WorkspaceUpserted(updated));
        }
    }

    /// The branches whose pull requests can be looked at, by project:
    /// worktrees' and checkouts' in git, with a branch checked out.
    fn branches_to_look_at(&self, only: Option<WorkspaceId>) -> Vec<ProjectBranches> {
        let mut projects: Vec<ProjectBranches> = Vec::new();
        for workspace in &self.state.workspaces {
            if only.is_some_and(|only| only != workspace.id) || workspace.kind.run().is_some() {
                continue;
            }
            let name = workspace.kind.branch();
            let Some(path) = workspace.kind.host_path() else {
                continue;
            };
            if !workspace.kind.in_git() || name.starts_with("detached at ") || name == "?" {
                continue;
            }
            let at = if let Some(at) = projects
                .iter()
                .position(|project| project.project == workspace.project)
            {
                at
            } else {
                let Some(root) = self.project_root(workspace.project) else {
                    continue;
                };
                let overrides = self
                    .config_dir
                    .join("projects")
                    .join(format!("{}.toml", workspace.project));
                projects.push(ProjectBranches {
                    project: workspace.project,
                    place: ProjectPlace { root, overrides },
                    branches: Vec::new(),
                });
                projects.len() - 1
            };
            projects[at].branches.push(Branch {
                workspace: workspace.id,
                name: name.to_owned(),
                path: path.to_owned(),
            });
        }
        projects
    }
}

/// Asks each project's git host for its branches' pull requests, one after
/// another. The branch pull requests merge into has none of its own to
/// show: on a host, other people's forks may well have one from it.
async fn look(hosts: Arc<dyn Hosts>, projects: Vec<ProjectBranches>) -> Looked {
    let mut looked = Looked::default();
    for ProjectBranches {
        project,
        place,
        branches,
    } in projects
    {
        let host = match hosts.for_project(place.clone()).await {
            Ok(host) => host,
            Err(error) => {
                looked.failed.push((project, error.to_string()));
                continue;
            }
        };
        let configured = by_hand::load_settings(place.root.clone(), place.overrides)
            .await
            .ok()
            .and_then(|config| config.base_branch);
        let base = by_hand::base_of(&place.root, configured.as_deref()).await;
        let base = base.strip_prefix("origin/").unwrap_or(&base);
        let mut failure = None;
        for branch in branches {
            if branch.name == base {
                looked.found.push((branch.workspace, branch.name, None));
                continue;
            }
            match host.pull_request_of(&branch.path, &branch.name).await {
                Ok(found) => looked.found.push((branch.workspace, branch.name, found)),
                Err(error) => failure = Some(error.to_string()),
            }
        }
        match failure {
            Some(error) => looked.failed.push((project, error)),
            None => looked.answered.push(project),
        }
    }
    looked
}
