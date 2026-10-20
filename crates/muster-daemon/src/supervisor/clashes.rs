//! Where the work in a project's workspaces meets (`PRODUCT.md` §6), kept
//! on each workspace (`Workspace::clashes`, never stored). It is looked at
//! again, off the loop under git's deadline (`ARC-13`), when a look at a
//! workspace's changes finds them moved or follows an agent's turn there,
//! when a branch changes under a workspace or its pull request merges, and
//! on every tenth pull request tick. Only workspaces whose work changes
//! something are compared, and not one whose pull request has merged: its
//! work is in its base. One look at a time for each project; one asked
//! while one is under way follows it.

use std::collections::HashMap;

use muster_core::clashes::Clash;
use muster_core::protocol::Delta;
use muster_core::{ProjectId, PullRequestState, Workspace, WorkspaceId};

use super::changes::{Place, base};
use super::{Request, Supervisor};
use crate::clashes::{self, Side};

/// One tick in this many looks at every project.
const EVERY: u32 = 10;

/// A look still not back after this many ticks is taken as lost.
const LOST_AFTER: u32 = 10;

/// The most workspaces of a project compared, the newest first.
const MAX_SIDES: usize = 12;

/// The Supervisor's part: each project's look under way.
#[derive(Debug, Default)]
pub(super) struct Clashes {
    /// The tick each began on, and whether another is wanted once it is
    /// back (`ARC-14`: an entry goes with its project).
    under_way: HashMap<ProjectId, (u32, bool)>,
    ticks: u32,
}

/// What a look found: each workspace compared, the branch it was on, and
/// its clashes; `None` where git could not tell.
#[derive(Debug)]
pub(super) struct Found {
    project: ProjectId,
    found: Option<Vec<(WorkspaceId, String, Vec<Clash>)>>,
}

/// Whether a workspace's work is compared: it changes something, and
/// its pull request has not merged.
fn has_work(workspace: &Workspace) -> bool {
    workspace.changes.is_some()
        && !workspace
            .pull_request
            .as_ref()
            .is_some_and(|found| found.state == PullRequestState::Merged)
}

impl Supervisor {
    /// Looks at where the work in `project`'s workspaces meets, off the
    /// loop, unless a look is under way, which another then follows.
    pub(super) fn check_clashes(&mut self, project: ProjectId) {
        if let Some((_, again)) = self.clashes.under_way.get_mut(&project) {
            *again = true;
            return;
        }
        let keep: Vec<WorkspaceId> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project)
            .map(|workspace| workspace.id)
            .collect();
        let mut compared: Vec<&Workspace> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project && has_work(workspace))
            .collect();
        compared.sort_by_key(|workspace| std::cmp::Reverse(workspace.created_at));
        let places: Vec<Place> = compared
            .iter()
            .filter_map(|workspace| self.changes_place(workspace.id).ok())
            .take(MAX_SIDES)
            .collect();
        if places.len() < 2 {
            self.settle_clashes(project, &[]);
            return;
        }
        self.clashes
            .under_way
            .insert(project, (self.clashes.ticks, false));
        let dir = clashes::dir(&self.data_dir, project);
        let at = self.clock.now();
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let mut sides = Vec::new();
            for place in &places {
                sides.push(Side {
                    workspace: place.workspace,
                    path: place.path.clone(),
                    base: base(place).await,
                });
            }
            let found = match clashes::check(&dir, &sides, &keep, at).await {
                Ok(found) => Some(
                    found
                        .into_iter()
                        .filter_map(|(workspace, clashes)| {
                            let place = places.iter().find(|place| place.workspace == workspace)?;
                            Some((workspace, place.branch.clone(), clashes))
                        })
                        .collect(),
                ),
                Err(error) => {
                    tracing::debug!(%error, "could not tell where workspaces' work meets");
                    None
                }
            };
            let _ = requests
                .send(Request::ClashesFound(Box::new(Found { project, found })))
                .await;
        });
    }

    /// A look came back: what it found is kept, and the next follows if
    /// one was asked meanwhile.
    pub(super) fn clashes_found(&mut self, found: Found) {
        let again = self
            .clashes
            .under_way
            .remove(&found.project)
            .is_some_and(|(_, again)| again);
        if !self
            .state
            .projects
            .iter()
            .any(|project| project.id == found.project)
        {
            // It went while git looked, and the look may have made its
            // store again.
            self.forget_clashes_of(found.project);
            return;
        }
        if let Some(clashes) = found.found {
            self.settle_clashes(found.project, &clashes);
        }
        if again {
            self.check_clashes(found.project);
        }
    }

    /// Keeps on each of `project`'s workspaces the clashes `found` for it,
    /// with workspaces still on the branches they were found on; none on
    /// a workspace not found. Tells clients of each change.
    fn settle_clashes(&mut self, project: ProjectId, found: &[(WorkspaceId, String, Vec<Clash>)]) {
        let before: HashMap<WorkspaceId, Vec<Clash>> = self
            .state
            .workspaces
            .iter()
            .filter(|workspace| workspace.project == project)
            .map(|workspace| (workspace.id, workspace.clashes.clone()))
            .collect();
        let branches: HashMap<WorkspaceId, String> = self
            .state
            .workspaces
            .iter()
            .map(|workspace| (workspace.id, workspace.kind.branch().to_owned()))
            .collect();
        let still_on = |workspace: WorkspaceId, branch: &str| {
            branches.get(&workspace).is_some_and(|now| *now == branch)
        };
        let found_on: HashMap<WorkspaceId, &str> = found
            .iter()
            .map(|(workspace, branch, _)| (*workspace, branch.as_str()))
            .collect();
        let mut changed = Vec::new();
        for workspace in self
            .state
            .workspaces
            .iter_mut()
            .filter(|workspace| workspace.project == project)
        {
            let clashes: Vec<Clash> = found
                .iter()
                .find(|(id, branch, _)| *id == workspace.id && still_on(*id, branch))
                .map(|(_, _, clashes)| {
                    clashes
                        .iter()
                        .filter(|clash| {
                            found_on
                                .get(&clash.with)
                                .is_some_and(|branch| still_on(clash.with, branch))
                        })
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if workspace.clashes != clashes {
                workspace.clashes = clashes;
                changed.push(workspace.clone());
            }
        }
        for workspace in changed {
            self.broadcast(Delta::WorkspaceUpserted(workspace));
        }
        self.clashes_looked(project, &before);
    }

    /// The pull request timer's tick: every tenth, a look at every
    /// project; a look gone too long is taken as lost.
    pub(super) fn tick_clashes(&mut self) {
        self.clashes.ticks = self.clashes.ticks.wrapping_add(1);
        let ticks = self.clashes.ticks;
        let lost: Vec<ProjectId> = self
            .clashes
            .under_way
            .iter()
            .filter(|(_, (began, _))| ticks.wrapping_sub(*began) >= LOST_AFTER)
            .map(|(project, _)| *project)
            .collect();
        for project in lost {
            tracing::warn!(%project, "a look at where workspaces' work meets never came back");
            self.clashes.under_way.remove(&project);
        }
        if ticks.is_multiple_of(EVERY) {
            let projects: Vec<ProjectId> = self
                .state
                .projects
                .iter()
                .map(|project| project.id)
                .collect();
            for project in projects {
                self.check_clashes(project);
            }
        }
    }

    /// A workspace went: no other's clashes name it. Its snapshot goes
    /// with the project's next look.
    pub(super) fn forget_clashes_with(&mut self, workspace: WorkspaceId) {
        let mut changed = Vec::new();
        for other in &mut self.state.workspaces {
            let before = other.clashes.len();
            other.clashes.retain(|clash| clash.with != workspace);
            if other.clashes.len() != before {
                changed.push(other.clone());
            }
        }
        for other in changed {
            self.broadcast(Delta::WorkspaceUpserted(other));
        }
    }

    /// A project went: its store of snapshots goes with it (`ARC-14`).
    pub(super) fn forget_clashes_of(&mut self, project: ProjectId) {
        self.clashes.under_way.remove(&project);
        let dir = clashes::dir(&self.data_dir, project);
        self.tasks.spawn(async move { clashes::remove(&dir).await });
    }
}
