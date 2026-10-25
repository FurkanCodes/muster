//! A client's copy of the daemon's state, kept current from the snapshot
//! and the deltas that follow it. It decides nothing; it only answers the
//! questions the clients ask of it: what needs the person and in what
//! order, how the sessions are listed, how the agents stand. The terminal
//! UI and the desktop app both keep one, so they answer alike (`ARC-24`).
//!
//! Like `muster-core` and `muster-state` it does no IO (`ARC-01`): a client
//! hands it what the daemon sent and reads what it says.

use std::collections::HashMap;

use muster_core::protocol::plugins::PluginInfo;
use muster_core::protocol::{Delta, ResourceReport, SessionUsage, Snapshot};
use muster_core::settings::Preferences;
use muster_core::{
    Project, ProjectId, Run, Session, SessionId, SessionStatus, Space, TicketKey, Workspace,
    WorkspaceId,
};

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures;
mod spaces;

pub use spaces::View;

/// One line of the sessions list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Row {
    Project(ProjectId),
    Worktree(WorkspaceId),
    Session(SessionId),
    /// The heading over the free terminals, those in no project
    /// (`ARC-33`), which follow it as sessions.
    Terminals,
}

/// How many agents work, wait on the person and are done: a shell, the
/// person's own, and a plugin's panel are no agents.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub working: usize,
    pub waiting: usize,
    pub done: usize,
}

/// The daemon's state as one client holds it.
#[derive(Debug, Default)]
pub struct Model {
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
    /// The person's spaces, in the order they were made.
    spaces: Vec<Space>,
    /// Whether a snapshot has arrived; until then the model is empty, not
    /// "no sessions".
    loaded: bool,
}

impl Model {
    /// Starts over from a snapshot.
    pub fn replace(&mut self, snapshot: Snapshot) {
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
        self.spaces = snapshot.spaces;
        self.loaded = true;
        self.order_projects();
    }

    /// Puts the projects in the person's order (`Preferences::projects`),
    /// the others after them oldest first, as the daemon lists them. The
    /// sort is stable, so projects added in the same moment keep the
    /// order they came in.
    fn order_projects(&mut self) {
        let order = &self.preferences.projects;
        self.projects.sort_by_key(|project| {
            let at = order.iter().position(|id| *id == project.id);
            (at.unwrap_or(usize::MAX), project.created_at)
        });
    }

    pub fn loaded(&self) -> bool {
        self.loaded
    }

    pub fn apply(&mut self, delta: Delta) {
        match delta {
            Delta::ProjectUpserted(project) => {
                upsert(&mut self.projects, project, |p| p.id);
                self.order_projects();
            }
            Delta::ProjectRemoved { id } => {
                let gone: Vec<WorkspaceId> = self
                    .workspaces_of(id)
                    .map(|workspace| workspace.id)
                    .collect();
                self.sessions.retain(|session| {
                    session
                        .workspace
                        .is_none_or(|workspace| !gone.contains(&workspace))
                });
                self.workspaces.retain(|workspace| workspace.project != id);
                self.projects.retain(|project| project.id != id);
            }
            Delta::WorkspaceUpserted(workspace) => {
                upsert(&mut self.workspaces, workspace, |w| w.id);
            }
            Delta::WorkspaceRemoved { id } => {
                self.workspaces.retain(|workspace| workspace.id != id);
                self.sessions.retain(|session| !session.is_in(id));
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
            Delta::Preferences(preferences) => {
                self.preferences = preferences;
                self.order_projects();
            }
            Delta::Notifier { ready } => self.notifier = ready,
            Delta::PluginsChanged(plugins) => self.plugins = plugins,
            Delta::SpaceUpserted(space) => upsert(&mut self.spaces, space, |s| s.id),
            Delta::SpaceRemoved { id } => self.spaces.retain(|space| space.id != id),
            // For the notch: this window says itself whether it is looked at.
            Delta::LookedAt { .. } => {}
        }
    }

    /// Muster's own settings.
    pub fn preferences(&self) -> &Preferences {
        &self.preferences
    }

    /// The plugins, as the daemon last said.
    pub fn plugins(&self) -> &[PluginInfo] {
        &self.plugins
    }

    /// Whether Muster's helper shows notifications of the Mac's own, so
    /// this window's terminal is sent none.
    pub fn notifier(&self) -> bool {
        self.notifier
    }

    /// The latest measurement of the host, if any.
    pub fn resources(&self) -> Option<&ResourceReport> {
        self.resources.as_ref()
    }

    /// What a running session used at the latest measurement.
    pub fn usage(&self, session: SessionId) -> Option<&SessionUsage> {
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

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    /// The projects' worktrees for ticket `key`: made for it, whatever their
    /// branches are called now.
    pub fn worktrees_for(&self, projects: &[ProjectId], key: &TicketKey) -> Vec<WorkspaceId> {
        self.workspaces
            .iter()
            .filter(|workspace| {
                projects.contains(&workspace.project) && workspace.kind.is_worktree()
            })
            .filter(|workspace| workspace.ticket.as_ref() == Some(key))
            .map(|workspace| workspace.id)
            .collect()
    }

    /// The agents working on ticket `key` in `projects`: the sessions in
    /// the worktrees made for it, oldest first; a plugin's
    /// panel beside them is none.
    pub fn agents_on(&self, projects: &[ProjectId], key: &TicketKey) -> Vec<&Session> {
        let workspaces = self.worktrees_for(projects, key);
        let mut agents: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session
                    .workspace
                    .is_some_and(|workspace| workspaces.contains(&workspace))
            })
            .filter(|session| !session.kind().is_panel())
            .collect();
        agents.sort_by_key(|session| (session.created_at, session.id));
        agents
    }

    /// The pull request that says most of how ticket `key`'s work stands,
    /// of those from its worktrees' branches: a merged one, then an open
    /// one, then a draft, then a closed one; the newest of each.
    pub fn ticket_pull_request(
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

    pub fn projects(&self) -> &[Project] {
        &self.projects
    }

    /// Every workspace, as the daemon listed them.
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.sessions.iter().find(|session| session.id == id)
    }

    /// A project's workspaces: its own checkout first, then its worktrees
    /// oldest first, then its runs' containers.
    pub fn workspaces_of(&self, project: ProjectId) -> impl Iterator<Item = &Workspace> {
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
    pub fn run_of(&self, workspace: WorkspaceId) -> Option<&Run> {
        let run = self.workspace(workspace)?.kind.run()?;
        self.runs.iter().find(|candidate| candidate.id == run)
    }

    /// A project's own checkout.
    pub fn checkout_of(&self, project: ProjectId) -> Option<WorkspaceId> {
        self.workspaces_of(project)
            .find(|workspace| workspace.kind.is_checkout())
            .map(|workspace| workspace.id)
    }

    /// Whether a project is a plain folder, not a git repository, as its
    /// own checkout shows: it has no branch.
    pub fn is_plain_folder(&self, project: ProjectId) -> bool {
        self.workspaces_of(project)
            .any(|workspace| workspace.kind.is_checkout() && !workspace.kind.in_git())
    }

    pub fn sessions_of(&self, workspace: WorkspaceId) -> impl Iterator<Item = &Session> {
        self.sessions
            .iter()
            .filter(move |session| session.is_in(workspace))
    }

    /// A workspace's sessions as the list shows them: its chats, then its
    /// shells and plugins' panels, which stand by them rather than among
    /// them.
    pub fn listed_of(&self, workspace: WorkspaceId) -> impl Iterator<Item = &Session> {
        let (chats, shells): (Vec<&Session>, Vec<&Session>) = self
            .sessions_of(workspace)
            .partition(|session| !stands_beside(session));
        chats.into_iter().chain(shells)
    }

    /// The agents whose program still runs in `workspace`, oldest first:
    /// another started there would change the same files. A shell, a
    /// plugin's panel and the checks are no agents.
    pub fn agents_alive_in(&self, workspace: WorkspaceId) -> Vec<&Session> {
        let mut agents: Vec<&Session> = self
            .sessions_of(workspace)
            .filter(|session| session.kind().is_agent() && !session.status.is_exited())
            .collect();
        agents.sort_by_key(|session| (session.created_at, session.id));
        agents
    }

    /// What the person is told before starting another agent in
    /// `workspace`, where one is already at work (`PRODUCT.md` §6); the
    /// desktop app says the same (`flows/secondAgent.ts`).
    pub fn second_agent_warning(&self, workspace: WorkspaceId) -> Option<String> {
        let agents = self.agents_alive_in(workspace);
        let who = match agents.as_slice() {
            [] => return None,
            [one] => format!(
                "{} already works here",
                muster_core::text::one_line(&one.title)
            ),
            several => format!("{} agents already work here", several.len()),
        };
        Some(format!(
            "{who}: two agents in one place change the same files. A new worktree keeps their \
             work apart."
        ))
    }

    /// Whether `shell` (or a plugin's panel) is shown as its workspace's
    /// one chat's: a worktree with one agent at work in it, as most are,
    /// has its shell under that agent's chat; where several chats share the
    /// workspace, its shells serve them all, and stand after them.
    pub fn under_its_chat(&self, shell: &Session) -> bool {
        let Some(workspace) = shell.workspace else {
            return false;
        };
        stands_beside(shell)
            && self
                .sessions_of(workspace)
                .filter(|session| !stands_beside(session))
                .count()
                == 1
    }

    /// The sessions list: each project, its worktrees, their sessions,
    /// then the free terminals under their heading.
    pub fn rows(&self) -> Vec<Row> {
        self.rows_in(View::All)
    }

    /// A project's lines: itself, its worktrees, their sessions.
    fn project_rows(&self, project: ProjectId, rows: &mut Vec<Row>) {
        rows.push(Row::Project(project));
        for workspace in self.workspaces_of(project) {
            rows.push(Row::Worktree(workspace.id));
            rows.extend(self.listed_of(workspace.id).map(|s| Row::Session(s.id)));
        }
    }

    /// Sessions waiting on the person: those asking something first, then
    /// those with news not yet seen; oldest first within each.
    pub fn needs_you(&self) -> Vec<SessionId> {
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

    /// The agents at work, the longest at it first: each chat running
    /// with no news for the person, by when it began this stretch of work
    /// (*Working*). A shell or a plugin's panel is not an agent.
    pub fn working(&self) -> Vec<SessionId> {
        let mut working: Vec<&Session> = self
            .sessions
            .iter()
            .filter(|session| {
                session.status == SessionStatus::Running
                    && !stands_beside(session)
                    && !needs_attention(session)
            })
            .collect();
        working.sort_by_key(|session| session.status_since);
        working.into_iter().map(|session| session.id).collect()
    }

    /// How the agents stand, as the top bar counts them: working, waiting
    /// on the person (asking something), and done (finished).
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for session in self.sessions.iter().filter(|s| !stands_beside(s)) {
            match session.status {
                SessionStatus::Running => counts.working += 1,
                SessionStatus::NeedsYou { .. } => counts.waiting += 1,
                SessionStatus::Finished => counts.done += 1,
                SessionStatus::Fresh | SessionStatus::Exited { .. } => {}
            }
        }
        counts
    }

    /// The sessions on a row: a project's in all its workspaces, a
    /// worktree's, or the session itself.
    pub fn sessions_on(&self, row: Row) -> impl Iterator<Item = &Session> {
        self.sessions.iter().filter(move |session| match row {
            Row::Project(project) => session
                .workspace
                .and_then(|workspace| self.workspace(workspace))
                .is_some_and(|workspace| workspace.project == project),
            Row::Worktree(workspace) => session.is_in(workspace),
            Row::Session(id) => session.id == id,
            Row::Terminals => session.is_free(),
        })
    }

    /// The session that says most of how a row stands, in the order *Needs
    /// you* goes: one asking the person something, then one with news not
    /// seen, oldest first; else one at work. `None` while none is.
    pub fn most_urgent(&self, row: Row) -> Option<&Session> {
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
    pub fn project_of(&self, row: Row) -> Option<ProjectId> {
        match row {
            Row::Project(id) => Some(id),
            Row::Worktree(id) => self.workspace(id).map(|workspace| workspace.project),
            Row::Session(id) => self
                .session(id)
                .and_then(|session| self.workspace(session.workspace?))
                .map(|workspace| workspace.project),
            Row::Terminals => None,
        }
    }

    /// The worktree a row belongs to, if any.
    pub fn worktree_of(&self, row: Row) -> Option<WorkspaceId> {
        match row {
            Row::Project(_) | Row::Terminals => None,
            Row::Worktree(id) => Some(id),
            Row::Session(id) => self.session(id).and_then(|session| session.workspace),
        }
    }

    /// What a workspace has checked out.
    pub fn branch(&self, workspace: WorkspaceId) -> Option<&str> {
        self.workspace(workspace).map(|found| found.kind.branch())
    }
}

pub use muster_core::news::needs_attention;

/// Whether `session` stands beside the chats rather than among them: a
/// shell, the person's own, or a plugin's panel, not an agent's chat.
pub fn stands_beside(session: &Session) -> bool {
    session.kind().stands_beside()
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
mod tests {
    use muster_core::WaitReason;

    use super::fixtures::{project, session, worktree};
    use super::*;

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
    fn projects_follow_the_order_the_person_dragged_them_to() {
        let mut first = project("first");
        let mut second = project("second");
        let mut third = project("third");
        (first.created_at, second.created_at, third.created_at) = (
            muster_core::Timestamp::from_millis(1),
            muster_core::Timestamp::from_millis(2),
            muster_core::Timestamp::from_millis(3),
        );
        let names = |model: &Model| -> Vec<String> {
            model.projects().iter().map(|p| p.name.clone()).collect()
        };
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![first.clone(), second.clone(), third.clone()],
            preferences: Preferences {
                // A removed project's id is passed over.
                projects: vec![third.id, ProjectId::new(), first.id],
                ..Preferences::default()
            },
            ..Snapshot::default()
        });
        // Those not in the order come after, as they were added.
        assert_eq!(names(&model), ["third", "first", "second"]);
        let mut fourth = project("fourth");
        fourth.created_at = muster_core::Timestamp::from_millis(4);
        model.apply(Delta::ProjectUpserted(fourth));
        assert_eq!(names(&model), ["third", "first", "second", "fourth"]);
        model.apply(Delta::Preferences(Preferences {
            projects: vec![second.id],
            ..Preferences::default()
        }));
        assert_eq!(names(&model), ["second", "first", "third", "fourth"]);
        // Put back, they are in the order they were added.
        model.apply(Delta::Preferences(Preferences::default()));
        assert_eq!(names(&model), ["first", "second", "third", "fourth"]);
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
    fn another_agent_is_warned_of_only_where_an_agent_still_runs() {
        let app = project("app");
        let tree = worktree(app.id, "work");
        let mut ended = session(tree.id, "Ended", 1);
        ended.status = SessionStatus::Exited { success: true };
        let mut shell = session(tree.id, "zsh", 2);
        shell.harness = "shell".to_owned();
        let mut checks = session(tree.id, "Checks", 3);
        checks.harness = "checks".to_owned();
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app],
            workspaces: vec![tree.clone()],
            sessions: vec![ended, shell, checks],
            ..Snapshot::default()
        });
        assert_eq!(model.second_agent_warning(tree.id), None);

        let mut done = session(tree.id, "Fix the login", 4);
        done.status = SessionStatus::Finished;
        model.apply(Delta::SessionUpserted(done));
        assert_eq!(
            model.second_agent_warning(tree.id).as_deref(),
            Some(
                "Fix the login already works here: two agents in one place change the same \
                 files. A new worktree keeps their work apart."
            )
        );
        model.apply(Delta::SessionUpserted(session(tree.id, "Tidy", 5)));
        assert!(
            model
                .second_agent_warning(tree.id)
                .unwrap()
                .starts_with("2 agents already work here:")
        );
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

    #[test]
    fn working_lists_the_agents_at_work_the_longest_at_it_first() {
        let app = project("app");
        let tree = worktree(app.id, "work");
        let at_work = |title: &str, created: i64, since: i64| {
            let mut found = session(tree.id, title, created);
            found.status = SessionStatus::Running;
            found.status_since = muster_core::Timestamp::from_millis(since);
            found
        };
        let lately = at_work("lately", 1, 300);
        let longest = at_work("longest", 2, 100);
        let between = at_work("between", 3, 200);
        let mut news = at_work("news", 4, 50);
        news.unseen = true;
        let mut shell = at_work("zsh", 5, 10);
        shell.harness = "shell".to_owned();
        let mut asking = session(tree.id, "asking", 6);
        asking.status = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app],
            workspaces: vec![tree],
            sessions: vec![
                lately.clone(),
                longest.clone(),
                between.clone(),
                news,
                shell,
                asking,
            ],
            ..Snapshot::default()
        });
        assert_eq!(model.working(), [longest.id, between.id, lately.id]);
    }

    #[test]
    fn counts_the_agents_by_how_they_stand_but_never_a_shell_or_a_panel() {
        let app = project("app");
        let tree = worktree(app.id, "work");
        let mut working = session(tree.id, "working", 1);
        working.status = SessionStatus::Running;
        let mut asking = session(tree.id, "asking", 2);
        asking.status = SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        };
        let mut done = session(tree.id, "done", 3);
        done.status = SessionStatus::Finished;
        let mut ended = session(tree.id, "ended", 4);
        ended.status = SessionStatus::Exited { success: true };
        let mut shell = session(tree.id, "zsh", 5);
        shell.harness = "shell".to_owned();
        shell.status = SessionStatus::Running;
        let mut panel = session(tree.id, "graph", 6);
        panel.harness = "plugin:zoetrope/graph".to_owned();
        panel.status = SessionStatus::Finished;
        let mut model = Model::default();
        model.replace(Snapshot {
            projects: vec![app],
            workspaces: vec![tree],
            sessions: vec![working, asking, done, ended, shell, panel],
            ..Snapshot::default()
        });
        assert_eq!(
            model.counts(),
            Counts {
                working: 1,
                waiting: 1,
                done: 1
            }
        );
    }
}
