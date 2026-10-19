//! The Tickets screen: each project's open tickets, from its tracker
//! (GitHub or GitLab issues, Jira, Linear), the agents working on each, and
//! assigning more. Muster only reads tickets, but for closing one.
//! Projects whose tickets are in one place share them (`sharing`): they
//! are listed once, and an agent starts in the one the person chooses.
//!
//! An agent on a ticket is a session in a worktree on a branch named for it
//! (`muster/issue-<key>-<title>`), started with the ticket as its first
//! prompt. ⏎ on a ticket opens the agent on it, or starts the first; `a`
//! starts another, in a worktree of its own. `x` closes the ticket in its
//! tracker, after asking, the one change Muster makes there, and then
//! offers to close its agents and remove its worktrees.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::keys::Action as Does;
use muster_core::protocol::Command;
use muster_core::{Issue, ProjectId, SessionId, TicketKey, WorkspaceId};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, Then, View};

/// One line of the tickets list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TicketRow {
    Project(ProjectId),
    Ticket(ProjectId, TicketKey),
}

/// A project's tickets, as last fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tickets {
    Loading,
    Loaded {
        tickets: Vec<Issue>,
        /// Where they came from, for the person: `GitHub issues`, `Jira
        /// (acme.atlassian.net)`.
        tracker: String,
        /// Whether they are only those assigned to the person.
        mine: bool,
        /// Where they came from, the same for projects sharing them.
        source: Option<String>,
    },
    /// Why they could not be fetched, as the tracker said.
    Failed(String),
}

/// A ticket's text, fetched when it is first shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TicketText {
    Loading,
    Loaded(String),
    Failed(String),
}

impl App {
    /// The tickets list: each project, then its tickets as fetched;
    /// projects sharing their tickets under the first one's heading.
    pub(crate) fn ticket_rows(&self) -> Vec<TicketRow> {
        let mut rows = Vec::new();
        for project in self.model.projects() {
            if self.lead(project.id) != project.id {
                continue;
            }
            rows.push(TicketRow::Project(project.id));
            if let Some(Tickets::Loaded { tickets, .. }) = self.tickets.get(&project.id) {
                rows.extend(
                    tickets
                        .iter()
                        .map(|ticket| TicketRow::Ticket(project.id, ticket.key.clone())),
                );
            }
        }
        rows
    }

    /// The ticket a row is, as fetched.
    pub(crate) fn ticket(&self, project: ProjectId, key: &TicketKey) -> Option<&Issue> {
        match self.tickets.get(&project)? {
            Tickets::Loaded { tickets, .. } => tickets.iter().find(|ticket| ticket.key == *key),
            Tickets::Loading | Tickets::Failed(_) => None,
        }
    }

    /// Where a project's tickets come from, once fetched.
    pub(crate) fn tracker_of(&self, project: ProjectId) -> &str {
        match self.tickets.get(&project) {
            Some(Tickets::Loaded { tracker, .. }) if !tracker.is_empty() => tracker,
            _ => "its tracker",
        }
    }

    /// The selected ticket, if a ticket is selected.
    fn selected_ticket(&self) -> Option<(ProjectId, TicketKey)> {
        match &self.tickets_selected {
            Some(TicketRow::Ticket(project, key)) => Some((*project, key.clone())),
            _ => None,
        }
    }

    /// Shows the Tickets screen, on `project`'s tickets if one is given.
    pub(super) fn show_tickets(&mut self, project: Option<ProjectId>) {
        self.view = View::Tickets;
        if let Some(project) = project {
            self.tickets_selected = Some(TicketRow::Project(project));
        }
    }

    /// A key on the Tickets screen; the rest are inert here.
    pub(super) fn tickets_key(&mut self, key: &KeyEvent, does: Option<Does>) -> Vec<Action> {
        match (key.code, does) {
            (KeyCode::Up, _) | (_, Some(Does::MoveUp)) => self.move_selection(-1),
            (KeyCode::Down, _) | (_, Some(Does::MoveDown)) => self.move_selection(1),
            (_, Some(Does::RefreshTickets)) => {
                // Everything is fetched again as it is shown.
                self.tickets.clear();
                self.ticket_texts.clear();
                self.say("Fetching the tickets again…");
            }
            (KeyCode::Enter, _) => return self.open_ticket(),
            (_, Some(Does::AnotherAgent)) => {
                if let Some((project, key)) = self.selected_ticket() {
                    return self.choose_agent(project, key, true);
                }
            }
            (_, Some(Does::CloseTicket)) => {
                if let Some((project, key)) = self.selected_ticket()
                    && let Some(ticket) = self.ticket(project, &key)
                {
                    self.mode = Mode::Dialog(Dialog::Confirm {
                        question: format!(
                            "Close {} {} in {}? Everyone watching it is told.",
                            key.shown(),
                            ticket.title,
                            self.tracker_of(project)
                        ),
                        then: Confirmed::CloseTicket { project, key },
                    });
                    return Vec::new();
                }
            }
            (_, Some(Does::TicketInBrowser)) => {
                if let Some((project, key)) = self.selected_ticket()
                    && let Some(ticket) = self.ticket(project, &key)
                {
                    return vec![Action::Open(ticket.url.clone())];
                }
            }
            _ => {}
        }
        self.settle()
    }

    /// ⏎ on a ticket: its agent's terminal (the latest still running, else
    /// the latest), or the first agent on it.
    fn open_ticket(&mut self) -> Vec<Action> {
        let Some((project, key)) = self.selected_ticket() else {
            return Vec::new();
        };
        match self.agent_to_open(project, &key) {
            Some(session) => {
                self.mode = Mode::Terminal(session);
                self.note = None;
                self.settle()
            }
            None => self.choose_agent(project, key, false),
        }
    }

    fn agent_to_open(&self, project: ProjectId, key: &TicketKey) -> Option<SessionId> {
        let agents = self.ticket_agents(project, key);
        agents
            .iter()
            .rev()
            .find(|session| session.ended_at.is_none())
            .or_else(|| agents.last())
            .map(|session| session.id)
    }

    /// Asks which agent to start on the ticket, offering the one last
    /// started on a ticket first, and, where several projects share it,
    /// in which, offering the one last chosen for tickets from there.
    fn choose_agent(&mut self, project: ProjectId, key: TicketKey, another: bool) -> Vec<Action> {
        self.note = None;
        self.mode = Mode::Dialog(Dialog::AssignAgent {
            project,
            key,
            harness: self.ticket_agent.clone(),
            another,
            into: self.project_to_offer(project),
            on: None,
        });
        Vec::new()
    }

    /// Whether an agent asked for as `another` on ticket `key` starts in a
    /// worktree of its own in `into`: only if the ticket has one there
    /// already; the first there starts in the ticket's worktree.
    pub(crate) fn starts_another(&self, into: ProjectId, key: &TicketKey, another: bool) -> bool {
        another && !self.model.worktrees_for(&[into], key).is_empty()
    }

    /// A key in the dialog choosing an agent for a ticket: ⇥ and ⇧⇥
    /// choose the agent, ← → what a new worktree is made on top of where
    /// one is offered (else the agent too), ↑ ↓ the project where several
    /// share the ticket, ⏎ starts it, esc thinks better of it.
    pub(super) fn assign_key(
        &mut self,
        project: ProjectId,
        key: TicketKey,
        harness: HarnessChoice,
        (another, into, on): (bool, ProjectId, Option<WorkspaceId>),
        pressed: &KeyEvent,
    ) -> Vec<Action> {
        let stacking = self.offers_stack(into, &key, another);
        let (harness, into, on) = match pressed.code {
            KeyCode::Right | KeyCode::Char('l') if stacking => {
                (harness, into, self.next_stack(into, on, false))
            }
            KeyCode::Left | KeyCode::Char('h') if stacking => {
                (harness, into, self.next_stack(into, on, true))
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
                (harness.next_agent(self.added_agents()), into, on)
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
                (harness.previous_agent(self.added_agents()), into, on)
            }
            // Another project's worktrees are its own to stack on.
            KeyCode::Down | KeyCode::Char('j') => {
                (harness, self.next_project(project, into, false), None)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                (harness, self.next_project(project, into, true), None)
            }
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.ticket_agent = harness.clone();
                self.remember_project(project, into);
                let another = self.starts_another(into, &key, another);
                let on = on.filter(|_| stacking);
                return self.assign_agent(into, key, &harness, another, on);
            }
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                return Vec::new();
            }
            _ => (harness, into, on),
        };
        self.mode = Mode::Dialog(Dialog::AssignAgent {
            project,
            key,
            harness,
            another,
            into,
            on,
        });
        Vec::new()
    }

    /// Starts `harness` on the ticket in `project`: in its worktree, or
    /// with `another` in a worktree of its own; it opens as it starts.
    fn assign_agent(
        &mut self,
        project: ProjectId,
        key: TicketKey,
        harness: &HarnessChoice,
        another: bool,
        on: Option<WorkspaceId>,
    ) -> Vec<Action> {
        self.say(format!("Starting {} on {}…", harness.label(), key.shown()));
        let command = Command::StartFromIssue {
            project,
            key,
            harness: harness.id().to_owned(),
            size: self.session_size(),
            another,
            on,
        };
        vec![self.command(command, Then::OpenIssueSession)]
    }

    /// A ticket closed in its tracker leaves the list; if work on it is
    /// still about, the person is offered to clean up after it.
    pub(super) fn ticket_closed(&mut self, project: ProjectId, key: TicketKey) {
        let tracker = self.tracker_of(project).to_owned();
        let worktrees = self.ticket_worktrees(project, &key).len();
        let agents = self.ticket_agents(project, &key).len();
        for sharing in self.sharing(project) {
            if let Some(Tickets::Loaded { tickets, .. }) = self.tickets.get_mut(&sharing) {
                tickets.retain(|ticket| ticket.key != key);
            }
            self.ticket_texts.remove(&(sharing, key.clone()));
        }
        if worktrees == 0 {
            self.say(format!("Closed {} in {tracker}.", key.shown()));
            return;
        }
        let what = match (agents, worktrees) {
            (0, 1) => "remove its worktree".to_owned(),
            (0, n) => format!("remove its {n} worktrees"),
            (1, 1) => "close its agent and remove its worktree".to_owned(),
            (a, 1) => format!("close its {a} agents and remove its worktree"),
            (a, n) => format!("close its {a} agents and remove its {n} worktrees"),
        };
        self.mode = Mode::Dialog(Dialog::Confirm {
            question: format!(
                "Closed {} in {tracker}. Also {what}? Branches are kept.",
                key.shown()
            ),
            then: Confirmed::CleanUpTicket { project, key },
        });
    }

    /// Stops a closed ticket's agents and removes its worktrees once
    /// nothing runs in them.
    pub(super) fn clean_up_ticket(&mut self, project: ProjectId, key: &TicketKey) -> Vec<Action> {
        let worktrees = self.ticket_worktrees(project, key);
        let running: Vec<SessionId> = self
            .ticket_agents(project, key)
            .iter()
            .filter(|session| session.ended_at.is_none())
            .map(|session| session.id)
            .collect();
        self.say(format!("Cleaning up after {}…", key.shown()));
        self.removing.extend(worktrees);
        running
            .into_iter()
            .map(|session| self.command(Command::StopSession { session }, Then::Closing(session)))
            .collect()
    }

    /// Removes the worktrees being cleaned up as soon as nothing runs in
    /// them; one that has uncommitted changes is asked about.
    pub(super) fn remove_idle_worktrees(&mut self) -> Vec<Action> {
        let pending = std::mem::take(&mut self.removing);
        let model = &self.model;
        let (idle, busy): (Vec<_>, Vec<_>) = pending
            .into_iter()
            .filter(|workspace| model.workspace(*workspace).is_some())
            .partition(|workspace| {
                model
                    .sessions_of(*workspace)
                    .all(|session| session.ended_at.is_some())
            });
        self.removing = busy;
        idle.into_iter()
            .map(|workspace| {
                self.command(
                    Command::RemoveWorktree {
                        workspace,
                        force: false,
                    },
                    Then::RemovedWorktree(workspace),
                )
            })
            .collect()
    }

    /// Forgets the tickets and texts that could not be fetched, so they are
    /// fetched again as they are shown: the person has come back to them,
    /// maybe with the VPN their tracker needs turned on.
    pub(super) fn forget_failed_tickets(&mut self) {
        self.tickets
            .retain(|_, tickets| !matches!(tickets, Tickets::Failed(_)));
        self.ticket_texts
            .retain(|_, text| !matches!(text, TicketText::Failed(_)));
    }

    /// Asks for the tickets of projects shown without them, and for the
    /// text of the ticket in focus, while the Tickets screen is up.
    pub(super) fn fetch_tickets(&mut self) -> Vec<Action> {
        if self.view != View::Tickets || !matches!(self.mode, Mode::Browse | Mode::Dialog(_)) {
            return Vec::new();
        }
        let wanted: Vec<ProjectId> = self
            .model
            .projects()
            .iter()
            .map(|project| project.id)
            .filter(|project| !self.tickets.contains_key(project))
            .collect();
        let mut actions = Vec::new();
        for project in wanted {
            self.tickets.insert(project, Tickets::Loading);
            actions.push(self.command(Command::ListIssues { project }, Then::Tickets(project)));
        }
        if let Some((project, key)) = self.selected_ticket()
            && !self.ticket_texts.contains_key(&(project, key.clone()))
        {
            self.ticket_texts
                .insert((project, key.clone()), TicketText::Loading);
            actions.push(self.command(
                Command::ViewIssue {
                    project,
                    key: key.clone(),
                },
                Then::TicketText(project, key),
            ));
        }
        actions
    }

    /// Keeps the tickets selection on a line that exists: one of a
    /// project sharing its tickets is the same line under the first's.
    pub(super) fn keep_ticket_selection(&mut self) {
        let rows = self.ticket_rows();
        self.tickets_selected = self.tickets_selected.take().map(|row| match row {
            TicketRow::Project(project) => TicketRow::Project(self.lead(project)),
            TicketRow::Ticket(project, key) => TicketRow::Ticket(self.lead(project), key),
        });
        let kept = self
            .tickets_selected
            .clone()
            .filter(|row| rows.contains(row));
        self.tickets_selected = kept.or_else(|| {
            // A ticket that went (closed, or fetched again): its project.
            match &self.tickets_selected {
                Some(TicketRow::Ticket(project, _)) => Some(TicketRow::Project(*project)),
                _ => None,
            }
            .filter(|row| rows.contains(row))
            .or_else(|| rows.first().cloned())
        });
    }
}
