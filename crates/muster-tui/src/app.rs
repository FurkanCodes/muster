//! The UI's state machine. It takes key presses, pastes and frames from the
//! daemon, and returns the frames to send. It does no IO, so every behaviour
//! can be tested by feeding it events and drawing it into a test buffer.

mod actions;
mod alarm;
mod changes;
pub(crate) mod cloning;
mod controls;
mod dialog_mouse;
pub(crate) mod drag;
mod events;
mod fold;
pub(crate) mod grab;
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
mod placed;
mod plugin_panel;
pub(crate) mod plugins;
mod pointer;
mod prefix;
mod project_order;
mod rebind;
mod rename;
pub(crate) mod screens;
pub(crate) mod scrollbars;
pub(crate) mod selection;
mod settings;
mod settle;
mod sharing;
mod shell;
mod spaces;
mod stacks;
mod telling;
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
pub(crate) use plugins::{PluginDialog, PluginKey};
pub(crate) use rebind::{RebindRow, Rebinding, Rebound};
pub(crate) use settings::{SettingRow, Settings};
pub(crate) use tickets::{TicketRow, TicketText, Tickets};

#[cfg(test)]
mod agents_tests;
#[cfg(test)]
mod alarm_tests;
#[cfg(test)]
mod by_hand_tests;
#[cfg(test)]
mod by_mouse_tests;
#[cfg(test)]
mod changes_tests;
#[cfg(test)]
mod clashes_tests;
#[cfg(test)]
mod controls_tests;
#[cfg(test)]
mod dialog_mouse_tests;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod fold_tests;
#[cfg(test)]
mod folder_tests;
#[cfg(test)]
mod grab_tests;
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
mod panel_keys_tests;
#[cfg(test)]
mod panels_tests;
#[cfg(test)]
mod picker_tests;
#[cfg(test)]
mod placed_tests;
#[cfg(test)]
mod plugin_hooks_tests;
#[cfg(test)]
mod plugin_panel_tests;
#[cfg(test)]
mod plugins_tests;
#[cfg(test)]
mod pointer_tests;
#[cfg(test)]
mod prefix_tests;
#[cfg(test)]
mod project_tests;
#[cfg(test)]
mod pull_request_tests;
#[cfg(test)]
mod rebind_tests;
#[cfg(test)]
mod scrollbars_tests;
#[cfg(test)]
mod selection_tests;
#[cfg(test)]
mod settings_tests;
#[cfg(test)]
mod sharing_tests;
#[cfg(test)]
mod shell_tests;
#[cfg(test)]
mod slide_tests;
#[cfg(test)]
mod spend_tests;
#[cfg(test)]
mod telling_tests;
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
    /// Play a sound the person is choosing for a notification, on a Mac.
    Hear(muster_core::telling::Sound),
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
        /// The GitHub repository to clone into the folder chosen, rather
        /// than a folder to add as it is.
        clone: Option<String>,
    },
    /// A GitHub repository to clone as a project: those the person can
    /// reach, narrowed by what they type, or one they name.
    CloneProject(cloning::CloneChoice),
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
    /// A new space's name, or a space's new name (`app::spaces`); a new
    /// one's `then` is done in it once it is made.
    SpaceName {
        space: Option<muster_core::SpaceId>,
        name: LineInput,
        then: Option<spaces::Later>,
    },
    /// Every action and its keys, for the person to set them.
    Keys(Rebinding),
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
    /// Where a project's tickets come from: a ticket plugin, by its id, or
    /// `None` for the one its git remote says; with its fields, as typed.
    TicketsFrom {
        project: ProjectId,
        tracker: Option<String>,
        fields: Vec<tickets_from::Field>,
        /// Which field has the cursor.
        at: usize,
        /// What was typed for another tracker's fields, by tracker and
        /// key, kept as ⇥ goes round them.
        typed: std::collections::BTreeMap<(String, String), String>,
    },
    /// Adding or updating a plugin, a step at a time.
    Plugin(PluginDialog),
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
    /// Remove the plugin by this id.
    RemovePlugin(String),
    /// Remove the space, which the daemon does only once it is empty.
    DeleteSpace(muster_core::SpaceId),
    /// Start the terminal again in the workspace, ending what runs in it.
    RestartIn {
        session: SessionId,
        workspace: WorkspaceId,
    },
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
    /// Say how a kind of notification is told now.
    TellChanged(muster_core::telling::Kind, muster_core::telling::Tell),
    /// Say how the UI looks now.
    LookChanged(look::LookChange),
    /// The list's fold is kept, or could not be.
    ListFolded,
    /// The panels' layout is kept, or could not be.
    PanelsKept,
    /// Offer the models the agent lists, in the dialog open for it.
    Models(String),
    /// Offer the repositories GitHub lists, in the dialog open for them.
    Repositories,
    /// Place the session started for a panel there, with the keys.
    Place(muster_core::layout::Place),
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
    /// The daemon read its plugins again; what changed comes as a change.
    PluginsRead,
    /// Show the preview in the dialog waiting for request `0`.
    PluginPreview(u64),
    /// Say the plugin is turned on, or off.
    PluginToggled(PluginKey, bool),
    /// Say what the preview showed is done, and select the plugin.
    PluginInstalled(Box<muster_core::protocol::plugins::PluginPreview>),
    /// Say the plugin by this id is removed.
    PluginRemoved(String),
    /// Something about spaces (`app::spaces`).
    Space(spaces::Waiting),
    Say(&'static str),
}

#[derive(Debug)]
pub(crate) struct App {
    pub(crate) areas_cache: std::cell::RefCell<crate::view::areas::Cache>,
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
    /// The space the sessions list shows; `None` for all of it. This UI's
    /// own: the daemon is never told (`app::spaces`).
    pub(crate) space: Option<muster_core::SpaceId>,
    /// Where the selected line was, so the selection stays nearby when
    /// that line goes away.
    selected_at: usize,
    /// The first line shown of the sessions list: the view moves only
    /// when the selection would leave it.
    pub(crate) list_offset: usize,
    pub(crate) needs: screens::NeedsScreen,
    pub(crate) tickets: screens::TicketsScreen,
    pub(crate) settings: screens::SettingsScreen,
    pub(crate) panels: screens::Panels,
    pub(crate) alarms: screens::Alarms,
    pub(crate) moving: screens::Moving,
    pub(crate) person: screens::Person,
    requests: screens::Requests,
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
    /// The time now, as the runtime last read it, for how long sessions
    /// have run, waited or sat finished; `None` until it has.
    pub(crate) now: Option<muster_core::Timestamp>,
    /// Whether the welcome fills the window, as the UI starts.
    pub(crate) welcome: bool,
    /// The harness last started from the new-session dialog, which it
    /// offers first next time.
    last_harness: HarnessChoice,
    /// The machine whose Muster this UI shows, reached through ssh
    /// (`muster --on vm`); `None` for this one's.
    pub(crate) machine: Option<String>,
    /// Whether this UI has started the notch since it connected, or seen
    /// it turned off since.
    notch_started: bool,
    /// The size the streamed session's program was last given.
    fitted: Option<TermSize>,
    /// What each key does, Muster's own with the person's over them.
    pub(crate) keymap: muster_core::keys::Keymap,
    /// A session started for a panel of its own, a shell, and where it
    /// goes: placed there, with the keys, once the state has it.
    wanted_place: Option<(SessionId, muster_core::layout::Place)>,
    /// The plugin's panel streamed beside the session, the same way.
    pub(super) plugin_stream: settle::Streamed,
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
            space: None,
            selected_at: 0,
            list_offset: 0,
            needs: screens::NeedsScreen::default(),
            tickets: screens::TicketsScreen::default(),
            settings: screens::SettingsScreen::default(),
            panels: screens::Panels::default(),
            alarms: screens::Alarms::default(),
            moving: screens::Moving::default(),
            person: screens::Person::default(),
            requests: screens::Requests::default(),
            screens: HashMap::new(),
            changes: None,
            selection: None,
            attached: None,
            focus_told: None,
            wanted: None,
            terminal_size,
            full_screen: false,
            closing: Vec::new(),
            removing: Vec::new(),
            cwd,
            now: None,
            welcome: false,
            last_harness: HarnessChoice::CLAUDE,
            machine: None,
            notch_started: false,
            areas_cache: std::cell::RefCell::default(),
            fitted: None,
            keymap: muster_core::keys::Keymap::default(),
            wanted_place: None,
            plugin_stream: settle::Streamed::default(),
        }
    }

    // Commands.

    fn command(&mut self, command: Command, then: Then) -> Action {
        let req_id = self.requests.next;
        self.requests.next += 1;
        self.requests.pending.insert(req_id, then);
        Action::Send(ClientFrame::Command { req_id, command })
    }

    pub(crate) fn say(&mut self, text: impl Into<String>) {
        self.note = Some(Note {
            text: text.into(),
            error: false,
            shown_at: self.moving.step,
        });
    }

    pub(crate) fn complain(&mut self, text: impl Into<String>) {
        self.note = Some(Note {
            text: text.into(),
            error: true,
            shown_at: self.moving.step,
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
            // A plugin's panel or a session placed beside the preview has
            // the keys: the preview stays on the one selected.
            (Mode::Terminal(session), _)
                if !self.is_plugin_panel(*session) && !self.placed_beside().contains(session) =>
            {
                Some(*session)
            }
            (_, View::NeedsYou) => self.needs.selected,
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

    /// The session drawn in the preview: the one shown, or, while that one
    /// is placed in a panel of its own, the last shown that is not (none if
    /// it is gone or placed too), so every terminal is drawn and none twice.
    pub(crate) fn session_beside_list(&self) -> Option<SessionId> {
        let pinned = self.placed_beside();
        match self.focused_session() {
            Some(session) if pinned.contains(&session) => self
                .panels
                .agents
                .into_iter()
                .flatten()
                .find(|kept| !pinned.contains(kept) && self.model.session(*kept).is_some()),
            focused => focused,
        }
    }

    /// The session the preview shows: the one drawn beside the list, but
    /// none while the person folded its preview away by its `✕`, unless
    /// they type in it.
    pub(crate) fn preview_session(&self) -> Option<SessionId> {
        self.session_beside_list().filter(|session| {
            self.panels.preview_folded != Some(*session) || self.mode == Mode::Terminal(*session)
        })
    }

    /// Whether the preview has nothing to draw: the session selected is
    /// placed and none is kept beside it, or its preview is folded away,
    /// so the preview goes.
    pub(crate) fn session_panel_empty(&self) -> bool {
        self.focused_session().is_some() && self.preview_session().is_none()
    }

    /// The session whose terminal is drawn, and so streamed: open, or
    /// selected beside the list. A narrow window's list draws none.
    pub(crate) fn shown_session(&self) -> Option<SessionId> {
        self.preview_session()
            .filter(|session| self.model.session(*session).is_some())
            // One placed is drawn, and streamed, in its own panel.
            .filter(|session| !self.placed_beside().contains(session))
            .filter(|_| self.session_drawn())
    }

    /// The session whose program is in focus: the one shown, beside the
    /// list or filling the window, in a window the person looks at (or one
    /// that never says). On screen is in focus, as in a terminal app,
    /// whether or not it has the keys: Claude Code suggests the person's
    /// next prompt only as a turn ends in focus, and the person watches a
    /// turn end from the list.
    pub(crate) fn focus_session(&self) -> Option<SessionId> {
        let shown = if self.typing_in_plugin() {
            self.plugin_beside()
        } else if let Some(pinned) = self.typing_in_placed() {
            Some(pinned)
        } else {
            self.shown_session()
        };
        shown.filter(|_| self.person.focused != Some(false))
    }

    /// Tells the daemon which session is in focus now, if that changed, so
    /// a program that asked learns it gained or lost focus.
    pub(super) fn tell_focus(&mut self) -> Option<Action> {
        let wanted = self.focus_session();
        let knows = matches!(self.link, Link::Connected { .. });
        if !knows || wanted == self.focus_told {
            return None;
        }
        self.focus_told = wanted;
        Some(Action::Send(ClientFrame::Keys { session: wanted }))
    }

    /// A session's terminal of `size` in pixels, scaled from a cell of the
    /// person's terminal; `None` where that terminal does not say its own.
    pub(crate) fn session_pixels(&self, size: TermSize) -> Option<PixelSize> {
        let cell = self.person.cell_pixels?;
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
