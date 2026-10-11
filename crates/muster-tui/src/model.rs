//! The UI's copy of the daemon's state, kept current from the snapshot and
//! the deltas that follow it. It decides nothing; it only answers questions
//! the views ask.

use std::collections::HashMap;

use muster_core::protocol::{Delta, ResourceReport, SessionUsage, Snapshot};
use muster_core::{
    Project, ProjectId, Run, RunId, RunStage, Session, SessionId, SessionStatus, Workspace,
    WorkspaceId,
};

/// Something waiting on the person, in the Needs you queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Need {
    Session(SessionId),
    Run(RunId),
}

/// How many ended runs the runs list shows, newest first.
const ENDED_RUNS_SHOWN: usize = 20;

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
    /// The same for each run's stage.
    run_changed: HashMap<RunId, u64>,
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
        self.run_changed.clear();
        for run in &snapshot.runs {
            self.clock += 1;
            self.run_changed.insert(run.id, self.clock);
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
            Delta::RunUpserted(run) => {
                let moved = self
                    .run(run.id)
                    .is_none_or(|known| known.stage != run.stage);
                if moved {
                    self.clock += 1;
                    self.run_changed.insert(run.id, self.clock);
                }
                upsert(&mut self.runs, run, |r| r.id);
            }
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

    pub(crate) fn run(&self, id: RunId) -> Option<&Run> {
        self.runs.iter().find(|run| run.id == id)
    }

    /// The runs list: those going, what waits on the person first, then
    /// the latest that ended.
    pub(crate) fn runs_listed(&self) -> Vec<RunId> {
        let mut going: Vec<&Run> = self
            .runs
            .iter()
            .filter(|run| !run.stage.is_over())
            .collect();
        going.sort_by_key(|run| (run.stage.list_order(), run.created_at, run.id));
        let mut ended: Vec<&Run> = self.runs.iter().filter(|run| run.stage.is_over()).collect();
        ended.sort_by_key(|run| std::cmp::Reverse((run.ended_at, run.created_at)));
        going
            .into_iter()
            .chain(ended.into_iter().take(ENDED_RUNS_SHOWN))
            .map(|run| run.id)
            .collect()
    }

    /// The session in which the person drives the run's agent, if they have
    /// taken it over.
    pub(crate) fn agent_session_of(&self, run: RunId) -> Option<SessionId> {
        let workspace = self
            .workspaces
            .iter()
            .find(|workspace| workspace.kind.run() == Some(run))?;
        self.sessions
            .iter()
            .find(|session| session.workspace == workspace.id && session.harness == "claude")
            .map(|session| session.id)
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

    /// Everything waiting on the person, oldest first within each kind:
    /// sessions asking something, runs stopped for them, runs ready for
    /// their review, then sessions with news not yet seen.
    pub(crate) fn needs_you(&self) -> Vec<Need> {
        let mut waiting: Vec<(u8, u64, Need)> = self
            .sessions
            .iter()
            .filter(|session| needs_attention(session))
            .map(|session| {
                let asking = matches!(session.status, SessionStatus::NeedsYou { .. });
                let changed = self.changed.get(&session.id).copied().unwrap_or(0);
                (
                    if asking { 0 } else { 3 },
                    changed,
                    Need::Session(session.id),
                )
            })
            .collect();
        waiting.extend(
            self.runs
                .iter()
                .filter(|run| run.stage.waits_on_person())
                .map(|run| {
                    let kind = if run.stage == RunStage::NeedsYou {
                        1
                    } else {
                        2
                    };
                    let changed = self.run_changed.get(&run.id).copied().unwrap_or(0);
                    (kind, changed, Need::Run(run.id))
                }),
        );
        waiting.sort_by_key(|(kind, changed, _)| (*kind, *changed));
        waiting.into_iter().map(|(_, _, need)| need).collect()
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
        let sessions =
            |ids: &[SessionId]| ids.iter().map(|id| Need::Session(*id)).collect::<Vec<_>>();
        assert_eq!(model.needs_you(), sessions(&[three.id, two.id, one.id]));

        model.apply(Delta::SessionStatus {
            session: two.id,
            status: SessionStatus::Finished,
            unseen: false,
        });
        assert_eq!(model.needs_you(), sessions(&[three.id, one.id]));
    }
}
