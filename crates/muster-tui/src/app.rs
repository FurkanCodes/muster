//! The UI's state machine. It takes key presses, pastes and frames from the
//! daemon, and returns the frames to send. It does no IO, so every behaviour
//! can be tested by feeding it events and drawing it into a test buffer.

mod actions;
mod alarm;
mod changes;
pub(crate) mod drag;
mod events;
mod fold;
mod harness_choice;
mod help;
mod lines;
mod link;
mod look;
mod menu;
mod motion;
mod naming;
mod narrow;
mod notch;
mod panels;
mod picker;
mod pointer;
mod rename;
pub(crate) mod selection;
mod settings;
mod settle;
mod sharing;
mod stacks;
mod tickets;
pub(crate) mod tickets_from;
mod tidy;
pub(crate) mod timeline;
mod welcome;

pub(crate) use actions::Here;
pub(crate) use changes::{ChangeRow, ChangesPane, DiffPane, Pick};
pub(crate) use harness_choice::HarnessChoice;
pub(crate) use help::{Help, HelpLine};
pub(crate) use menu::Menu;
pub(crate) use picker::{FolderPicker, PICKER_ROWS, Shown, picker_filter};
pub(crate) use settings::{SettingRow, Settings};
pub(crate) use tickets::{TicketRow, TicketText, Tickets};

#[cfg(test)]
mod agents_tests;
#[cfg(test)]
mod alarm_tests;
#[cfg(test)]
mod by_hand_tests;
#[cfg(test)]
mod changes_tests;
#[cfg(test)]
mod clashes_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod fold_tests;
#[cfg(test)]
mod folder_tests;
#[cfg(test)]
mod help_tests;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod look_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod motion_tests;
#[cfg(test)]
mod mouse_sweep_tests;
#[cfg(test)]
mod naming_tests;
#[cfg(test)]
mod narrow_tests;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod notch_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod panels_tests;
#[cfg(test)]
mod picker_tests;
#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod pull_request_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod sharing_tests;
#[cfg(test)]
mod slide_tests;
#[cfg(test)]
mod spend_tests;
#[cfg(test)]
mod terminal_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tickets_from_tests;
#[cfg(test)]
mod tickets_tests;
#[cfg(test)]
mod welcome_tests;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use muster_core::config::TrackerKind;
use muster_core::layout::Layout;
use muster_core::protocol::{ClientFrame, Command, PixelSize, TermSize};
use muster_core::{ProjectId, SessionId, TicketKey, WorkspaceId};

use crate::input::LineInput;
use crate::model::{Model, Row};
use crate::screen::Screen;
use selection::Selection;

/// What the runtime must do for the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Send(ClientFrame),
    /// Tell the person, who is looking elsewhere: a desktop notification
    /// where the terminal shows one, and its bell.
    Notify(String),
    /// Open a web page in the person's browser.
    Open(String),
    /// Put text on the person's clipboard.
    Copy(String),
    /// List the folders in this folder, for the folder picker
    /// ([`App::on_folders`]).
    ListFolders(PathBuf),
    /// Start Muster's notch, unless it runs already.
    StartNotch,
    /// Bring the person's terminal, this window's, to the front.
    Activate,
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
    /// A workspace's changes are beside the list (`App::changes`), and
    /// keys go to them.
    Changes,
    Dialog(Dialog),
    /// What can be done on the selected line, opened by a right-click on
    /// it (`app::menu`).
    Menu(Menu),
    /// Every key of the screen shown, over it (`app::help`).
    Help(Help),
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
    /// Which agent to start on a ticket, and, where several projects
    /// share it, in which: in its worktree there, or with `another` in a
    /// worktree of its own if it has one there already.
    AssignAgent {
        /// The ticket's project, as listed.
        project: ProjectId,
        key: TicketKey,
        harness: HarnessChoice,
        another: bool,
        /// Where it starts: `project`, or another sharing its tickets.
        into: ProjectId,
        /// The worktree in `into` whose branch a new worktree for it is
        /// made on top of; `None` for the base.
        on: Option<WorkspaceId>,
    },
    Confirm {
        question: String,
        then: Confirmed,
    },
    /// Whether to put a workspace's files back as they were before an
    /// entry of its timeline (`what`), and its branch too where it can
    /// go; `agent_remembers` names the agent whose turn it was.
    Undo {
        workspace: WorkspaceId,
        plan: muster_core::timeline::UndoPlan,
        what: String,
        agent_remembers: Option<String>,
    },
    /// A session's new name, as the person types it.
    Rename {
        session: SessionId,
        name: LineInput,
    },
    /// The model `agent` names chats with, as the person types or picks
    /// it among those it lists, once the daemon says.
    NamingModel {
        agent: String,
        model: LineInput,
        listed: Option<Result<Vec<String>, String>>,
    },
    /// Which agent tidies a workspace's commits not pushed yet.
    Tidy {
        workspace: WorkspaceId,
        harness: HarnessChoice,
    },
    /// Where a project's tickets come from: a tracker, or `None` for the
    /// one its git remote says; Jira's site and project's key, or Linear's
    /// team's key, as typed.
    TicketsFrom {
        project: ProjectId,
        tracker: Option<TrackerKind>,
        site: LineInput,
        key: LineInput,
        /// Whether the key has the cursor, rather than Jira's site.
        on_key: bool,
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
    /// Move the worktree's branch onto the newest of its base, or of the
    /// branch it is stacked on.
    MoveOnto(WorkspaceId),
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
    /// The same, after the person chose where its tickets come from.
    TicketsFromChanged(ProjectId),
    /// Keep the project's settings as changed: whether its branches start
    /// with `muster/`.
    BranchesChanged(ProjectId),
    /// Say whether the notch is on now: turned on, or off.
    NotchChanged(bool),
    /// Say from how many columns a session sits beside the list now.
    SideBySideChanged(u16),
    /// Say how the UI looks now.
    LookChanged(look::LookChange),
    /// The list's fold is kept, or could not be.
    ListFolded,
    /// The panels' layout is kept, or could not be.
    PanelsKept,
    /// Offer the models the agent lists, in the dialog open for it.
    Models(String),
    /// Show the workspace's changes in the pane.
    Changes(WorkspaceId),
    /// Show the file's or the entry's diff in the pane.
    Diff(WorkspaceId, Pick),
    /// Show the workspace's timeline in the pane.
    Timeline(WorkspaceId),
    /// Ask the person whether to put the files back, as the plan says.
    UndoPlan(WorkspaceId),
    /// Say the files are back from that entry, and show what is there now.
    Undone(WorkspaceId, u64),
    /// Say the branch moved, and onto what.
    Moved(WorkspaceId),
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
    /// The projects' and worktrees' lines folded in the sessions list,
    /// hiding what is under them (`app::fold`).
    folded: HashSet<Row>,
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
    /// A workspace's changes, shown beside the list while open (`d`).
    pub(crate) changes: Option<ChangesPane>,
    /// Text selected with the mouse in the screen of the session with the
    /// keys.
    pub(crate) selection: Option<Selection>,
    /// The session whose terminal is streamed to this client.
    attached: Option<SessionId>,
    /// The session the daemon was last told is in focus.
    focus_told: Option<SessionId>,
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
    /// The step a session last finished at, while Kep is glad of it.
    glad_at: Option<u64>,
    /// Finishes not told yet, each with the step it came at: held a
    /// moment, in case the session goes straight back to work
    /// (`app::alarm`). One for each session, gone once told or dropped.
    held_news: Vec<(SessionId, u64)>,
    /// The session told of last, which `l` goes to.
    last_alarm: Option<SessionId>,
    /// The selected line whose name, cut short, slides along.
    slide: Option<motion::Slide>,
    /// Whether the welcome fills the window, as the UI starts.
    pub(crate) welcome: bool,
    /// The harness last started from the new-session dialog, which it
    /// offers first next time.
    last_harness: HarnessChoice,
    /// The agent last started on a ticket, which the next ticket is
    /// offered first.
    ticket_agent: HarnessChoice,
    /// The project last chosen for an agent on a ticket that several
    /// projects share, by where their tickets come from: offered first
    /// next time.
    ticket_projects: HashMap<String, ProjectId>,
    /// The protocol the daemon agreed to speak: a change it does not know
    /// is never sent to it (`ARC-09`).
    pub(crate) daemon_protocol: u32,
    /// What the person's terminal draws: the agents' logos as pictures,
    /// or their marks in text.
    pub(crate) graphics: crate::view::logos::Graphics,
    /// Whether Muster's notch can show here: a Mac, with a program to
    /// start it.
    pub(crate) notch_here: bool,
    /// Whether this UI has started the notch since it connected, or seen
    /// it turned off since.
    notch_started: bool,
    /// The notch is being turned on or off: the answer is on its way.
    pub(crate) notch_changing: bool,
    /// The colours of the person's terminal, as it said them as the UI
    /// started, told to each daemon the UI connects to.
    pub(crate) colours: muster_core::colours::TerminalColours,
    /// One cell of the person's terminal in pixels, as its size says,
    /// read as the UI starts and as the window is resized.
    pub(crate) cell_pixels: Option<PixelSize>,
    /// The width a session sits beside the list from is being changed.
    pub(crate) side_by_side_changing: bool,
    /// The look is being changed: whose colours, or how many lines.
    pub(crate) look_changing: bool,
    /// The panels as the person moved them, until the daemon keeps them,
    /// or for as long as the window lasts where it cannot.
    panels_kept: Option<Layout>,
    /// The list folded away or brought back, the same way.
    folded_kept: Option<bool>,
    /// The size the streamed session's program was last given.
    fitted: Option<TermSize>,
    /// A panel or the line between two being dragged.
    pub(crate) drag: Option<drag::Drag>,
    /// The panels being moved with the keys.
    pub(crate) arranging: Option<drag::Arranging>,
    /// What each key does, Muster's own with the person's over them.
    pub(crate) keymap: muster_core::keys::Keymap,
}

impl App {
    /// The coding agents the person added in Muster's own settings, as the
    /// daemon last said.
    pub(crate) fn added_agents(&self) -> &[muster_core::settings::AddedAgent] {
        &self.model.preferences().agents
    }

    pub(crate) fn new(cwd: PathBuf, terminal_size: TermSize) -> Self {
        Self {
            model: Model::default(),
            view: View::Sessions,
            mode: Mode::Browse,
            link: Link::Connecting,
            note: None,
            selected: None,
            folded: HashSet::new(),
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
            changes: None,
            selection: None,
            attached: None,
            focus_told: None,
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
            glad_at: None,
            held_news: Vec::new(),
            last_alarm: None,
            slide: None,
            welcome: false,
            last_harness: HarnessChoice::Claude,
            ticket_agent: HarnessChoice::Claude,
            ticket_projects: HashMap::new(),
            daemon_protocol: muster_core::protocol::PROTOCOL_VERSION,
            graphics: crate::view::logos::Graphics::Text,
            notch_here: false,
            notch_started: false,
            notch_changing: false,
            colours: muster_core::colours::TerminalColours::default(),
            cell_pixels: None,
            side_by_side_changing: false,
            look_changing: false,
            panels_kept: None,
            folded_kept: None,
            fitted: None,
            drag: None,
            arranging: None,
            keymap: muster_core::keys::Keymap::default(),
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
        // The session's screen is drawn again at another size.
        self.selection = None;
        // Narrower or wider than a session beside the list needs, the
        // session shown may be another, or none; the one still shown is
        // given its new size.
        self.settle()
    }

    /// The size sessions' programs are given: the pane beside the list, or
    /// the window below the top bar while one fills it, as it does in a
    /// window too narrow for both. Programs draw for their terminal's size,
    /// so it is always the size they are shown at.
    pub(crate) fn session_size(&self) -> TermSize {
        if self.fills_window() || self.narrow() {
            return self.terminal_size;
        }
        let window =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let pane = crate::view::session_area(self, window);
        TermSize {
            cols: pane.width.max(1),
            rows: pane.height.max(1),
        }
    }

    /// The session whose terminal is drawn, and so streamed: open, or
    /// selected beside the list. A narrow window's list draws none.
    pub(crate) fn shown_session(&self) -> Option<SessionId> {
        self.focused_session()
            .filter(|session| self.model.session(*session).is_some())
            .filter(|_| self.session_drawn())
    }

    /// The session whose program is in focus: the one shown, beside the
    /// list or filling the window, in a window the person looks at (or one
    /// that never says). On screen is in focus, as in a terminal app,
    /// whether or not it has the keys: Claude Code suggests the person's
    /// next prompt only as a turn ends in focus, and the person watches a
    /// turn end from the list.
    pub(crate) fn focus_session(&self) -> Option<SessionId> {
        self.shown_session().filter(|_| self.focused != Some(false))
    }

    /// Tells the daemon which session is in focus now, if that changed, so
    /// a program that asked learns it gained or lost focus. Not to a
    /// daemon before it knew how (`ARC-09`).
    pub(super) fn tell_focus(&mut self) -> Option<Action> {
        let wanted = self.focus_session();
        let knows = matches!(self.link, Link::Connected { .. })
            && self.daemon_protocol >= muster_core::colours::TERMINAL_PROTOCOL;
        if !knows || wanted == self.focus_told {
            return None;
        }
        self.focus_told = wanted;
        Some(Action::Send(ClientFrame::Keys { session: wanted }))
    }

    /// A session's terminal of `size` in pixels, scaled from a cell of the
    /// person's terminal; `None` where that terminal does not say its own.
    pub(crate) fn session_pixels(&self, size: TermSize) -> Option<PixelSize> {
        let cell = self.cell_pixels?;
        Some(PixelSize {
            width: size.cols.saturating_mul(cell.width),
            height: size.rows.saturating_mul(cell.height),
        })
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
        self.fitted = Some(size);
        let running = self
            .model
            .session(session)
            .is_some_and(|stored| stored.ended_at.is_none());
        if running {
            vec![Action::Send(ClientFrame::Resize {
                session,
                size,
                pixels: self.session_pixels(size),
            })]
        } else {
            Vec::new()
        }
    }
}
