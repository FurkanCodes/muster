//! The Tickets screen: each project's open tickets (GitHub issues, through
//! the person's `gh`), the agents working on each, and assigning more.
//! Muster only reads tickets: nothing changes in the tracker.
//!
//! An agent on a ticket is a Claude Code session in a worktree on a branch
//! named for it (`muster/issue-<n>-<title>`), started with the ticket as
//! its first prompt. ⏎ on a ticket opens the agent on it, or starts the
//! first; `a` starts another, in a worktree of its own. `x` closes the
//! ticket on GitHub, after asking, the one change Muster makes there, and
//! then offers to close its agents and remove its worktrees.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;
use muster_core::{Issue, ProjectId, SessionId};

use super::{Action, App, Confirmed, Dialog, HarnessChoice, Mode, Then, View};

/// One line of the tickets list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TicketRow {
    Project(ProjectId),
    Ticket(ProjectId, u64),
}

/// A project's tickets, as last fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tickets {
    Loading,
    Loaded(Vec<Issue>),
    /// Why they could not be fetched, as `gh` said.
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
            if let Some(Tickets::Loaded(tickets)) = self.tickets.get(&project.id) {
                rows.extend(
                    tickets
                        .iter()
                        .map(|ticket| TicketRow::Ticket(project.id, ticket.number)),
                );
            }
        }
        rows
    }

    /// The ticket a row is, as fetched.
    pub(crate) fn ticket(&self, project: ProjectId, number: u64) -> Option<&Issue> {
        match self.tickets.get(&project)? {
            Tickets::Loaded(tickets) => tickets.iter().find(|ticket| ticket.number == number),
            Tickets::Loading | Tickets::Failed(_) => None,
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
                if let Some(TicketRow::Ticket(project, number)) = self.tickets_selected {
                    return self.assign_agent(project, number, true);
                }
            }
            KeyCode::Char('x') => {
                if let Some(TicketRow::Ticket(project, number)) = self.tickets_selected
                    && let Some(ticket) = self.ticket(project, number)
                {
                    self.mode = Mode::Dialog(Dialog::Confirm {
                        question: format!(
                            "Close #{number} {} on GitHub? Everyone watching it is told.",
                            ticket.title
                        ),
                        then: Confirmed::CloseTicket { project, number },
                    });
                    return Vec::new();
                }
            }
            KeyCode::Char('b') => {
                if let Some(TicketRow::Ticket(project, number)) = self.tickets_selected
                    && let Some(ticket) = self.ticket(project, number)
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
        let Some(TicketRow::Ticket(project, number)) = self.tickets_selected else {
            return Vec::new();
        };
        match self.agent_to_open(project, number) {
            Some(session) => {
                self.mode = Mode::Terminal(session);
                self.note = None;
                self.settle()
            }
            None => self.assign_agent(project, number, false),
        }
    }

    fn agent_to_open(&self, project: ProjectId, number: u64) -> Option<SessionId> {
        let agents = self.model.agents_on(project, number);
        agents
            .iter()
            .rev()
            .find(|session| session.ended_at.is_none())
            .or_else(|| agents.last())
            .map(|session| session.id)
    }

    /// Starts Claude Code on the ticket: in its worktree, or with `another`
    /// in a worktree of its own; it opens as it starts.
    fn assign_agent(&mut self, project: ProjectId, number: u64, another: bool) -> Vec<Action> {
        self.say(format!("Starting an agent on #{number}…"));
        let command = Command::StartFromIssue {
            project,
            number,
            harness: HarnessChoice::Claude.id().to_owned(),
            size: self.session_size(),
            another,
        };
        vec![self.command(command, Then::OpenIssueSession)]
    }

    /// A ticket closed on GitHub leaves the list; if work on it is still
    /// about, the person is offered to clean up after it.
    pub(super) fn ticket_closed(&mut self, project: ProjectId, number: u64) {
        if let Some(Tickets::Loaded(tickets)) = self.tickets.get_mut(&project) {
            tickets.retain(|ticket| ticket.number != number);
        }
        self.ticket_texts.remove(&(project, number));
        let worktrees = self.model.worktrees_for(project, number).len();
        if worktrees == 0 {
            self.say(format!("Closed #{number} on GitHub."));
            return;
        }
        let agents = self.model.agents_on(project, number).len();
        let what = match (agents, worktrees) {
            (0, 1) => "remove its worktree".to_owned(),
            (0, n) => format!("remove its {n} worktrees"),
            (1, 1) => "close its agent and remove its worktree".to_owned(),
            (a, 1) => format!("close its {a} agents and remove its worktree"),
            (a, n) => format!("close its {a} agents and remove its {n} worktrees"),
        };
        self.mode = Mode::Dialog(Dialog::Confirm {
            question: format!("Closed #{number} on GitHub. Also {what}? Branches are kept."),
            then: Confirmed::CleanUpTicket { project, number },
        });
    }

    /// Stops a closed ticket's agents and removes its worktrees once
    /// nothing runs in them.
    pub(super) fn clean_up_ticket(&mut self, project: ProjectId, number: u64) -> Vec<Action> {
        let worktrees = self.model.worktrees_for(project, number);
        let running: Vec<SessionId> = self
            .model
            .agents_on(project, number)
            .iter()
            .filter(|session| session.ended_at.is_none())
            .map(|session| session.id)
            .collect();
        self.say(format!("Cleaning up after #{number}…"));
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
        if let Some(TicketRow::Ticket(project, number)) = self.tickets_selected
            && !self.ticket_texts.contains_key(&(project, number))
        {
            self.ticket_texts
                .insert((project, number), TicketText::Loading);
            actions.push(self.command(
                Command::ViewIssue { project, number },
                Then::TicketText(project, number),
            ));
        }
        actions
    }

    /// Keeps the tickets selection on a line that exists.
    pub(super) fn keep_ticket_selection(&mut self) {
        let rows = self.ticket_rows();
        let kept = self.tickets_selected.filter(|row| rows.contains(row));
        self.tickets_selected = kept.or_else(|| {
            // A ticket that went (closed, or fetched again): its project.
            match self.tickets_selected {
                Some(TicketRow::Ticket(project, _)) => Some(TicketRow::Project(project)),
                _ => None,
            }
            .filter(|row| rows.contains(row))
            .or_else(|| rows.first().copied())
        });
    }
}
