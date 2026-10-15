//! Projects that share their tickets: several repositories whose tickets
//! are in one Jira project (or one Linear team), with nothing in a ticket
//! to say which repository it is for. They are listed once, under one
//! heading; the agents on a ticket are those in any of them; and an agent
//! assigned to it starts in the one the person chooses, the one last
//! chosen offered first.
//!
//! The daemon says where each project's tickets come from
//! (`Reply::Issues::source`); projects with the same source share them.
//! A repository's own issues (GitHub's, GitLab's) are never shared, and
//! a project's tickets are its alone until they are fetched.

use muster_core::{ProjectId, Session, TicketKey, WorkspaceId};

use super::{App, Tickets};

impl App {
    /// Where a project's tickets come from, if they can be shared.
    fn source_of(&self, project: ProjectId) -> Option<&str> {
        match self.tickets.get(&project)? {
            Tickets::Loaded {
                source: Some(source),
                ..
            } => Some(source),
            _ => None,
        }
    }

    /// The projects that list the same tickets as `project`, it among
    /// them, in the list's order; only `project` while its are its own.
    pub(crate) fn sharing(&self, project: ProjectId) -> Vec<ProjectId> {
        let Some(source) = self.source_of(project) else {
            return vec![project];
        };
        self.model
            .projects()
            .iter()
            .map(|found| found.id)
            .filter(|other| self.source_of(*other) == Some(source))
            .collect()
    }

    /// The first of the projects sharing `project`'s tickets, whose
    /// heading and rows stand for them all.
    pub(crate) fn lead(&self, project: ProjectId) -> ProjectId {
        self.sharing(project).first().copied().unwrap_or(project)
    }

    /// The agents on a ticket, in whichever of the projects sharing it
    /// they work, oldest first.
    pub(crate) fn ticket_agents(&self, project: ProjectId, key: &TicketKey) -> Vec<&Session> {
        self.model.agents_on(&self.sharing(project), key)
    }

    /// The worktrees for a ticket, in whichever of the projects sharing
    /// it they are.
    pub(crate) fn ticket_worktrees(&self, project: ProjectId, key: &TicketKey) -> Vec<WorkspaceId> {
        self.model.worktrees_for(&self.sharing(project), key)
    }

    /// The project to offer first for an agent on one of `project`'s
    /// tickets: the one last chosen for tickets from there, else the
    /// first.
    pub(super) fn project_to_offer(&self, project: ProjectId) -> ProjectId {
        let sharing = self.sharing(project);
        self.source_of(project)
            .and_then(|source| self.ticket_projects.get(source))
            .copied()
            .filter(|chosen| sharing.contains(chosen))
            .unwrap_or_else(|| self.lead(project))
    }

    /// Remembers the project chosen for an agent on one of `project`'s
    /// tickets, to offer it first next time.
    pub(super) fn remember_project(&mut self, project: ProjectId, chosen: ProjectId) {
        if let Some(source) = self.source_of(project).map(str::to_owned) {
            self.ticket_projects.insert(source, chosen);
        }
    }

    /// The project after (or with `back`, before) `chosen` among those
    /// sharing `project`'s tickets, round the end.
    pub(super) fn next_project(
        &self,
        project: ProjectId,
        chosen: ProjectId,
        back: bool,
    ) -> ProjectId {
        let sharing = self.sharing(project);
        let at = sharing
            .iter()
            .position(|found| *found == chosen)
            .unwrap_or(0);
        let next = if back {
            (at + sharing.len() - 1) % sharing.len()
        } else {
            (at + 1) % sharing.len()
        };
        sharing.get(next).copied().unwrap_or(chosen)
    }
}
