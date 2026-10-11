//! The UI's copy of the daemon's state, kept current from the snapshot and
//! the deltas that follow it. It decides nothing; it only answers questions
//! the views ask.

use std::collections::HashMap;

use muster_core::protocol::{Delta, ResourceReport, SessionUsage, Snapshot};
use muster_core::{
    Project, ProjectId, Session, SessionId, SessionStatus, Workspace, WorkspaceId, WorkspaceKind,
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
        self.resources = snapshot.resources;
        self.loaded = true;
    }

    pub(crate) fn loaded(&self) -> bool {
        self.loaded
    }

    pub(crate) fn apply(&mut self, delta: Delta) {
        match delta {
            Delta::ProjectUpserted(project) => upsert(&mut self.projects, project, |p| p.id),
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

    pub(crate) fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub(crate) fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub(crate) fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    pub(crate) fn workspaces_of(&self, project: ProjectId) -> impl Iterator<Item = &Workspace> {
        self.workspaces
            .iter()
            .filter(move |workspace| workspace.project == project)
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

    /// The branch of a worktree.
    pub(crate) fn branch(&self, workspace: WorkspaceId) -> Option<&str> {
        self.workspace(workspace).map(|found| {
            let WorkspaceKind::Worktree { branch, .. } = &found.kind;
            branch.as_str()
        })
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

    use muster_core::{Timestamp, WaitReason};

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
