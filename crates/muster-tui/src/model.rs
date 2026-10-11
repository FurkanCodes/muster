//! The UI's copy of the daemon's state, kept current from the snapshot and
//! the deltas that follow it. It decides nothing; it only answers questions
//! the views ask.

use std::collections::HashMap;

use muster_core::protocol::{Delta, ResourceReport, SessionUsage, Snapshot};
use muster_core::{
    Project, ProjectId, Run, Session, SessionId, SessionStatus, Workspace, WorkspaceId,
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
            } => {
                if let Some(stored) = self.sessions.iter_mut().find(|s| s.id == session) {
                    if stored.status != status {
                        self.clock += 1;
                        self.changed.insert(session, self.clock);
                    }
                    stored.status = status;
                    stored.unseen = unseen;
                }
            }
            Delta::Resources(report) => self.resources = Some(report),
            Delta::RunUpserted(run) => upsert(&mut self.runs, run, |r| r.id),
        }
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

    /// The project's worktrees for issue `number`: on branches named for it.
    pub(crate) fn worktrees_for(&self, project: ProjectId, number: u64) -> Vec<WorkspaceId> {
        self.workspaces
            .iter()
            .filter(|workspace| workspace.project == project && workspace.kind.is_worktree())
            .filter(|workspace| {
                muster_core::tickets::issue_of_branch(workspace.kind.branch()) == Some(number)
            })
            .map(|workspace| workspace.id)
            .collect()
    }

    /// The sessions working on issue `number` of `project`: those in the
    /// project's worktrees on branches named for it, oldest first.
    pub(crate) fn agents_on(&self, project: ProjectId, number: u64) -> Vec<&Session> {
        let workspaces = self.worktrees_for(project, number);
        let mut agents: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| workspaces.contains(&session.workspace))
            .collect();
        agents.sort_by_key(|session| (session.created_at, session.id));
        agents
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

    /// The sessions list: each project, its worktrees, their sessions.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for project in &self.projects {
            rows.push(Row::Project(project.id));
            for workspace in self.workspaces_of(project.id) {
                rows.push(Row::Worktree(workspace.id));
                rows.extend(self.sessions_of(workspace.id).map(|s| Row::Session(s.id)));
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

/// Whether a session is waiting on the person: asking something, or with
/// news they have not seen.
pub(crate) fn needs_attention(session: &Session) -> bool {
    matches!(session.status, SessionStatus::NeedsYou { .. }) || session.unseen
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
        }
    }

    pub(crate) fn session(workspace: WorkspaceId, title: &str, created: i64) -> Session {
        Session {
            id: SessionId::new(),
            workspace,
            harness: "shell".to_owned(),
            title: title.to_owned(),
            status: SessionStatus::Fresh,
            unseen: false,
            resume_id: None,
            created_at: Timestamp::from_millis(created),
            ended_at: None,
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
        };
        model.apply(finished_unseen(two.id));
        model.apply(finished_unseen(one.id));
        model.apply(Delta::SessionStatus {
            session: three.id,
            status: SessionStatus::NeedsYou {
                reason: WaitReason::Permission,
            },
            unseen: true,
        });
        assert_eq!(model.needs_you(), [three.id, two.id, one.id]);

        model.apply(Delta::SessionStatus {
            session: two.id,
            status: SessionStatus::Finished,
            unseen: false,
        });
        assert_eq!(model.needs_you(), [three.id, one.id]);
    }
}
