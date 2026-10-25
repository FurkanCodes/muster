//! Every key of the screen shown, over it (`h`, or `?` from habit): so
//! the keys can be found without knowing them. Typing narrows the list to
//! the keys whose key or words hold what is typed; ↑↓ scroll it, and Esc
//! or ⏎ close it.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

use muster_core::keys::{Action as Does, Keymap};

use super::{Action, App, Mode, View};
use crate::input::{Edit, LineInput};

/// The help over the screen.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Help {
    /// What the keys shown must hold.
    pub(crate) filter: LineInput,
    /// The first line shown.
    pub(crate) offset: usize,
    /// Opened over a workspace's changes, which it goes back to.
    pub(crate) over_changes: bool,
}

/// One line of a section: an action, its keys as the person set them, or
/// keys that are the same everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Entry {
    /// An action, said as it says itself.
    Act(Does),
    /// An action, said otherwise here.
    DoesAs(Does, &'static str),
    /// Keys no setting changes, and what they do.
    Fixed(&'static str, &'static str),
}

/// Lines under a heading.
type Section = (&'static str, &'static [Entry]);

/// One line of the help as shown: a heading, or keys and what they do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HelpLine {
    Heading(&'static str),
    Key(String, String),
}

use Entry::{Act as D, DoesAs, Fixed};

const ANYWHERE: &[Entry] = &[
    D(Does::PanelKey),
    DoesAs(Does::NextTab, "the next tab"),
    DoesAs(Does::PreviousTab, "the previous tab"),
    D(Does::SessionsTab),
    D(Does::NeedsYouTab),
    D(Does::TicketsTab),
    D(Does::SettingsTab),
    D(Does::Back),
    D(Does::LastAlarm),
    D(Does::FoldList),
    D(Does::MovePanels),
    D(Does::Help),
    D(Does::Quit),
];

/// What goes to a session's program, and what does not.
const IN_A_SESSION: Section = (
    "In a session",
    &[
        D(Does::LeaveSession),
        D(Does::FoldList),
        D(Does::PanelKey),
        Fixed("Ctrl-1 Ctrl-2 Ctrl-3", "straight to a tab"),
        Fixed("any other key", "goes to its program, esc and ⇥ too"),
    ],
);

/// What the key after the panel key does, on the list or in a panel.
const AFTER_THE_PANEL_KEY: Section = (
    "After the panel key",
    &[
        D(Does::SplitRight),
        D(Does::SplitDown),
        D(Does::FocusLeft),
        D(Does::FocusDown),
        D(Does::FocusUp),
        D(Does::FocusRight),
        D(Does::SwapLeft),
        D(Does::SwapDown),
        D(Does::SwapUp),
        D(Does::SwapRight),
        D(Does::Zoom),
        D(Does::TakeOff),
        D(Does::ClosePanel),
        D(Does::PlacePreview),
        D(Does::Resize),
        D(Does::ToPanel),
        D(Does::ToList),
        D(Does::SendPanelKey),
        D(Does::PanelKeys),
        Fixed("esc", "give up"),
        Fixed("right-click a title", "the same, with their keys"),
    ],
);

const SESSIONS: &[Section] = &[
    (
        "On the list",
        &[
            Fixed("↑ ↓", "move"),
            D(Does::MoveUp),
            D(Does::MoveDown),
            Fixed(
                "⏎",
                "type in the session beside the list; on a worktree a new session, on a project a new worktree",
            ),
            D(Does::FullScreen),
            D(Does::Fold),
            D(Does::AddProject),
            D(Does::CloneProject),
            D(Does::NewWorktree),
            D(Does::NewSession),
            D(Does::ShowTickets),
            D(Does::ShowChanges),
            D(Does::Shell),
            D(Does::StopSession),
            D(Does::RestartSession),
            D(Does::Rename),
            D(Does::Close),
            D(Does::Checks),
            D(Does::Review),
            D(Does::Tidy),
            D(Does::MoveBranch),
            D(Does::OpenPullRequest),
            D(Does::PullRequestInBrowser),
            Fixed("PgUp PgDn End", "scroll the session beside the list"),
            Fixed("right-click", "what can be done on a line, with its keys"),
        ],
    ),
    IN_A_SESSION,
    AFTER_THE_PANEL_KEY,
    ("Anywhere", ANYWHERE),
];

const NEEDS_YOU: &[Section] = &[
    (
        "On the queue",
        &[
            Fixed("↑ ↓", "move"),
            D(Does::MoveUp),
            D(Does::MoveDown),
            Fixed("⏎", "type in the session"),
            D(Does::FullScreen),
            DoesAs(Does::ShowChanges, "what its workspace has changed"),
            DoesAs(Does::NewSession, "a new session in its worktree"),
            D(Does::Shell),
            D(Does::PinSession),
            D(Does::StopSession),
            D(Does::RestartSession),
            D(Does::Rename),
            DoesAs(Does::Close, "close the session"),
            Fixed("PgUp PgDn End", "scroll the session beside the list"),
            Fixed("right-click", "what can be done on a line, with its keys"),
        ],
    ),
    IN_A_SESSION,
    AFTER_THE_PANEL_KEY,
    ("Anywhere", ANYWHERE),
];

const TICKETS: &[Section] = &[
    (
        "On the tickets",
        &[
            Fixed("↑ ↓", "move"),
            D(Does::MoveUp),
            D(Does::MoveDown),
            Fixed("⏎", "open the agent on the ticket, or start the first"),
            D(Does::AnotherAgent),
            D(Does::TicketInBrowser),
            D(Does::CloseTicket),
            D(Does::RefreshTickets),
        ],
    ),
    (
        "Choosing an agent",
        &[
            Fixed("⇥ ⇧⇥", "the agent"),
            Fixed("← →", "what its worktree is made on top of"),
            Fixed("↑ ↓", "which project, where several share the tickets"),
        ],
    ),
    ("Anywhere", ANYWHERE),
];

const SETTINGS: &[Section] = &[
    (
        "On the settings",
        &[
            Fixed("↑ ↓", "move"),
            D(Does::MoveUp),
            D(Does::MoveDown),
            Fixed(
                "⏎ space",
                "turn the setting on or off, go to its next choice, or open its dialog",
            ),
            Fixed("← →", "a smaller or a larger choice"),
            D(Does::ReadSettings),
        ],
    ),
    (
        "On the plugins",
        &[
            Fixed("⏎ space", "turn the plugin on or off"),
            D(Does::AddPlugin),
            D(Does::UpdatePlugin),
            D(Does::RemovePlugin),
        ],
    ),
    ("Anywhere", ANYWHERE),
];

const CHANGES: &[Section] = &[
    (
        "In the changes",
        &[
            Fixed("↑ ↓  j k", "choose a file or a turn; in a diff, scroll"),
            Fixed("⏎ → l", "its diff"),
            Fixed("PgUp PgDn space", "a page"),
            Fixed("Home End  g G", "a diff's top or bottom"),
            Fixed("esc ⌫ ← h", "back from a diff"),
            D(Does::ChangesAgain),
            D(Does::PutBack),
            D(Does::CloseChanges),
            DoesAs(Does::LeaveSession, "back to the list"),
            D(Does::FoldList),
            D(Does::ChangesHelp),
        ],
    ),
    (
        "Anywhere",
        &[
            DoesAs(Does::NextTab, "the next tab"),
            Fixed("Ctrl-C", "quit the UI"),
        ],
    ),
];

impl Help {
    /// The keys of `view`, or of the changes.
    fn sections(view: View, over_changes: bool) -> &'static [Section] {
        if over_changes {
            return CHANGES;
        }
        match view {
            View::Sessions => SESSIONS,
            View::NeedsYou => NEEDS_YOU,
            View::Tickets => TICKETS,
            View::Settings => SETTINGS,
        }
    }

    /// The lines shown for `view`, the keys as the person set them: each
    /// heading with its keys that hold what is typed, ignoring case; a
    /// heading with none goes, and an action without a key.
    pub(crate) fn lines(&self, view: View, keymap: &Keymap) -> Vec<HelpLine> {
        let wanted = self.filter.text().trim().to_lowercase();
        let holds = |(keys, what): &(String, String)| {
            wanted.is_empty()
                || keys.to_lowercase().contains(&wanted)
                || what.to_lowercase().contains(&wanted)
        };
        let said = |entry: &Entry| match *entry {
            Entry::Act(does) => (keymap.labels(does), does.what().to_owned()),
            Entry::DoesAs(does, what) => (keymap.labels(does), what.to_owned()),
            Entry::Fixed(keys, what) => (keys.to_owned(), what.to_owned()),
        };
        let mut lines = Vec::new();
        let mut add = |heading: &'static str, entries: Vec<(String, String)>| {
            let shown: Vec<HelpLine> = entries
                .into_iter()
                .filter(|(keys, _)| keys != "none")
                .filter(holds)
                .map(|(keys, what)| HelpLine::Key(keys, what))
                .collect();
            if !shown.is_empty() {
                lines.push(HelpLine::Heading(heading));
                lines.extend(shown);
            }
        };
        let lists = matches!(view, View::Sessions | View::NeedsYou) && !self.over_changes;
        for (at, (heading, entries)) in Self::sections(view, self.over_changes).iter().enumerate() {
            add(heading, entries.iter().map(said).collect());
            // The panels of the plugins that run, after the list's own keys.
            if at == 0 && lists {
                let panels = keymap
                    .panels()
                    .map(|(action, _)| {
                        (
                            keymap.panel_labels(&action.name()),
                            keymap.panel_what(action),
                        )
                    })
                    .collect();
                add(PANELS, panels);
            }
        }
        lines
    }
}

/// What the panels of the plugins that run are listed under.
const PANELS: &str = "Plugins' panels";

/// How many lines of the help a page is.
const PAGE: usize = 10;

impl App {
    /// `h` or `?`: the keys of the screen shown, over it.
    pub(super) fn open_help(&mut self) {
        self.note = None;
        self.mode = Mode::Help(Help {
            over_changes: self.mode == Mode::Changes,
            ..Help::default()
        });
    }

    /// A key while the help is open: typing narrows it, ↑↓ and the page
    /// keys scroll it, Esc or ⏎ close it.
    pub(super) fn help_key(&mut self, mut help: Help, key: &KeyEvent) -> Vec<Action> {
        let scroll = match key.code {
            KeyCode::Up => Some(-1),
            KeyCode::Down => Some(1),
            KeyCode::PageUp => Some(-isize::try_from(PAGE).unwrap_or(1)),
            KeyCode::PageDown => Some(isize::try_from(PAGE).unwrap_or(1)),
            _ => None,
        };
        if let Some(by) = scroll {
            help.offset = help.offset.saturating_add_signed(by);
            self.keep_help(help);
            return Vec::new();
        }
        match help.filter.handle(key) {
            Edit::Submit | Edit::Cancel => self.close_help(help.over_changes),
            Edit::Changed => {
                help.offset = 0;
                self.keep_help(help);
            }
            Edit::Ignored => self.keep_help(help),
        }
        Vec::new()
    }

    /// The mouse while the help is open: the wheel scrolls it, and a click
    /// closes it.
    pub(super) fn help_mouse(&mut self, mut help: Help, event: MouseEvent) -> Vec<Action> {
        match event.kind {
            MouseEventKind::ScrollUp => help.offset = help.offset.saturating_sub(3),
            MouseEventKind::ScrollDown => help.offset += 3,
            MouseEventKind::Down(_) => {
                self.close_help(help.over_changes);
                return Vec::new();
            }
            _ => {}
        }
        self.keep_help(help);
        Vec::new()
    }

    /// Typed into the help.
    pub(super) fn help_paste(&mut self, mut help: Help, text: &str) {
        help.filter.paste(text);
        help.offset = 0;
        self.keep_help(help);
    }

    /// Keeps the help open, scrolled no further than its last line.
    fn keep_help(&mut self, mut help: Help) {
        let lines = help.lines(self.view, &self.keymap).len();
        help.offset = help.offset.min(lines.saturating_sub(1));
        self.mode = Mode::Help(help);
    }

    fn close_help(&mut self, over_changes: bool) {
        self.mode = if over_changes && self.changes.is_some() {
            Mode::Changes
        } else {
            Mode::Browse
        };
    }
}
