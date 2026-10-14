//! The UI's state machine. It takes key presses, pastes and frames from the
//! daemon, and returns the frames to send. It does no IO, so every behaviour
//! can be tested by feeding it events and drawing it into a test buffer.

mod events;
mod harness_choice;
mod link;
mod motion;
mod picker;
mod pointer;
mod settings;
mod settle;
mod tickets;

pub(crate) use harness_choice::HarnessChoice;
pub(crate) use picker::{FolderPicker, PICKER_ROWS, Shown, picker_filter};
pub(crate) use settings::{SettingRow, Settings};
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
mod look_tests;
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
mod settings_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tickets_tests;

use std::collections::HashMap;
use std::path::PathBuf;

use muster_core::protocol::{ClientFrame, Command, TermSize};
use muster_core::{ProjectId, SessionId, TicketKey, WorkspaceId};

use crate::input::LineInput;
use crate::model::{Model, Row};
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
    Settings,
}

impl View {
    /// The screens as tabs, in the order ⇥ goes round them.
    pub(crate) const TABS: [Self; 4] = [
        Self::Sessions,
        Self::NeedsYou,
        Self::Tickets,
        Self::Settings,
    ];

    /// The tab after this one, or with `back` the one before.
    pub(crate) fn next(self, back: bool) -> Self {
        let at = Self::TABS.iter().position(|tab| *tab == self).unwrap_or(0);
        let len = Self::TABS.len();
        Self::TABS[if back {
            (at + len - 1) % len
        } else {
            (at + 1) % len
        }]
    }
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
    /// Which agent to start on a ticket: in its worktree, or with
    /// `another` in a worktree of its own.
    AssignAgent {
        project: ProjectId,
        key: TicketKey,
        harness: HarnessChoice,
        another: bool,
    },
    Confirm {
        question: String,
        then: Confirmed,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Confirmed {
    StopSession(SessionId),
    /// Stop it and take it off the list once it has ended.
    CloseSession(SessionId),
    /// Close the ticket in its tracker.
    CloseTicket {
        project: ProjectId,
        key: TicketKey,
    },
    /// Close a closed ticket's agents and remove its worktrees.
    CleanUpTicket {
        project: ProjectId,
        key: TicketKey,
    },
    RemoveWorktree {
        workspace: WorkspaceId,
        force: bool,
    },
    RemoveProject(ProjectId),
}

/// A line under the screen telling the person what just happened. It
/// fades after a few seconds and gives the keys' hints back; an error
/// stays until the next key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Note {
    pub(crate) text: String,
    pub(crate) error: bool,
    /// The motion step it was shown at.
    pub(crate) shown_at: u64,
}

/// What to do with a command's reply.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    TicketText(ProjectId, TicketKey),
    OpenIssueSession,
    /// Say which pull request was opened.
    ShowPullRequest,
    /// A session being closed was asked to stop; if it cannot be, it is
    /// not closed either.
    Closing(SessionId),
    /// Drop the ticket from the list, and offer to clean up after it.
    TicketClosed(ProjectId, TicketKey),
    /// Keep the project's settings.
    Settings(ProjectId),
    /// Keep the project's settings as changed, and fetch its tickets again.
    SettingChanged(ProjectId),
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
    pub(crate) ticket_texts: HashMap<(ProjectId, TicketKey), TicketText>,
    /// Each project's settings, as last read; read again each time the
    /// Settings screen is shown, and on `r`.
    pub(crate) settings: HashMap<ProjectId, Settings>,
    /// The selected line of the settings list, and its first line shown.
    pub(crate) settings_selected: Option<SettingRow>,
    pub(crate) settings_offset: usize,
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
    /// The time now, as the runtime last read it, for how long sessions
    /// have run, waited or sat finished; `None` until it has.
    pub(crate) now: Option<muster_core::Timestamp>,
    /// Sessions whose lines flash for a change of status, with the step
    /// each flash began.
    flashes: HashMap<SessionId, u64>,
    /// The harness last started from the new-session dialog, which it
    /// offers first next time.
    last_harness: HarnessChoice,
    /// The agent last started on a ticket, which the next ticket is
    /// offered first.
    ticket_agent: HarnessChoice,
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
            settings: HashMap::new(),
            settings_selected: None,
            settings_offset: 0,
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
            now: None,
            flashes: HashMap::new(),
            last_harness: HarnessChoice::Claude,
            ticket_agent: HarnessChoice::Claude,
        }
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
            shown_at: self.step,
        });
    }

    pub(crate) fn complain(&mut self, text: impl Into<String>) {
        self.note = Some(Note {
            text: text.into(),
            error: true,
            shown_at: self.step,
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
            (_, View::Tickets | View::Settings) => None,
        }
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
}
