//! The Tickets screen: each project's open tickets, from its tracker
//! (GitHub or GitLab issues, Jira, Linear), the agents working on each, and
//! assigning more. Muster only reads tickets, but for closing one.
//!
//! An agent on a ticket is a session in a worktree on a branch named for it
//! (`muster/issue-<key>-<title>`), started with the ticket as its first
//! prompt. ⏎ on a ticket opens the agent on it, or starts the first; `a`
//! starts another, in a worktree of its own. `x` closes the ticket in its
//! tracker, after asking, the one change Muster makes there, and then
//! offers to close its agents and remove its worktrees.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;
use muster_core::{Issue, ProjectId, SessionId, TicketKey};

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
    /// The tickets, where they came from (`GitHub issues`, `Jira
    /// (acme.atlassian.net)`), and whether they are only those assigned to
    /// the person.
    Loaded(Vec<Issue>, String, bool),
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
    /// The tickets list: each project, then its tickets as fetched.
    pub(crate) fn ticket_rows(&self) -> Vec<TicketRow> {
        let mut rows = Vec::new();
        for project in self.model.projects() {
            rows.push(TicketRow::Project(project.id));
            if let Some(Tickets::Loaded(tickets, ..)) = self.tickets.get(&project.id) {
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
            Tickets::Loaded(tickets, ..) => tickets.iter().find(|ticket| ticket.key == *key),
            Tickets::Loading | Tickets::Failed(_) => None,
        }
    }

    /// Where a project's tickets come from, once fetched.
    pub(crate) fn tracker_of(&self, project: ProjectId) -> &str {
        match self.tickets.get(&project) {
            Some(Tickets::Loaded(_, tracker, _)) if !tracker.is_empty() => tracker,
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
    pub(super) fn tickets_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Char('r') => {
                // Everything is fetched again as it is shown.
                self.tickets.clear();
                self.ticket_texts.clear();
                self.say("Fetching the tickets again…");
            }
            KeyCode::Enter => return self.open_ticket(),
            KeyCode::Char('a') => {
                if let Some((project, key)) = self.selected_ticket() {
                    return self.choose_agent(project, key, true);
                }
            }
            KeyCode::Char('x') => {
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
            KeyCode::Char('b') => {
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
        let agents = self.model.agents_on(project, key);
        agents
            .iter()
            .rev()
            .find(|session| session.ended_at.is_none())
            .or_else(|| agents.last())
            .map(|session| session.id)
    }

    /// Asks which agent to start on the ticket, offering the one last
    /// started on a ticket first.
    fn choose_agent(&mut self, project: ProjectId, key: TicketKey, another: bool) -> Vec<Action> {
        self.note = None;
        self.mode = Mode::Dialog(Dialog::AssignAgent {
            project,
            key,
            harness: self.ticket_agent,
            another,
        });
        Vec::new()
    }

    /// A key in the dialog choosing an agent for a ticket: ⇥ and ⇧⇥ (or
    /// the arrows) choose, ⏎ starts it, esc thinks better of it.
    pub(super) fn assign_key(
        &mut self,
        project: ProjectId,
        key: TicketKey,
        harness: HarnessChoice,
        another: bool,
        pressed: &KeyEvent,
    ) -> Vec<Action> {
        let harness = match pressed.code {
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => harness.next_agent(),
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => harness.previous_agent(),
            KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.ticket_agent = harness;
                return self.assign_agent(project, key, harness, another);
            }
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                return Vec::new();
            }
            _ => harness,
        };
        self.mode = Mode::Dialog(Dialog::AssignAgent {
            project,
            key,
            harness,
            another,
        });
        Vec::new()
    }

    /// Starts `harness` on the ticket: in its worktree, or with `another`
    /// in a worktree of its own; it opens as it starts.
    fn assign_agent(
        &mut self,
        project: ProjectId,
        key: TicketKey,
        harness: HarnessChoice,
        another: bool,
    ) -> Vec<Action> {
        self.say(format!("Starting {} on {}…", harness.label(), key.shown()));
        let command = Command::StartFromIssue {
            project,
            key,
            harness: harness.id().to_owned(),
            size: self.session_size(),
            another,
        };
        vec![self.command(command, Then::OpenIssueSession)]
    }

    /// A ticket closed in its tracker leaves the list; if work on it is
    /// still about, the person is offered to clean up after it.
    pub(super) fn ticket_closed(&mut self, project: ProjectId, key: TicketKey) {
        let tracker = self.tracker_of(project).to_owned();
        if let Some(Tickets::Loaded(tickets, ..)) = self.tickets.get_mut(&project) {
            tickets.retain(|ticket| ticket.key != key);
        }
        self.ticket_texts.remove(&(project, key.clone()));
        let worktrees = self.model.worktrees_for(project, &key).len();
        if worktrees == 0 {
            self.say(format!("Closed {} in {tracker}.", key.shown()));
            return;
        }
        let agents = self.model.agents_on(project, &key).len();
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
        let worktrees = self.model.worktrees_for(project, key);
        let running: Vec<SessionId> = self
            .model
            .agents_on(project, key)
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

    /// Keeps the tickets selection on a line that exists.
    pub(super) fn keep_ticket_selection(&mut self) {
        let rows = self.ticket_rows();
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
