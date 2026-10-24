//! What happens, told to plugins' hooks (`PRD-15`, `ARCHITECTURE.md`
//! §7a): the Supervisor raises each event where it sees the change, once
//! per change, never for a shell or a plugin's panel; each hook of a
//! plugin that runs takes it or not (`Hook::takes`), the person away from
//! every Muster window or not; the runner runs those that take it
//! (`plugins::hooks`). A finish is held a moment first, as the person is
//! told of one; the first look at pull requests and clashes after the
//! daemon starts is what is, no news.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use muster_core::clashes::Clash;
use muster_core::plugins::{Event, EventKind};
use muster_core::{ProjectId, PullRequestStatus, SessionId, SessionStatus, WorkspaceId};

use super::Supervisor;
use crate::plugins::hooks::Hooks;

/// Plugins' hooks, and what raising events keeps between changes.
#[derive(Debug)]
pub(super) struct Events {
    /// Runs the hooks.
    hooks: Hooks,
    /// Finishes held, each until when.
    held: Vec<(SessionId, Instant)>,
    /// Workspaces whose pull request was looked at since the daemon
    /// started.
    pulls: HashSet<WorkspaceId>,
    /// Projects whose clashes were looked at since the daemon started.
    clashes: HashSet<ProjectId>,
    /// The session whose program is ending as the person asked, which is
    /// no error however it ends.
    stopped: Option<SessionId>,
}

impl Events {
    pub(super) fn new(hooks: Hooks) -> Self {
        Self {
            hooks,
            held: Vec::new(),
            pulls: HashSet::new(),
            clashes: HashSet::new(),
            stopped: None,
        }
    }

    /// `session`'s program ended, as the person asked or not: its status
    /// comes next.
    pub(super) fn ending(&mut self, session: SessionId, requested: bool) {
        self.stopped = requested.then_some(session);
    }

    /// The hooks' runner.
    pub(super) fn hooks(&self) -> &Hooks {
        &self.hooks
    }

    /// When the first finish held is due.
    pub(super) fn next_deadline(&self) -> Option<Instant> {
        self.held.iter().map(|(_, at)| *at).min()
    }
}

impl Supervisor {
    /// `event` happened: each hook that takes it runs.
    fn raise(&self, event: &Event) {
        let away = !*self.clients.looked_at.borrow();
        let jobs = self.plugins.hook_jobs(event, away);
        if !jobs.is_empty() {
            self.events.hooks.raise(jobs);
        }
    }

    /// An event of `kind` now in `workspace`, if it is still there.
    fn event_in(&self, kind: EventKind, workspace: WorkspaceId) -> Option<Event> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)?;
        let project = self
            .state
            .projects
            .iter()
            .find(|project| project.id == found.project);
        Some(Event::new(kind, self.clock.now(), project, Some(found)))
    }

    /// An event of `kind` now of `session`, if it is an agent's.
    fn session_event(&self, kind: EventKind, session: SessionId) -> Option<Event> {
        let stored = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .filter(|stored| !stored.kind().stands_beside())?;
        Some(self.event_in(kind, stored.workspace)?.of_session(stored))
    }

    /// A session was added: an agent's starts.
    pub(super) fn session_added(&self, session: SessionId) {
        if let Some(event) = self.session_event(EventKind::SessionStarted, session) {
            self.raise(&event);
        }
    }

    /// `session` went from `before` to `status`.
    pub(super) fn status_events(
        &mut self,
        session: SessionId,
        before: SessionStatus,
        status: SessionStatus,
    ) {
        if before == status {
            return;
        }
        self.events.held.retain(|(held, _)| *held != session);
        let stopped = self.events.stopped.take() == Some(session);
        let kinds: &[EventKind] = match status {
            SessionStatus::NeedsYou { .. } => &[EventKind::SessionNeedsYou],
            SessionStatus::Finished => {
                let due = self.clock.instant() + muster_core::news::FINISH_HOLD;
                self.events.held.push((session, due));
                &[]
            }
            // Stopped by the person, it ended as they asked: no error.
            SessionStatus::Exited { success } if !before.is_exited() => {
                if success || stopped {
                    &[EventKind::SessionEnded]
                } else {
                    &[EventKind::SessionError, EventKind::SessionEnded]
                }
            }
            // Started again, in the same session, after it ended.
            SessionStatus::Fresh | SessionStatus::Running if before.is_exited() => {
                &[EventKind::SessionStarted]
            }
            _ => &[],
        };
        for kind in kinds {
            if let Some(event) = self.session_event(*kind, session) {
                self.raise(&event);
            }
        }
    }

    /// The finishes held long enough by `now`: each still finished is
    /// told; one at work again meanwhile was let go as it changed.
    pub(super) fn tick_events(&mut self, now: Instant) {
        let (due, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.events.held)
            .into_iter()
            .partition(|(_, at)| *at <= now);
        self.events.held = held;
        for (session, _) in due {
            let finished = self
                .state
                .sessions
                .iter()
                .any(|stored| stored.id == session && stored.status == SessionStatus::Finished);
            if finished && let Some(event) = self.session_event(EventKind::SessionFinished, session)
            {
                self.raise(&event);
            }
        }
    }

    /// `session`, taken over from the daemon before this one or back from
    /// a restart that did not happen, finished `ago` and was not told yet:
    /// it is told as its hold ends, as it would have been (`PRD-09`). One
    /// held already, as after a restart that did not happen, is held once.
    pub(super) fn hold_carried_finish(&mut self, session: SessionId, ago: Duration) {
        let Some(left) = muster_core::news::FINISH_HOLD
            .checked_sub(ago)
            .filter(|left| !left.is_zero())
        else {
            return;
        };
        if self.events.held.iter().any(|(held, _)| *held == session) {
            return;
        }
        let due = self.clock.instant() + left;
        self.events.held.push((session, due));
    }

    /// The person started an agent, `session`, on the ticket `key`.
    pub(super) fn ticket_assigned(&self, session: SessionId, key: &muster_core::TicketKey) {
        if let Some(mut event) = self.session_event(EventKind::TicketAssigned, session) {
            event.ticket = Some(key.to_string());
            self.raise(&event);
        }
    }

    /// A worktree was made, or found made outside Muster, as `workspace`.
    pub(super) fn worktree_created(&self, workspace: WorkspaceId) {
        if let Some(event) = self.event_in(EventKind::WorktreeCreated, workspace) {
            self.raise(&event);
        }
    }

    /// `workspace` goes, told before it leaves the state: a worktree's
    /// going is told; a run's container's is not.
    pub(super) fn workspace_going(&self, workspace: WorkspaceId) {
        let worktree = self
            .state
            .workspaces
            .iter()
            .any(|found| found.id == workspace && found.kind.is_worktree());
        if worktree && let Some(event) = self.event_in(EventKind::WorktreeRemoved, workspace) {
            self.raise(&event);
        }
    }

    /// The pull request from `workspace`'s branch was looked at: `found`,
    /// where it was `before`. Told where it is one and changed since an
    /// earlier look of this daemon's.
    pub(super) fn pull_request_looked(
        &mut self,
        workspace: WorkspaceId,
        before: Option<&PullRequestStatus>,
        found: Option<&PullRequestStatus>,
    ) {
        let first = self.events.pulls.insert(workspace);
        if first || before == found {
            return;
        }
        if let Some(found) = found
            && let Some(event) = self.event_in(EventKind::PullRequestChanged, workspace)
        {
            self.raise(&event.with_pull_request(found));
        }
    }

    /// `project`'s clashes were looked at, each workspace's `before` and
    /// as they are now: each pair of workspaces that came to meet is told
    /// once, from the side named first.
    pub(super) fn clashes_looked(
        &mut self,
        project: ProjectId,
        before: &HashMap<WorkspaceId, Vec<Clash>>,
    ) {
        if self.events.clashes.insert(project) {
            return;
        }
        let mut told: HashSet<(WorkspaceId, WorkspaceId)> = HashSet::new();
        let mut events = Vec::new();
        for workspace in self
            .state
            .workspaces
            .iter()
            .filter(|found| found.project == project)
        {
            for clash in &workspace.clashes {
                let was = before
                    .get(&workspace.id)
                    .is_some_and(|clashes| clashes.iter().any(|old| old.with == clash.with));
                let pair = if workspace.id < clash.with {
                    (workspace.id, clash.with)
                } else {
                    (clash.with, workspace.id)
                };
                if was || !told.insert(pair) {
                    continue;
                }
                let Some(with) = self
                    .state
                    .workspaces
                    .iter()
                    .find(|other| other.id == clash.with)
                else {
                    continue;
                };
                if let Some(event) = self.event_in(EventKind::ClashFound, workspace.id) {
                    events.push(event.with_clash(clash, with));
                }
            }
        }
        for event in events {
            self.raise(&event);
        }
    }

    /// Forgets what was looked at in `workspace`, gone.
    pub(super) fn forget_events_of(&mut self, workspace: WorkspaceId) {
        self.events.pulls.remove(&workspace);
    }
}
