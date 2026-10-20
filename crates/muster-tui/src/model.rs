//! The UI's copy of the daemon's state, kept current from the snapshot and
//! the deltas that follow it. It decides nothing; it only answers questions
//! the views ask.

use std::collections::HashMap;

use muster_core::protocol::plugins::PluginInfo;
use muster_core::protocol::{Delta, ResourceReport, SessionUsage, Snapshot};
use muster_core::settings::Preferences;
use muster_core::{
    Project, ProjectId, Run, Session, SessionId, SessionStatus, TicketKey, Workspace, WorkspaceId,
};

/// One line of the sessions list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Row {
    Project(ProjectId),
    Worktree(WorkspaceId),
    Session(SessionId),
}

#[derive(Debug, Default)]
pub(crate) struct Model {
    projects: Vec<Project>,
    workspaces: Vec<Workspace>,
    sessions: Vec<Session>,
    runs: Vec<Run>,
    /// When each session's status last changed, as this client saw it: a
    /// counter, which orders the Needs you queue oldest first.
    changed: HashMap<SessionId, u64>,
    clock: u64,
    resources: Option<ResourceReport>,
    /// Muster's own settings.
    preferences: Preferences,
    /// Whether Muster's helper shows notifications of the Mac's own.
    notifier: bool,
    /// The plugins, as the daemon has them (protocol 41).
    plugins: Vec<PluginInfo>,
    /// Whether a snapshot has arrived; until then the model is empty, not
    /// "no sessions".
    loaded: bool,
}

impl Model {
    /// Starts over from a snapshot.
    pub(crate) fn replace(&mut self, snapshot: Snapshot) {
        let mut sessions = snapshot.sessions;
        sessions.sort_by_key(|session| (session.created_at, session.id));
        self.changed.clear();
        for session in &sessions {
            self.clock += 1;
            self.changed.insert(session.id, self.clock);
        }
        self.projects = snapshot.projects;
        self.workspaces = snapshot.workspaces;
        self.sessions = sessions;
        self.runs = snapshot.runs;
        self.resources = snapshot.resources;
        self.preferences = snapshot.preferences;
        self.notifier = snapshot.notifier;
        self.plugins = snapshot.plugins;
        self.loaded = true;
    }

    pub(crate) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(crate) fn apply(&mut self, delta: Delta) {
        match delta {
            Delta::ProjectUpserted(project) => upsert(&mut self.projects, project, |p| p.id),
            Delta::ProjectRemoved { id } => {
                let gone: Vec<WorkspaceId> = self
                    .workspaces_of(id)
                    .map(|workspace| workspace.id)
                    .collect();
                self.sessions
                    .retain(|session| !gone.contains(&session.workspace));
                self.workspaces.retain(|workspace| workspace.project != id);
                self.projects.retain(|project| project.id != id);
            }
            Delta::WorkspaceUpserted(workspace) => {
                upsert(&mut self.workspaces, workspace, |w| w.id);
            }
            Delta::WorkspaceRemoved { id } => {
                self.workspaces.retain(|workspace| workspace.id != id);
                self.sessions.retain(|session| session.workspace != id);
            }
            Delta::SessionUpserted(session) => {
                self.touch(session.id);
                upsert(&mut self.sessions, session, |s| s.id);
            }
            Delta::SessionRemoved { id } => {
                self.sessions.retain(|session| session.id != id);
                self.changed.remove(&id);
            }
            Delta::SessionStatus {
                session,
                status,
                unseen,
                since,
            } => {
                if let Some(stored) = self.sessions.iter_mut().find(|s| s.id == session) {
                    if stored.status != status {
                        self.clock += 1;
                        self.changed.insert(session, self.clock);
                    }
                    stored.status = status;
                    stored.unseen = unseen;
                    stored.status_since = since;
                }
            }
            Delta::SessionActivity { session, activity } => {
                if let Some(stored) = self.sessions.iter_mut().find(|s| s.id == session) {
                    stored.activity = activity;
                }
            }
            Delta::Resources(report) => self.resources = Some(report),
            Delta::RunUpserted(run) => upsert(&mut self.runs, run, |r| r.id),
            Delta::Preferences(preferences) => self.preferences = preferences,
            Delta::Notifier { ready } => self.notifier = ready,
            Delta::PluginsChanged(plugins) => self.plugins = plugins,
            // For the notch: this window says itself whether it is looked at.
            Delta::LookedAt { .. } => {}
        }
    }

    /// Muster's own settings.
    pub(crate) fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    /// The plugins, as the daemon last said.
    pub(crate) fn plugins(&self) -> &[PluginInfo] {
        &self.plugins
    }

    /// Whether Muster's helper shows notifications of the Mac's own, so
    /// this window's terminal is sent none.
    pub(crate) fn notifier(&self) -> bool {
        self.notifier
    }

    /// The latest measurement of the host, if any.
    pub(crate) fn resources(&self) -> Option<&ResourceReport> {
        self.resources.as_ref()
    }

    /// What a running session used at the latest measurement.
    pub(crate) fn usage(&self, session: SessionId) -> Option<&SessionUsage> {
        self.resources
            .as_ref()?
            .sessions
            .iter()
            .find(|usage| usage.session == session)
    }

    fn touch(&mut self, session: SessionId) {
        if !self.changed.contains_key(&session) {
            self.clock += 1;
            self.changed.insert(session, self.clock);
        }
    }

    pub(crate) fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// The projects' worktrees for ticket `key`: on branches named for it.
    pub(crate) fn worktrees_for(
        &self,
        projects: &[ProjectId],
        key: &TicketKey,
    ) -> Vec<WorkspaceId> {
        self.workspaces
            .iter()
            .filter(|workspace| {
                projects.contains(&workspace.project) && workspace.kind.is_worktree()
            })
            .filter(|workspace| {
                muster_core::tickets::issue_of_branch(workspace.kind.branch()).as_ref() == Some(key)
            })
            .map(|workspace| workspace.id)
            .collect()
    }

    /// The agents working on ticket `key` in `projects`: the sessions in
    /// their worktrees on branches named for it, oldest first; a plugin's
    /// panel beside them is none.
    pub(crate) fn agents_on(&self, projects: &[ProjectId], key: &TicketKey) -> Vec<&Session> {
        let workspaces = self.worktrees_for(projects, key);
        let mut agents: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| workspaces.contains(&session.workspace))
            .filter(|session| !muster_core::harness::is_panel(&session.harness))
            .collect();
        agents.sort_by_key(|session| (session.created_at, session.id));
        agents
    }

    /// The pull request that says most of how ticket `key`'s work stands,
    /// of those from its worktrees' branches: a merged one, then an open
    /// one, then a draft, then a closed one; the newest of each.
    pub(crate) fn ticket_pull_request(
        &self,
        projects: &[ProjectId],
        key: &TicketKey,
    ) -> Option<&muster_core::PullRequestStatus> {
        let rank = |state| match state {
            muster_core::PullRequestState::Merged => 3,
            muster_core::PullRequestState::Open => 2,
            muster_core::PullRequestState::Draft => 1,
            muster_core::PullRequestState::Closed => 0,
        };
        self.worktrees_for(projects, key)
            .into_iter()
            .filter_map(|id| self.workspace(id)?.pull_request.as_ref())
            .max_by_key(|found| (rank(found.state), found.number))
    }

    pub(crate) fn projects(&self) -> &[Project] {
        &self.projects
    }

    pub(crate) fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub(crate) fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub(crate) fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    /// A project's workspaces: its own checkout first, then its worktrees
    /// oldest first, then its runs' containers.
    pub(crate) fn workspaces_of(&self, project: ProjectId) -> impl Iterator<Item = &Workspace> {
        let mine = move |workspace: &&Workspace| workspace.project == project;
        let checkouts = self
            .workspaces
            .iter()
            .filter(mine)
            .filter(|workspace| workspace.kind.is_checkout());
        let worktrees = self
            .workspaces
            .iter()
            .filter(mine)
            .filter(|workspace| workspace.kind.is_worktree());
        let containers = self
            .workspaces
            .iter()
            .filter(mine)
            .filter(|workspace| workspace.kind.run().is_some());
        checkouts.chain(worktrees).chain(containers)
    }

    /// The run whose container `workspace` is, if it is one.
    pub(crate) fn run_of(&self, workspace: WorkspaceId) -> Option<&Run> {
        let run = self.workspace(workspace)?.kind.run()?;
        self.runs.iter().find(|candidate| candidate.id == run)
    }

    /// A project's own checkout.
    pub(crate) fn checkout_of(&self, project: ProjectId) -> Option<WorkspaceId> {
        self.workspaces_of(project)
            .find(|workspace| workspace.kind.is_checkout())
            .map(|workspace| workspace.id)
    }

    /// Whether a project is a plain folder, not a git repository, as its
    /// own checkout shows: it has no branch.
    pub(crate) fn is_plain_folder(&self, project: ProjectId) -> bool {
        self.workspaces_of(project)
            .any(|workspace| workspace.kind.is_checkout() && !workspace.kind.in_git())
    }

    pub(crate) fn sessions_of(&self, workspace: WorkspaceId) -> impl Iterator<Item = &Session> {
        self.sessions
            .iter()
            .filter(move |session| session.workspace == workspace)
    }

    /// A workspace's sessions as the list shows them: its chats, then its
    /// shells and plugins' panels, which stand by them rather than among
    /// them.
    pub(crate) fn listed_of(&self, workspace: WorkspaceId) -> impl Iterator<Item = &Session> {
        let (chats, shells): (Vec<&Session>, Vec<&Session>) = self
            .sessions_of(workspace)
            .partition(|session| !stands_beside(session));
        chats.into_iter().chain(shells)
    }

    /// Whether `shell` (or a plugin's panel) is shown as its workspace's
    /// one chat's: a worktree with one agent at work in it, as most are,
    /// has its shell under that agent's chat; where several chats share the
    /// workspace, its shells serve them all, and stand after them.
    pub(crate) fn under_its_chat(&self, shell: &Session) -> bool {
        stands_beside(shell)
            && self
                .sessions_of(shell.workspace)
                .filter(|session| !stands_beside(session))
                .count()
                == 1
    }

    /// The sessions list: each project, its worktrees, their sessions.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for project in &self.projects {
            rows.push(Row::Project(project.id));
            for workspace in self.workspaces_of(project.id) {
                rows.push(Row::Worktree(workspace.id));
                rows.extend(self.listed_of(workspace.id).map(|s| Row::Session(s.id)));
            }
        }
        rows
    }

    /// Sessions waiting on the person: those asking something first, then
    /// those with news not yet seen; oldest first within each.
    pub(crate) fn needs_you(&self) -> Vec<SessionId> {
        let mut waiting: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| needs_attention(session))
            .collect();
        waiting.sort_by_key(|session| {
            let asking = matches!(session.status, SessionStatus::NeedsYou { .. });
            (!asking, self.changed.get(&session.id).copied().unwrap_or(0))
        });
        waiting.into_iter().map(|session| session.id).collect()
    }

    /// The sessions on a row: a project's in all its workspaces, a
    /// worktree's, or the session itself.
    pub(crate) fn sessions_on(&self, row: Row) -> impl Iterator<Item = &Session> {
        self.sessions.iter().filter(move |session| match row {
            Row::Project(project) => self
                .workspace(session.workspace)
                .is_some_and(|workspace| workspace.project == project),
            Row::Worktree(workspace) => session.workspace == workspace,
            Row::Session(id) => session.id == id,
        })
    }

    /// The session that says most of how a row stands, in the order *Needs
    /// you* goes: one asking the person something, then one with news not
    /// seen, oldest first; else one at work. `None` while none is.
    pub(crate) fn most_urgent(&self, row: Row) -> Option<&Session> {
        let on: Vec<SessionId> = self.sessions_on(row).map(|session| session.id).collect();
        self.needs_you()
            .into_iter()
            .find(|id| on.contains(id))
            .and_then(|id| self.session(id))
            .or_else(|| {
                self.sessions_on(row)
                    .find(|session| session.status == SessionStatus::Running)
            })
    }

    /// The project a row belongs to.
    pub(crate) fn project_of(&self, row: Row) -> Option<ProjectId> {
        match row {
            Row::Project(id) => Some(id),
            Row::Worktree(id) => self.workspace(id).map(|workspace| workspace.project),
            Row::Session(id) => self
                .session(id)
                .and_then(|session| self.workspace(session.workspace))
                .map(|workspace| workspace.project),
        }
    }

    /// The worktree a row belongs to, if any.
    pub(crate) fn worktree_of(&self, row: Row) -> Option<WorkspaceId> {
        match row {
            Row::Project(_) => None,
            Row::Worktree(id) => Some(id),
            Row::Session(id) => self.session(id).map(|session| session.workspace),
        }
    }

    /// What a workspace has checked out.
    pub(crate) fn branch(&self, workspace: WorkspaceId) -> Option<&str> {
        self.workspace(workspace).map(|found| found.kind.branch())
    }
}

pub(crate) use muster_core::news::needs_attention;

/// Whether `session` stands beside the chats rather than among them: a
/// shell, the person's own, or a plugin's panel, not an agent's chat.
pub(crate) fn stands_beside(session: &Session) -> bool {
    muster_core::harness::stands_beside(&session.harness)
}

fn upsert<T, K: PartialEq>(items: &mut Vec<T>, item: T, key: impl Fn(&T) -> K) {
    match items
        .iter_mut()
        .find(|existing| key(existing) == key(&item))
    {
        Some(existing) => *existing = item,
        None => items.push(item),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::PathBuf;

    use muster_core::{Timestamp, WaitReason, WorkspaceKind};

    use super::*;

    pub(crate) fn project(name: &str) -> Project {
        Project {
            id: ProjectId::new(),
            name: name.to_owned(),
            root: PathBuf::from(format!("/src/{name}")),
            created_at: Timestamp::from_millis(1),
        }
    }

    pub(crate) fn worktree(project: ProjectId, branch: &str) -> Workspace {
        Workspace {
            id: WorkspaceId::new(),
            project,
            kind: WorkspaceKind::Worktree {
                path: PathBuf::from(format!("/w/{branch}")),
                branch: branch.to_owned(),
            },
            created_at: Timestamp::from_millis(2),
            pull_request: None,
            changes: None,
            timeline: None,
            clashes: Vec::new(),
            stacked_on: None,
        }
    }

    /// A session of an agent that is no one of Muster's own, so nothing
    /// an agent of its own would do (a hook, a suspend) comes with it.
    pub(crate) fn session(workspace: WorkspaceId, title: &str, created: i64) -> Session {
        Session {
            id: SessionId::new(),
            workspace,
            harness: "agent".to_owned(),
            title: title.to_owned(),
            status: SessionStatus::Fresh,
            unseen: false,
            resume_id: None,
            created_at: Timestamp::from_millis(created),
            ended_at: None,
            status_since: Timestamp::from_millis(created),
            activity: None,
            spend: None,
            tidy: None,
            beside: None,
        }
    }

    #[test]
    fn rows_nest_sessions_under_worktrees_under_projects() {
        let app = project("app");
        let other = project("other");
        let tree = worktree(app.id, "work");
        let shell = session(tree.id, "Shell", 3);
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app.clone(), other.clone()],
            workspaces: vec![tree.clone()],
            sessions: vec![shell.clone()],
            ..Snapshot::default()
        });
        assert_eq!(
            model.rows(),
            [
                Row::Project(app.id),
                Row::Worktree(tree.id),
                Row::Session(shell.id),
                Row::Project(other.id),
            ]
        );
        assert_eq!(model.project_of(Row::Session(shell.id)), Some(app.id));
        assert_eq!(model.worktree_of(Row::Session(shell.id)), Some(tree.id));
        assert_eq!(model.branch(tree.id), Some("work"));
    }

    #[test]
    fn deltas_update_and_removing_a_worktree_drops_its_sessions() {
        let app = project("app");
        let tree = worktree(app.id, "work");
        let shell = session(tree.id, "Shell", 3);
        let mut model = Model::default();
        model.apply(Delta::ProjectUpserted(app));
        model.apply(Delta::WorkspaceUpserted(tree.clone()));
        model.apply(Delta::SessionUpserted(shell.clone()));
        model.apply(Delta::SessionStatus {
            session: shell.id,
            status: SessionStatus::Running,
            unseen: false,
            since: muster_core::Timestamp::from_millis(10),
        });
        assert_eq!(
            model.session(shell.id).unwrap().status,
            SessionStatus::Running
        );
        model.apply(Delta::WorkspaceRemoved { id: tree.id });
        assert!(model.sessions().is_empty());
        assert_eq!(model.rows().len(), 1);
    }

    #[test]
    fn needs_you_puts_questions_first_then_news_oldest_first() {
        let app = project("app");
        let tree = worktree(app.id, "work");
        let (one, two, three) = (
            session(tree.id, "one", 1),
            session(tree.id, "two", 2),
            session(tree.id, "three", 3),
        );
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app],
            workspaces: vec![tree],
            sessions: vec![three.clone(), one.clone(), two.clone()],
            ..Snapshot::default()
        });
        let finished_unseen = |session: SessionId| Delta::SessionStatus {
            session,
            status: SessionStatus::Finished,
            unseen: true,
            since: muster_core::Timestamp::from_millis(10),
        };
        model.apply(finished_unseen(two.id));
        model.apply(finished_unseen(one.id));
        model.apply(Delta::SessionStatus {
            session: three.id,
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            unseen: true,
            since: muster_core::Timestamp::from_millis(10),
        });
        assert_eq!(model.needs_you(), [three.id, two.id, one.id]);

        model.apply(Delta::SessionStatus {
            session: two.id,
            status: SessionStatus::Finished,
            unseen: false,
            since: muster_core::Timestamp::from_millis(10),
        });
        assert_eq!(model.needs_you(), [three.id, one.id]);
    }
}
