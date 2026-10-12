//! The UI's state machine. It takes key presses, pastes and frames from the
//! daemon, and returns the frames to send. It does no IO, so every behaviour
//! can be tested by feeding it events and drawing it into a test buffer.

mod events;
mod harness_choice;
mod motion;
mod picker;
mod pointer;
mod tickets;

pub(crate) use harness_choice::HarnessChoice;
pub(crate) use picker::{FolderPicker, PICKER_ROWS, Shown, picker_filter};
pub(crate) use tickets::{TicketRow, TicketText, Tickets};

#[cfg(test)]
mod by_hand_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod folder_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod motion_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod picker_tests;
#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tickets_tests;

use std::collections::HashMap;
use std::path::PathBuf;

use muster_core::protocol::{
    ClientFrame, Command, Delta, DetachReason, ErrorBody, ErrorCode, Reply, ServerFrame, TermSize,
};
use muster_core::{ProjectId, SessionId, WorkspaceId};

use crate::input::LineInput;
use crate::model::{Model, Row, needs_attention};
use crate::screen::Screen;

/// What the runtime must do for the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Send(ClientFrame),
    /// Tell the person, who is looking elsewhere: a desktop notification
    /// where the terminal shows one, and its bell.
    Notify(String),
    /// Open a web page in the person's browser.
    Open(String),
    /// List the folders in this folder, for the folder picker
    /// ([`App::on_folders`]).
    ListFolders(PathBuf),
    Quit,
}

/// The state of the link to the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Link {
    Connecting,
    Connected {
        daemon_version: String,
    },
    /// The daemon went away; the runtime is reconnecting.
    Lost(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum View {
    Sessions,
    NeedsYou,
    Tickets,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Browse,
    /// A session's terminal fills the screen and keys go to it.
    Terminal(SessionId),
    Dialog(Dialog),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Dialog {
    AddProject {
        path: LineInput,
        picker: FolderPicker,
    },
    NewWorktree {
        project: ProjectId,
        branch: LineInput,
    },
    NewSession {
        workspace: WorkspaceId,
        harness: HarnessChoice,
        prompt: LineInput,
    },
    Confirm {
        question: String,
        then: Confirmed,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Confirmed {
    StopSession(SessionId),
    /// Stop it and take it off the list once it has ended.
    CloseSession(SessionId),
    /// Close the ticket on GitHub.
    CloseTicket {
        project: ProjectId,
        number: u64,
    },
    /// Close a closed ticket's agents and remove its worktrees.
    CleanUpTicket {
        project: ProjectId,
        number: u64,
    },
    RemoveWorktree {
        workspace: WorkspaceId,
        force: bool,
    },
    RemoveProject(ProjectId),
}

/// A line under the screen telling the person what just happened.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) text: String,
    pub(crate) error: bool,
}

/// What to do with a command's reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Then {
    SelectProject,
    SelectWorktree,
    OpenSession,
    /// Open the restarted session, streaming its new program from the start.
    OpenRestarted,
    /// Offer to force the removal if it was refused for uncommitted changes.
    RemovedWorktree(WorkspaceId),
    /// Keep the project's tickets.
    Tickets(ProjectId),
    /// Keep the ticket's text.
    TicketText(ProjectId, u64),
    OpenIssueSession,
    /// Say which pull request was opened.
    ShowPullRequest,
    /// A session being closed was asked to stop; if it cannot be, it is
    /// not closed either.
    Closing(SessionId),
    /// Drop the ticket from the list, and offer to clean up after it.
    TicketClosed(ProjectId, u64),
    Say(&'static str),
}

#[derive(Debug)]
pub(crate) struct App {
    pub(crate) model: Model,
    pub(crate) view: View,
    pub(crate) mode: Mode,
    pub(crate) link: Link,
    pub(crate) note: Option<Note>,
    /// The selected line of the sessions list, by identity.
    pub(crate) selected: Option<Row>,
    /// Where the selected line was, so the selection stays nearby when
    /// that line goes away.
    selected_at: usize,
    /// The first line shown of the sessions list and of the Needs you
    /// queue: the view moves only when the selection would leave it.
    pub(crate) list_offset: usize,
    pub(crate) needs_offset: usize,
    /// The Needs you queue as shown: a session stays in place while it is
    /// selected, even once looking at it has cleared it, so the list does
    /// not move under the person.
    pub(crate) needs_list: Vec<SessionId>,
    /// The selected session of the Needs you queue.
    pub(crate) needs_selected: Option<SessionId>,
    /// Each project's tickets, as last fetched; fetched again on `r`.
    pub(crate) tickets: HashMap<ProjectId, Tickets>,
    /// The selected line of the tickets list, and its first line shown.
    pub(crate) tickets_selected: Option<TicketRow>,
    pub(crate) tickets_offset: usize,
    /// Tickets' text, fetched as each is first shown.
    pub(crate) ticket_texts: HashMap<(ProjectId, u64), TicketText>,
    pub(crate) screens: HashMap<SessionId, Screen>,
    /// The session whose terminal is streamed to this client.
    attached: Option<SessionId>,
    /// Something a reply named that the state may not show yet: replies and
    /// deltas travel separately, so either can arrive first. Selected (and,
    /// for a new session, opened) as soon as it appears.
    wanted: Option<(Row, bool)>,
    pending: HashMap<u64, Then>,
    next_req: u64,
    /// The window below the top bar: a session filling the window is drawn
    /// at this size; beside the list, at the pane's (`session_size`).
    pub(crate) terminal_size: TermSize,
    /// Whether the session with the keys fills the window, rather than
    /// sitting beside the list.
    pub(crate) full_screen: bool,
    /// Sessions closed while running: asked to stop, and taken off the
    /// list as soon as they have ended.
    closing: Vec<SessionId>,
    /// Worktrees to remove as soon as nothing runs in them: a closed
    /// ticket's, being cleaned up after.
    removing: Vec<WorkspaceId>,
    /// Where `muster` was started, offered when adding a project.
    cwd: PathBuf,
    /// Whether the person's terminal window has focus, once it has said;
    /// terminals that never say count as looking.
    pub(crate) focused: Option<bool>,
    /// Whether to notify the person of news while they look elsewhere.
    pub(crate) notify: bool,
    /// Whether anything moves: running sessions' marks spin and what
    /// needs the person pulses.
    pub(crate) motion: bool,
    /// The step of the motion to draw, counted out by the runtime.
    pub(crate) step: u64,
    /// Sessions whose lines flash for a change of status, with the step
    /// each flash began.
    flashes: HashMap<SessionId, u64>,
    /// The harness last started from the new-session dialog, which it
    /// offers first next time.
    last_harness: HarnessChoice,
}

impl App {
    pub(crate) fn new(cwd: PathBuf, terminal_size: TermSize) -> Self {
        Self {
            model: Model::default(),
            view: View::Sessions,
            mode: Mode::Browse,
            link: Link::Connecting,
            note: None,
            selected: None,
            selected_at: 0,
            list_offset: 0,
            needs_offset: 0,
            needs_list: Vec::new(),
            needs_selected: None,
            tickets: HashMap::new(),
            tickets_selected: None,
            tickets_offset: 0,
            ticket_texts: HashMap::new(),
            screens: HashMap::new(),
            attached: None,
            wanted: None,
            pending: HashMap::new(),
            next_req: 1,
            terminal_size,
            full_screen: false,
            closing: Vec::new(),
            removing: Vec::new(),
            cwd,
            focused: None,
            notify: true,
            motion: true,
            step: 0,
            flashes: HashMap::new(),
            last_harness: HarnessChoice::Claude,
        }
    }

    // The link to the daemon.

    pub(crate) fn on_connected(&mut self, daemon_version: String) -> Vec<Action> {
        self.link = Link::Connected { daemon_version };
        self.keep_in_view();
        self.attached = None;
        self.pending.clear();
        // Asked of the last daemon, never answered: asked again.
        self.forget_unanswered();
        // A new connection is a new daemon, which numbers its terminals'
        // output afresh: positions in the old screens mean nothing to it.
        self.screens.clear();
        vec![Action::Send(ClientFrame::Subscribe)]
    }

    pub(crate) fn on_disconnected(&mut self, reason: String) {
        self.link = Link::Lost(reason);
        self.attached = None;
        self.pending.clear();
        self.forget_unanswered();
        // The reason takes lines from the list.
        self.keep_in_view();
    }

    // Frames from the daemon.

    pub(crate) fn on_frame(&mut self, frame: ServerFrame) -> Vec<Action> {
        let mut news = None;
        match frame {
            ServerFrame::Snapshot(snapshot) => self.model.replace(snapshot),
            ServerFrame::Delta(delta) => {
                let session = match &delta {
                    Delta::SessionStatus { session, .. } => Some(*session),
                    Delta::SessionUpserted(session) => Some(session.id),
                    _ => None,
                };
                let before = session.and_then(|id| self.model.session(id).map(needs_attention));
                let status_before =
                    session.and_then(|id| self.model.session(id).map(|stored| stored.status));
                let was_ended = session
                    .and_then(|id| self.model.session(id))
                    .is_some_and(|stored| stored.ended_at.is_some());
                self.model.apply(delta);
                if before == Some(false) {
                    news = session.and_then(|id| self.news(id));
                }
                if let Some(id) = session {
                    self.reattach_if_reopened(id, was_ended);
                    if let Some(status) = status_before {
                        self.flash_if_changed(id, status);
                    }
                }
            }
            ServerFrame::Reply { req_id, outcome } => return self.on_reply(req_id, outcome),
            ServerFrame::Replay {
                session,
                seq,
                data,
                reset,
            } => self.screen(session).replay(seq, &data, reset),
            ServerFrame::Output { session, seq, data } => self.screen(session).output(seq, &data),
            ServerFrame::Modes { session, modes } => self.screen(session).modes = modes,
            ServerFrame::Detached { session, reason } => {
                self.screen(session).ended = Some(reason);
            }
            ServerFrame::Welcome(_) | ServerFrame::Incompatible(_) | ServerFrame::Error(_) => {}
        }
        let mut actions: Vec<Action> = news.into_iter().collect();
        actions.extend(self.settle());
        actions
    }

    /// A session that had ended reopened (restarted, maybe by another
    /// client). A stream the daemon ended because it held no terminal for
    /// it does not come back on its own, so attach again.
    fn reattach_if_reopened(&mut self, session: SessionId, was_ended: bool) {
        let reopened = was_ended
            && self
                .model
                .session(session)
                .is_some_and(|stored| stored.ended_at.is_none());
        let stream_gone = self
            .screens
            .get(&session)
            .is_some_and(|screen| screen.ended == Some(DetachReason::NotRunning));
        if reopened && stream_gone && self.attached == Some(session) {
            self.attached = None;
        }
    }

    /// A notification for a session that has just come to need the
    /// person, if they are looking elsewhere and want to be told.
    fn news(&self, session: SessionId) -> Option<Action> {
        if !self.notify || self.focused != Some(false) {
            return None;
        }
        let stored = self.model.session(session)?;
        if !needs_attention(stored) {
            return None;
        }
        let what = match stored.status {
            muster_core::SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Permission,
            } => "needs your permission",
            muster_core::SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Question,
            } => "has a question for you",
            muster_core::SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Startup,
            } => "is asking something before it starts",
            muster_core::SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Action,
            } => "needs you",
            muster_core::SessionStatus::Exited { success: false } => "exited with an error",
            muster_core::SessionStatus::Exited { success: true } => "exited",
            _ => "finished",
        };
        let place = self
            .model
            .branch(stored.workspace)
            .map(|branch| format!(" ({branch})"))
            .unwrap_or_default();
        Some(Action::Notify(format!("{}{place} {what}", stored.title)))
    }

    /// The person's terminal window gained or lost focus. Looking away
    /// stops watching the attached session, so what happens meanwhile is
    /// news; looking back watches it again.
    pub(crate) fn on_focus(&mut self, focused: bool) -> Vec<Action> {
        self.focused = Some(focused);
        match self.attached {
            Some(session) if matches!(self.link, Link::Connected { .. }) => {
                vec![Action::Send(ClientFrame::Watch {
                    session,
                    watching: focused,
                })]
            }
            _ => Vec::new(),
        }
    }

    fn on_reply(&mut self, req_id: u64, outcome: Result<Reply, ErrorBody>) -> Vec<Action> {
        let Some(then) = self.pending.remove(&req_id) else {
            return Vec::new();
        };
        match (outcome, then) {
            (Ok(Reply::ProjectAdded { id }), Then::SelectProject) => {
                self.wanted = Some((Row::Project(id), false));
                self.say("Added the project.");
            }
            (Ok(Reply::WorktreeCreated { id }), Then::SelectWorktree) => {
                self.wanted = Some((Row::Worktree(id), false));
                self.say("Created the worktree.");
            }
            (Ok(Reply::SessionStarted { id }), Then::OpenSession)
            | (Ok(Reply::IssueStarted { session: id, .. }), Then::OpenIssueSession) => {
                self.wanted = Some((Row::Session(id), true));
                self.note = None;
            }
            (Ok(Reply::SessionStarted { id }), Then::OpenRestarted) => {
                // The stream follows the session to its new program; one the
                // daemon had no terminal for is attached again once the
                // state says it reopened (see `on_frame`).
                self.wanted = Some((Row::Session(id), true));
                self.note = None;
            }
            (Ok(Reply::PullRequestOpened { pr }), Then::ShowPullRequest) => {
                self.say(format!(
                    "Draft pull request #{} is open: {}",
                    pr.number, pr.url
                ));
            }
            (Ok(Reply::Issues { issues }), Then::Tickets(project)) => {
                self.tickets.insert(project, Tickets::Loaded(issues));
            }
            (Err(error), Then::Tickets(project)) => {
                self.tickets.insert(project, Tickets::Failed(error.message));
            }
            (Ok(Reply::IssueViewed { body, .. }), Then::TicketText(project, number)) => {
                self.ticket_texts
                    .insert((project, number), TicketText::Loaded(body));
            }
            (Err(error), Then::TicketText(project, number)) => {
                self.ticket_texts
                    .insert((project, number), TicketText::Failed(error.message));
            }
            (Ok(_), Then::RemovedWorktree(_)) => {
                self.say("Removed the worktree; its branch is kept.")
            }
            (Ok(_), Then::Say(text)) => self.say(text),
            (Ok(_), Then::TicketClosed(project, number)) => self.ticket_closed(project, number),
            (Err(error), Then::Closing(session)) => {
                self.closing.retain(|closing| *closing != session);
                self.complain(error.message);
            }
            (Ok(_), _) => {}
            (Err(error), Then::RemovedWorktree(workspace))
                if error.code == ErrorCode::Conflict && error.message.contains("uncommitted") =>
            {
                self.mode = Mode::Dialog(Dialog::Confirm {
                    question: "It has uncommitted changes. Remove it anyway and discard them?"
                        .to_owned(),
                    then: Confirmed::RemoveWorktree {
                        workspace,
                        force: true,
                    },
                });
            }
            (Err(error), _) => self.complain(error.message),
        }
        self.settle()
    }

    // Commands.

    fn command(&mut self, command: Command, then: Then) -> Action {
        let req_id = self.next_req;
        self.next_req += 1;
        self.pending.insert(req_id, then);
        Action::Send(ClientFrame::Command { req_id, command })
    }

    pub(crate) fn say(&mut self, text: impl Into<String>) {
        self.note = Some(Note {
            text: text.into(),
            error: false,
        });
    }

    pub(crate) fn complain(&mut self, text: impl Into<String>) {
        self.note = Some(Note {
            text: text.into(),
            error: true,
        });
    }

    // Terminals.

    fn screen(&mut self, session: SessionId) -> &mut Screen {
        let size = self.session_size();
        self.screens
            .entry(session)
            .or_insert_with(|| Screen::new(size.cols, size.rows))
    }

    /// Whether the session shown is drawing a frame it has not finished (a
    /// synchronized update): drawn now, it would show half of one.
    pub(crate) fn frame_pending(&self) -> bool {
        self.attached
            .and_then(|session| self.screens.get(&session))
            .is_some_and(Screen::in_frame)
    }

    /// The session shown: open in the terminal, or selected in a list.
    pub(crate) fn focused_session(&self) -> Option<SessionId> {
        match (&self.mode, self.view) {
            (Mode::Terminal(session), _) => Some(*session),
            (_, View::NeedsYou) => self.needs_selected,
            (_, View::Sessions) => match self.selected {
                Some(Row::Session(session)) => Some(session),
                _ => None,
            },
            (_, View::Tickets) => None,
        }
    }

    /// Asked of a daemon and never answered (the connection went): asked
    /// again once the screen wants it.
    fn forget_unanswered(&mut self) {
        self.tickets
            .retain(|_, tickets| !matches!(tickets, Tickets::Loading));
        self.ticket_texts
            .retain(|_, text| !matches!(text, TicketText::Loading));
    }

    /// Scrolls the lists just enough to show their selections.
    fn keep_in_view(&mut self) {
        let full =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let height = usize::from(
            crate::view::browse_areas(self, full)
                .list
                .height
                .saturating_sub(2),
        )
        .max(1);
        let rows = self.model.rows();
        let selected = self
            .selected
            .and_then(|row| rows.iter().position(|r| *r == row));
        self.list_offset = in_view(self.list_offset, selected, rows.len(), height);
        let needs = self
            .needs_selected
            .and_then(|session| self.needs_list.iter().position(|s| *s == session));
        self.needs_offset = in_view(self.needs_offset, needs, self.needs_list.len(), height);
        let tickets = self.ticket_rows();
        let ticket = self
            .tickets_selected
            .and_then(|row| tickets.iter().position(|r| *r == row));
        self.tickets_offset = in_view(self.tickets_offset, ticket, tickets.len(), height);
    }

    /// The terminal area changed size.
    pub(crate) fn on_resize(&mut self, size: TermSize) -> Vec<Action> {
        self.terminal_size = size;
        self.keep_in_view();
        self.fit_attached()
    }

    /// The size sessions' programs are given: the pane beside the list, or
    /// the window below the top bar while one fills it. Programs draw for
    /// their terminal's size, so it is always the size they are shown at.
    pub(crate) fn session_size(&self) -> TermSize {
        if self.full_screen && matches!(self.mode, Mode::Terminal(_)) {
            return self.terminal_size;
        }
        let window =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let pane = crate::view::session_area(window);
        TermSize {
            cols: pane.width.max(1),
            rows: pane.height.max(1),
        }
    }

    /// Whether the list has the keys, rather than a session's terminal.
    pub(crate) fn list_has_keys(&self) -> bool {
        !matches!(self.mode, Mode::Terminal(_))
    }

    /// Gives the streamed session's program the size it is shown at now.
    pub(super) fn fit_attached(&mut self) -> Vec<Action> {
        let Some(session) = self.attached else {
            return Vec::new();
        };
        let size = self.session_size();
        self.screen(session).resize(size.cols, size.rows);
        let running = self
            .model
            .session(session)
            .is_some_and(|stored| stored.ended_at.is_none());
        if running {
            vec![Action::Send(ClientFrame::Resize { session, size })]
        } else {
            Vec::new()
        }
    }

    /// Brings the selection and the streamed terminal in line with the
    /// state, after anything changed.
    fn settle(&mut self) -> Vec<Action> {
        self.take_wanted();
        self.keep_selection();
        self.keep_ticket_selection();
        self.keep_in_view();
        if let Mode::Terminal(session) = self.mode
            && self.model.session(session).is_none()
        {
            self.mode = Mode::Browse;
            self.full_screen = false;
        }
        if !matches!(self.link, Link::Connected { .. }) {
            return Vec::new();
        }
        let mut actions = self.fetch_tickets();
        actions.extend(self.remove_closed());
        actions.extend(self.remove_idle_worktrees());
        let focus = self
            .focused_session()
            .filter(|s| self.model.session(*s).is_some());
        if focus == self.attached {
            return actions;
        }
        if let Some(old) = self.attached.take() {
            actions.push(Action::Send(ClientFrame::Detach { session: old }));
        }
        if let Some(session) = focus {
            let size = self.session_size();
            let screen = self.screen(session);
            screen.resize(size.cols, size.rows);
            let from_seq = screen.next_seq();
            let running = self
                .model
                .session(session)
                .is_some_and(|stored| stored.ended_at.is_none());
            if running {
                actions.push(Action::Send(ClientFrame::Resize { session, size }));
            }
            actions.push(Action::Send(ClientFrame::Attach {
                session,
                from_seq,
                // Attached while the window is out of focus: nobody looks.
                watching: self.focused != Some(false),
            }));
            self.attached = Some(session);
        }
        actions
    }

    /// Takes closed sessions off the list once their programs have ended.
    fn remove_closed(&mut self) -> Vec<Action> {
        let (ended, waiting): (Vec<SessionId>, Vec<SessionId>) = std::mem::take(&mut self.closing)
            .into_iter()
            .partition(|session| {
                self.model
                    .session(*session)
                    .is_none_or(|stored| stored.ended_at.is_some())
            });
        self.closing = waiting;
        // One already gone from the state needs nothing more.
        let ended: Vec<SessionId> = ended
            .into_iter()
            .filter(|session| self.model.session(*session).is_some())
            .collect();
        ended
            .into_iter()
            .map(|session| {
                self.command(
                    Command::RemoveSession { session },
                    Then::Say("Closed the session."),
                )
            })
            .collect()
    }

    /// Selects what a reply named, once the state shows it.
    fn take_wanted(&mut self) {
        let Some((row, open)) = self.wanted else {
            return;
        };
        if !self.model.rows().contains(&row) {
            return;
        }
        self.wanted = None;
        // A session opened from the Tickets screen leaves the person there
        // once they come back from it.
        if !(open && self.view == View::Tickets) {
            self.view = View::Sessions;
        }
        self.selected = Some(row);
        if let (true, Row::Session(session)) = (open, row) {
            self.mode = Mode::Terminal(session);
        }
    }

    /// Keeps the selection on something that exists: the line that took
    /// the place of one that went away, or the last line.
    fn keep_selection(&mut self) {
        let rows = self.model.rows();
        match self
            .selected
            .and_then(|row| rows.iter().position(|r| *r == row))
        {
            Some(at) => self.selected_at = at,
            None => {
                let at = self.selected_at.min(rows.len().saturating_sub(1));
                self.selected = rows.get(at).copied();
            }
        }
        // Screens of sessions that are gone are not drawn again.
        let model = &self.model;
        self.screens.retain(|id, _| model.session(*id).is_some());
        // Questions first, then news, oldest first; but the selected session
        // keeps its place after looking at it has cleared it.
        let mut list = self.model.needs_you();
        if let Some(selected) = self.needs_selected
            && !list.contains(&selected)
            && self.model.session(selected).is_some()
            && let Some(was) = self.needs_list.iter().position(|s| *s == selected)
        {
            list.insert(was.min(list.len()), selected);
        }
        if !self
            .needs_selected
            .is_some_and(|session| list.contains(&session))
        {
            self.needs_selected = list.first().copied();
        }
        self.needs_list = list;
    }
}

/// The first line to show of a list of `len` lines, `height` at a time,
/// moved from `offset` only as far as needed to show `selected`, and never
/// leaving empty space below the last line.
fn in_view(offset: usize, selected: Option<usize>, len: usize, height: usize) -> usize {
    let mut offset = offset.min(len.saturating_sub(height));
    if let Some(selected) = selected {
        if selected < offset {
            offset = selected;
        } else if selected >= offset + height {
            offset = selected + 1 - height;
        }
    }
    offset
}
