//! Every key of the screen shown, over it (`h`, or `?` from habit): so
//! the keys can be found without knowing them. Typing narrows the list to
//! the keys whose key or words hold what is typed; ↑↓ scroll it, and Esc
//! or ⏎ close it.

use crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

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

/// One key, or a few that do the same, and what it does.
pub(crate) type Key = (&'static str, &'static str);

/// Keys under a heading.
pub(crate) type Section = (&'static str, &'static [Key]);

/// One line of the help as shown: a heading, or a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HelpLine {
    Heading(&'static str),
    Key(Key),
}

const MOVING: Key = ("↑ ↓  j k", "move");
const TABS: Key = ("⇥ ⇧⇥", "the next or previous tab");
const TAB_NUMBERS: Key = ("1 2 3 4", "Sessions, Needs you, Tickets, Settings");
const BACK: Key = ("esc ⌫", "back to Sessions");
const HELP: Key = ("h ?", "these keys");
const QUIT: Key = ("q", "quit the UI (sessions keep running)");

/// What goes to a session's program, and what does not.
const IN_A_SESSION: Section = (
    "In a session",
    &[
        ("Shift-Q", "back to the list (or a click on it)"),
        ("Ctrl-1 Ctrl-2 Ctrl-3", "straight to a tab"),
        ("any other key", "goes to its program, esc and ⇥ too"),
    ],
);

const SESSIONS: &[Section] = &[
    (
        "On the list",
        &[
            MOVING,
            (
                "⏎",
                "type in the session beside the list; on a worktree a new session, on a project a new worktree",
            ),
            ("f", "the session full screen"),
            (
                "e",
                "fold or unfold a project or a worktree (on a session, its worktree)",
            ),
            ("a", "add a project"),
            ("w", "a new worktree for the project"),
            (
                "n",
                "a new session in the worktree, or a project's checkout",
            ),
            ("i", "the project's tickets"),
            (
                "d",
                "what the worktree or session has changed, and its turns",
            ),
            ("s", "stop the session"),
            ("r", "start an ended session again"),
            (
                "x",
                "close the session; remove the worktree or the project, after asking",
            ),
            ("c", "run the project's checks in the worktree"),
            ("v", "have Claude Code review the worktree's change"),
            ("t", "have an agent tidy the worktree's commits"),
            (
                "u",
                "move the worktree's branch onto its base, after asking",
            ),
            ("o", "push the branch and open a draft pull request"),
            ("b", "the branch's pull request in the browser"),
            ("PgUp PgDn End", "scroll the session beside the list"),
            ("right-click", "what can be done on a line, with its keys"),
        ],
    ),
    IN_A_SESSION,
    ("Anywhere", &[TABS, TAB_NUMBERS, BACK, HELP, QUIT]),
];

const NEEDS_YOU: &[Section] = &[
    (
        "On the queue",
        &[
            MOVING,
            ("⏎", "type in the session"),
            ("f", "the session full screen"),
            ("d", "what its workspace has changed"),
            ("n", "a new session in its worktree"),
            ("s", "stop the session"),
            ("r", "start an ended session again"),
            ("x", "close the session"),
            ("PgUp PgDn End", "scroll the session beside the list"),
            ("right-click", "what can be done on a line, with its keys"),
        ],
    ),
    IN_A_SESSION,
    ("Anywhere", &[TABS, TAB_NUMBERS, BACK, HELP, QUIT]),
];

const TICKETS: &[Section] = &[
    (
        "On the tickets",
        &[
            MOVING,
            ("⏎", "open the agent on the ticket, or start the first"),
            ("a", "start another agent on it, in a worktree of its own"),
            ("b", "the ticket in the browser"),
            (
                "x",
                "close the ticket in its tracker, after asking; then clean up",
            ),
            ("r", "fetch the tickets again"),
        ],
    ),
    (
        "Choosing an agent",
        &[
            ("⇥ ⇧⇥", "the agent"),
            ("← →", "what its worktree is made on top of"),
            ("↑ ↓", "which project, where several share the tickets"),
        ],
    ),
    ("Anywhere", &[TABS, TAB_NUMBERS, BACK, HELP, QUIT]),
];

const SETTINGS: &[Section] = &[
    (
        "On the settings",
        &[
            MOVING,
            (
                "⏎ space",
                "turn the setting on or off, go to its next choice, or open its dialog",
            ),
            ("← →", "a smaller or a larger choice"),
            ("r", "read the settings again, after editing a file by hand"),
        ],
    ),
    ("Anywhere", &[TABS, TAB_NUMBERS, BACK, HELP, QUIT]),
];

const CHANGES: &[Section] = &[
    (
        "In the changes",
        &[
            ("↑ ↓  j k", "choose a file or a turn; in a diff, scroll"),
            ("⏎ → l", "its diff"),
            ("PgUp PgDn space", "a page"),
            ("Home End  g G", "a diff's top or bottom"),
            ("esc ⌫ ← h", "back from a diff"),
            ("r", "ask git again"),
            (
                "z",
                "on a turn, put the files back as before it, after asking",
            ),
            ("d q Shift-Q", "back to the list"),
            ("?", "these keys"),
        ],
    ),
    ("Anywhere", &[TABS, ("Ctrl-C", "quit the UI")]),
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

    /// The lines shown for `view`: each heading with its keys that hold
    /// what is typed, ignoring case; a heading with none goes.
    pub(crate) fn lines(&self, view: View) -> Vec<HelpLine> {
        let wanted = self.filter.text().trim().to_lowercase();
        let holds = |(keys, what): &Key| {
            wanted.is_empty()
                || keys.to_lowercase().contains(&wanted)
                || what.to_lowercase().contains(&wanted)
        };
        let mut lines = Vec::new();
        for (heading, keys) in Self::sections(view, self.over_changes) {
            let shown: Vec<HelpLine> = keys
                .iter()
                .filter(|key| holds(key))
                .map(|key| HelpLine::Key(*key))
                .collect();
            if !shown.is_empty() {
                lines.push(HelpLine::Heading(heading));
                lines.extend(shown);
            }
        }
        lines
    }
}

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
        let lines = help.lines(self.view).len();
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
