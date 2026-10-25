//! The keys of Muster's screens (`PRODUCT.md` §6): each thing a key does
//! there is an [`Action`], by name, with the screens it works on, the keys
//! it has unless the person says otherwise, and what it does, so the
//! person can set any of them on the Settings screen (`[keys]` in
//! Muster's own settings) and the hints, the help and the menus show the
//! keys as set. Moving about (the arrows, the page keys, ⏎) and typing in
//! a field are not actions: they are the same everywhere. A name in
//! `[keys]` that is no action of Muster's, as a plugin's action is once
//! its plugin is removed, is kept aside and warned of, never refused: the
//! file still reads. Parsing does no IO (`ARC-01`).

use std::collections::BTreeMap;
use std::fmt;

#[cfg(all(test, feature = "ts"))]
mod export;
mod panels;

pub use panels::{PanelAction, is_panel_action, panel_action};

/// Where a key is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Screen {
    /// The *Sessions* list.
    Sessions,
    /// The *Needs you* queue.
    NeedsYou,
    /// The *Tickets* list.
    Tickets,
    /// The *Settings* list.
    Settings,
    /// A workspace's changes, while they have the keys.
    Changes,
    /// A session's terminal, while it has the keys: every key not an
    /// action here goes to its program.
    InSession,
    /// A session shown straight in a terminal tab of the person's own
    /// (`muster attach`, `muster enter`): every key but this screen's goes
    /// to its program.
    InTab,
    /// The key after the panel key (`panel_key`), on the list or typing in
    /// a panel: what it does to the panel with the keys. Nothing of it
    /// reaches a program.
    Prefix,
}

impl Screen {
    /// As the help and the Settings screen head it.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Sessions => "Sessions",
            Self::NeedsYou => "Needs you",
            Self::Tickets => "Tickets",
            Self::Settings => "Settings",
            Self::Changes => "In the changes",
            Self::InSession => "In a session",
            Self::InTab => "In a terminal tab",
            Self::Prefix => "After the panel key",
        }
    }
}

/// The four lists the tabs show.
const LISTS: &[Screen] = &[
    Screen::Sessions,
    Screen::NeedsYou,
    Screen::Tickets,
    Screen::Settings,
];

/// Where the tab keys go round the tabs: the lists, and a workspace's
/// changes, which they leave.
const TABS: &[Screen] = &[
    Screen::Sessions,
    Screen::NeedsYou,
    Screen::Tickets,
    Screen::Settings,
    Screen::Changes,
];

/// The lists of sessions.
const SESSION_LISTS: &[Screen] = &[Screen::Sessions, Screen::NeedsYou];

const SESSIONS: &[Screen] = &[Screen::Sessions];

/// After the panel key.
const PREFIX: &[Screen] = &[Screen::Prefix];

/// Everywhere the list or a session beside it has the keys.
const EVERYWHERE: &[Screen] = &[
    Screen::Sessions,
    Screen::NeedsYou,
    Screen::Tickets,
    Screen::Settings,
    Screen::Changes,
    Screen::InSession,
];

macro_rules! actions {
    ($($action:ident $name:literal $screens:expr, [$($key:literal),*], $what:literal;)*) => {
        /// What a key does.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action {
            $($action,)*
        }

        impl Action {
            /// Every action, in the order the Settings screen lists them.
            pub const ALL: &'static [Self] = &[$(Self::$action,)*];

            /// Its name in `[keys]`.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$action => $name,)*
                }
            }

            /// The screens it works on.
            pub const fn screens(self) -> &'static [Screen] {
                match self {
                    $(Self::$action => $screens,)*
                }
            }

            /// Its keys unless the person sets others, as written in `[keys]`.
            pub const fn defaults(self) -> &'static [&'static str] {
                match self {
                    $(Self::$action => &[$($key,)*],)*
                }
            }

            /// What it does, for the help and the Settings screen.
            pub const fn what(self) -> &'static str {
                match self {
                    $(Self::$action => $what,)*
                }
            }
        }
    };
}

actions! {
    Quit "quit" LISTS, ["q"], "quit the UI (sessions keep running)";
    NextTab "next_tab" TABS, ["tab"], "the next tab";
    PreviousTab "previous_tab" TABS, ["shift-tab"], "the previous tab";
    SessionsTab "sessions_tab" LISTS, ["1"], "the Sessions tab";
    NeedsYouTab "needs_you_tab" LISTS, ["2"], "the Needs you tab";
    TicketsTab "tickets_tab" LISTS, ["3"], "the Tickets tab";
    SettingsTab "settings_tab" LISTS, ["4"], "the Settings tab";
    Back "back" &[Screen::NeedsYou, Screen::Tickets, Screen::Settings], ["esc", "backspace"],
        "back to Sessions";
    LastAlarm "last_alarm" LISTS, ["l"], "the session that needed you last, as notified";
    Help "help" LISTS, ["h", "?"], "these keys";
    FoldList "fold_list" EVERYWHERE, ["ctrl-\\"], "fold the list away, or bring it back";
    MovePanels "move_panels" LISTS, ["m"], "move the panels with the keys (or drag one by its title)";
    PanelKey "panel_key" EVERYWHERE, ["ctrl-b"],
        "wait for one more key, to split, zoom, move or close the panel with the keys";
    MoveUp "move_up" LISTS, ["k"], "move up (as ↑)";
    MoveDown "move_down" LISTS, ["j"], "move down (as ↓)";
    AddProject "add_project" SESSION_LISTS, ["a"], "add a project";
    CloneProject "clone_project" SESSION_LISTS, ["g"], "clone a project from GitHub, into a folder you choose";
    NewWorktree "new_worktree" SESSION_LISTS, ["w"], "a new worktree for the project";
    NewSession "new_session" SESSION_LISTS, ["n"], "a new session in the worktree, or a project's checkout";
    ShowTickets "show_tickets" SESSION_LISTS, ["i"], "the project's tickets";
    FullScreen "full_screen" SESSION_LISTS, ["f"], "the session full screen";
    ShowChanges "show_changes" SESSION_LISTS, ["d"], "what the worktree or session has changed, and its turns";
    Shell "shell" SESSION_LISTS, ["T"], "a shell in the workspace, beside the panel with the keys (the one off the screen there, else a new one)";
    PinSession "pin_session" SESSION_LISTS, ["P"], "place the session in a panel of its own, at the right of the one with the keys (again: off the screen; or drag its line, or its menu)";
    StopSession "stop_session" SESSION_LISTS, ["s"], "stop the session";
    RestartSession "restart_session" SESSION_LISTS, ["r"], "start an ended session again";
    Close "close" SESSION_LISTS, ["x"], "close the session; remove the worktree or the project, after asking";
    Rename "rename" SESSION_LISTS, ["R"], "rename the session's chat";
    Spaces "spaces" SESSION_LISTS, ["W"], "spaces: go to one, make, rename or remove one, or put the line's project or terminal in one";
    NewTerminal "new_terminal" SESSION_LISTS, ["+"], "a new terminal in no project, in the home folder (in the space shown)";
    Fold "fold" SESSIONS, ["e"], "fold or unfold a project or a worktree (on a session, its worktree)";
    Checks "checks" SESSIONS, ["c"], "run the project's checks in the worktree";
    Review "review" SESSIONS, ["v"], "have Claude Code review the worktree's change";
    Tidy "tidy" SESSIONS, ["t"], "have an agent tidy the worktree's commits";
    MoveBranch "move_branch" SESSIONS, ["u"], "move the worktree's branch onto its base, after asking";
    OpenPullRequest "open_pull_request" SESSIONS, ["o"], "push the branch and open a draft pull request";
    PullRequestInBrowser "pull_request_in_browser" SESSIONS, ["b"], "the branch's pull request in the browser";
    NextSpace "next_space" SESSIONS, ["]"], "the next space the list shows (after the last, all of it)";
    PreviousSpace "previous_space" SESSIONS, ["["], "the space before";
    FileTerminal "file_terminal" SESSIONS, ["F"], "put a terminal in no project under a project: where its shell is, or started again there";
    RefreshTickets "refresh_tickets" &[Screen::Tickets], ["r"], "fetch the tickets again";
    AnotherAgent "another_agent" &[Screen::Tickets], ["a"], "start another agent on it, in a worktree of its own";
    TicketInBrowser "ticket_in_browser" &[Screen::Tickets], ["b"], "the ticket in the browser";
    CloseTicket "close_ticket" &[Screen::Tickets], ["x"], "close the ticket in its tracker, after asking; then clean up";
    NextPart "next_part" &[Screen::Tickets], ["]"], "the ticket's next part: its text, then each comment";
    PreviousPart "previous_part" &[Screen::Tickets], ["["], "the ticket's previous part";
    ChoosePart "choose_part" &[Screen::Tickets], ["space"], "give the agent this part of the ticket, or not";
    ReadSettings "read_settings" &[Screen::Settings], ["r"], "read the settings again, after editing a file by hand";
    AddPlugin "add_plugin" &[Screen::Settings], ["a"], "add a plugin from a git repository or a folder, after showing what it runs";
    UpdatePlugin "update_plugin" &[Screen::Settings], ["u"], "update the plugin, after showing what it runs then";
    RemovePlugin "remove_plugin" &[Screen::Settings], ["x"], "remove the plugin, after asking";
    CloseChanges "close_changes" &[Screen::Changes], ["d", "q"], "back to the list";
    ChangesAgain "changes_again" &[Screen::Changes], ["r"], "ask git again";
    PutBack "put_back" &[Screen::Changes], ["z"], "on a turn, put the files back as before it, after asking";
    ChangesHelp "changes_help" &[Screen::Changes], ["?"], "these keys";
    LeaveSession "leave_session" &[Screen::InSession, Screen::Changes], ["Q", "ctrl-]"],
        "back to the list (or a click on it)";
    LeaveTab "leave_tab" &[Screen::InTab], ["ctrl-q"],
        "back to the session shown before in the tab, else to your own shell (the session keeps running)";
    SplitRight "split_right" PREFIX, ["v"], "split the panel right, a new shell in its workspace";
    SplitDown "split_down" PREFIX, ["s", "-"], "split the panel down, a new shell in its workspace";
    FocusLeft "focus_left" PREFIX, ["h"], "the keys to the panel on the left (← too)";
    FocusDown "focus_down" PREFIX, ["j"], "the keys to the panel below (↓ too)";
    FocusUp "focus_up" PREFIX, ["k"], "the keys to the panel above (↑ too)";
    FocusRight "focus_right" PREFIX, ["l"], "the keys to the panel on the right (→ too)";
    SwapLeft "swap_left" PREFIX, ["H"], "swap the panel with the one on the left (⇧← too)";
    SwapDown "swap_down" PREFIX, ["J"], "swap the panel with the one below (⇧↓ too)";
    SwapUp "swap_up" PREFIX, ["K"], "swap the panel with the one above (⇧↑ too)";
    SwapRight "swap_right" PREFIX, ["L"], "swap the panel with the one on the right (⇧→ too)";
    Zoom "zoom" PREFIX, ["z"], "zoom the panel over the others beside the list, or put it back";
    TakeOff "take_off" PREFIX, ["x"], "take the panel off the screen; its session runs on";
    ClosePanel "close_panel" PREFIX, ["X"], "close the panel's session, after asking while it runs";
    PlacePreview "place_preview" PREFIX, ["p"], "place the preview's session where the preview is";
    Resize "resize" PREFIX, ["r"],
        "resize the panel: h j k l move its edge a cell, H J K L five, ⏎ keeps it, esc puts it back";
    ToPanel "to_panel" PREFIX, ["1", "2", "3", "4", "5", "6", "7", "8", "9"],
        "the keys to the panel of that number, as its title shows while the panel key waits";
    ToList "to_list" PREFIX, ["w"], "the keys back to the list";
    SendPanelKey "send_panel_key" PREFIX, ["b"],
        "send the panel key to the program, as pressing it again does (Claude Code backgrounds a command on Ctrl-B)";
    PanelKeys "panel_keys" PREFIX, ["?"], "these keys";
}

impl Action {
    /// Where it is listed among the keys: on every list, on the lists of
    /// sessions, or on one screen.
    pub fn group(self) -> &'static str {
        match self.screens() {
            [Screen::InSession, ..] => "In a session",
            screens if screens.len() >= 3 => "Anywhere",
            [Screen::Sessions, Screen::NeedsYou] => "Sessions and Needs you",
            [only, ..] => only.title(),
            [] => "",
        }
    }

    /// The action named `name` in `[keys]`.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.name() == name)
    }
}

/// A key a person presses: a character, or a key with a name, with Ctrl or
/// Alt. A capital letter is the letter with Shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Chord {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
}

/// A key by itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Key {
    Char(char),
    Tab,
    BackTab,
    Esc,
    Backspace,
    Enter,
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    F(u8),
}

/// Why a key or the person's keys do not read: said for the person,
/// naming what is wrong.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeysError {
    #[error(
        "{0:?} is not a key: a key is a character (a, R, ?), or a named one (tab, esc, \
         backspace, f5), after ctrl- or alt- if it has them"
    )]
    NotAKey(String),
    #[error("keys.{action}: {key} is {used}, which is the same on every screen")]
    Kept {
        action: String,
        key: String,
        used: &'static str,
    },
    #[error("keys.{action}: {key} is {other}'s on the same screen")]
    Taken {
        action: String,
        key: String,
        other: String,
    },
    #[error(
        "keys.{action}: {key} would never reach a session's program; a key there needs ctrl \
         or alt, or is a capital letter"
    )]
    Typed { action: String, key: String },
    #[error(
        "keys.{action}: {key} cannot be told from typing in a terminal tab; a key there is a \
         letter with ctrl or alt"
    )]
    InTab { action: String, key: String },
}

impl Chord {
    /// A key with nothing held.
    pub const fn plain(key: Key) -> Self {
        Self {
            key,
            ctrl: false,
            alt: false,
        }
    }

    /// A key as `[keys]` writes it: `a`, `R`, `?`, `ctrl-]`, `alt-x`,
    /// `shift-q` (`Q`), `tab`, `shift-tab`, `esc`, `backspace`, `space`,
    /// `f5`.
    pub fn parse(text: &str) -> Result<Self, KeysError> {
        let not = || KeysError::NotAKey(text.to_owned());
        let mut chord = Self::plain(Key::Esc);
        let mut shift = false;
        let mut rest = text.trim();
        loop {
            let lower = rest.to_ascii_lowercase();
            if let Some(after) = lower
                .strip_prefix("ctrl-")
                .filter(|after| !after.is_empty())
            {
                chord.ctrl = true;
                rest = &rest[rest.len() - after.len()..];
            } else if let Some(after) = lower.strip_prefix("alt-").filter(|after| !after.is_empty())
            {
                chord.alt = true;
                rest = &rest[rest.len() - after.len()..];
            } else if let Some(after) = lower
                .strip_prefix("shift-")
                .filter(|after| !after.is_empty())
            {
                shift = true;
                rest = &rest[rest.len() - after.len()..];
            } else {
                break;
            }
        }
        let mut chars = rest.chars();
        chord.key = match (chars.next(), chars.next()) {
            (Some(c), None) if !c.is_control() => {
                Key::Char(if shift { c.to_ascii_uppercase() } else { c })
            }
            _ => match rest.to_ascii_lowercase().as_str() {
                "tab" if shift => Key::BackTab,
                "tab" => Key::Tab,
                "backtab" => Key::BackTab,
                "esc" | "escape" => Key::Esc,
                "backspace" => Key::Backspace,
                "enter" | "return" => Key::Enter,
                "space" => Key::Char(' '),
                "up" => Key::Up,
                "down" => Key::Down,
                "left" => Key::Left,
                "right" => Key::Right,
                "pageup" => Key::PageUp,
                "pagedown" => Key::PageDown,
                "home" => Key::Home,
                "end" => Key::End,
                name => match name.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
                    Some(n @ 1..=24) => Key::F(n),
                    _ => return Err(not()),
                },
            },
        };
        Ok(chord)
    }

    /// As people read it: `r`, `Shift-R` for a capital letter,
    /// `Ctrl-]`, `⏎`, `⇥`.
    pub fn label(self) -> String {
        let key = match self.key {
            Key::Char(' ') => "space".to_owned(),
            Key::Char(c) if c.is_ascii_uppercase() => format!("Shift-{c}"),
            Key::Char(c) => c.to_string(),
            Key::Tab => "⇥".to_owned(),
            Key::BackTab => "⇧⇥".to_owned(),
            Key::Esc => "esc".to_owned(),
            Key::Backspace => "⌫".to_owned(),
            Key::Enter => "⏎".to_owned(),
            Key::Up => "↑".to_owned(),
            Key::Down => "↓".to_owned(),
            Key::Left => "←".to_owned(),
            Key::Right => "→".to_owned(),
            Key::PageUp => "PgUp".to_owned(),
            Key::PageDown => "PgDn".to_owned(),
            Key::Home => "Home".to_owned(),
            Key::End => "End".to_owned(),
            Key::F(n) => format!("F{n}"),
        };
        match (self.ctrl, self.alt) {
            (true, true) => format!("Ctrl-Alt-{key}"),
            (true, false) => format!("Ctrl-{key}"),
            (false, true) => format!("Alt-{key}"),
            (false, false) => key,
        }
    }

    /// Whether a session's program could never take it as typed: with
    /// Ctrl or Alt, or a capital letter.
    fn untyped(self) -> bool {
        self.ctrl || self.alt || matches!(self.key, Key::Char(c) if c.is_ascii_uppercase())
    }

    /// Whether a terminal tab can tell it from typing, in what the person's
    /// terminal sends whatever keyboard mode the program asked for: a
    /// lower-case letter with Ctrl or Alt.
    pub fn in_tab(self) -> bool {
        (self.ctrl || self.alt) && matches!(self.key, Key::Char(c) if c.is_ascii_lowercase())
    }
}

impl fmt::Display for Chord {
    /// As `[keys]` writes it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("ctrl-")?;
        }
        if self.alt {
            f.write_str("alt-")?;
        }
        match self.key {
            Key::Char(' ') => f.write_str("space"),
            Key::Char(c) => write!(f, "{c}"),
            Key::Tab => f.write_str("tab"),
            Key::BackTab => f.write_str("shift-tab"),
            Key::Esc => f.write_str("esc"),
            Key::Backspace => f.write_str("backspace"),
            Key::Enter => f.write_str("enter"),
            Key::Up => f.write_str("up"),
            Key::Down => f.write_str("down"),
            Key::Left => f.write_str("left"),
            Key::Right => f.write_str("right"),
            Key::PageUp => f.write_str("pageup"),
            Key::PageDown => f.write_str("pagedown"),
            Key::Home => f.write_str("home"),
            Key::End => f.write_str("end"),
            Key::F(n) => write!(f, "f{n}"),
        }
    }
}

/// Keys that are the same on every screen, never an action's: moving
/// about, ⏎, and Ctrl-C, which quits.
fn kept(chord: Chord) -> Option<&'static str> {
    if chord.ctrl && chord.key == Key::Char('c') && !chord.alt {
        return Some("Ctrl-C, which quits");
    }
    if chord.ctrl || chord.alt {
        return None;
    }
    match chord.key {
        Key::Up | Key::Down | Key::Left | Key::Right | Key::PageUp | Key::PageDown => {
            Some("a key that moves about")
        }
        Key::Home | Key::End => Some("a key that moves about"),
        Key::Enter => Some("⏎, which does what is selected"),
        _ => None,
    }
}

/// What reads in the person's keys but does nothing: said for the
/// person, naming the key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum KeysWarning {
    #[error(
        "keys.{0} is not something a key does in Muster: kept, for a plugin's action of that \
         name, and doing nothing meanwhile"
    )]
    NotAnAction(String),
    /// A key the person set for a plugin's panel that cannot be its.
    #[error("keys.{name}: {key} {why}, so it does nothing for the panel: set it another")]
    PanelKey {
        name: String,
        key: String,
        why: String,
    },
    /// A key a plugin suggests for its panel that cannot be its.
    #[error(
        "the {plugin} plugin's {panel} panel suggests {key}, which {why}: set it a key of its \
         own here"
    )]
    Suggested {
        plugin: String,
        panel: String,
        key: String,
        why: String,
    },
}

/// The keys of every action: Muster's own, with the person's over them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Keymap {
    keys: BTreeMap<Action, Vec<Chord>>,
    /// The names set that are no action of Muster's, with their keys as
    /// written: a plugin's actions, bound by name once its plugin is
    /// known.
    aside: BTreeMap<String, Vec<String>>,
    /// The panels of the plugins that run, with their keys
    /// ([`Keymap::with_panels`]).
    panels: Vec<panels::Bound>,
    /// The keys of panels that cannot be theirs, each said why.
    dropped: Vec<KeysWarning>,
}

impl Default for Keymap {
    fn default() -> Self {
        let keys = Action::ALL
            .iter()
            .map(|action| {
                let chords = action
                    .defaults()
                    .iter()
                    .filter_map(|key| Chord::parse(key).ok())
                    .collect();
                (*action, chords)
            })
            .collect();
        Self {
            keys,
            aside: BTreeMap::new(),
            panels: Vec::new(),
            dropped: Vec::new(),
        }
    }
}

impl Keymap {
    /// Muster's keys with the person's `set` over them (`[keys]`: each
    /// action's name and its keys as written; no keys leaves an action
    /// without one). Refused, naming the key, where a key is not one, is
    /// one every screen keeps, is another action's on a screen they share,
    /// or could never reach a session's program. A name that is no action
    /// of Muster's is kept aside with its keys as written, unread, and
    /// warned of ([`Keymap::warnings`]).
    pub fn with(set: &BTreeMap<String, Vec<String>>) -> Result<Self, KeysError> {
        let mut map = Self::default();
        for (name, keys) in set {
            let Some(action) = Action::named(name) else {
                map.aside.insert(name.clone(), keys.clone());
                continue;
            };
            let chords = keys
                .iter()
                .map(|key| Chord::parse(key))
                .collect::<Result<Vec<_>, _>>()?;
            map.keys.insert(action, chords);
        }
        for action in Action::ALL {
            for chord in map.chords(*action) {
                map.check(*action, *chord)?;
            }
        }
        Ok(map)
    }

    /// Whether `chord` may be `action`'s, beside the others' keys.
    pub fn check(&self, action: Action, chord: Chord) -> Result<(), KeysError> {
        let named = || (action.name().to_owned(), chord.label());
        if let Some(used) = kept(chord) {
            let (action, key) = named();
            return Err(KeysError::Kept { action, key, used });
        }
        if action.screens().contains(&Screen::InSession) && !chord.untyped() {
            let (action, key) = named();
            return Err(KeysError::Typed { action, key });
        }
        if action.screens().contains(&Screen::InTab) && !chord.in_tab() {
            let (action, key) = named();
            return Err(KeysError::InTab { action, key });
        }
        let other = self
            .taken(action, chord)
            .map(|other| other.name().to_owned())
            .or_else(|| self.taken_by_panel(action, chord));
        if let Some(other) = other {
            let (action, key) = named();
            return Err(KeysError::Taken { action, key, other });
        }
        Ok(())
    }

    /// Another action that has `chord` on a screen `action` works on.
    pub fn taken(&self, action: Action, chord: Chord) -> Option<Action> {
        self.keys
            .iter()
            .filter(|(other, _)| **other != action)
            .filter(|(other, _)| {
                other
                    .screens()
                    .iter()
                    .any(|screen| action.screens().contains(screen))
            })
            .find(|(_, chords)| chords.contains(&chord))
            .map(|(other, _)| *other)
    }

    /// What `chord` does on `screen`, if anything.
    pub fn action(&self, screen: Screen, chord: Chord) -> Option<Action> {
        self.keys
            .iter()
            .find(|(action, chords)| action.screens().contains(&screen) && chords.contains(&chord))
            .map(|(action, _)| *action)
    }

    /// `action`'s keys, its first the one shown in hints.
    pub fn chords(&self, action: Action) -> &[Chord] {
        self.keys.get(&action).map_or(&[], Vec::as_slice)
    }

    /// `action`'s first key as people read it; `none` where it has none.
    pub fn label(&self, action: Action) -> String {
        self.chords(action)
            .first()
            .map_or_else(|| "none".to_owned(), |chord| chord.label())
    }

    /// Each of `action`'s keys as people read them, spaced.
    pub fn labels(&self, action: Action) -> String {
        let labels: Vec<String> = self
            .chords(action)
            .iter()
            .map(|chord| chord.label())
            .collect();
        if labels.is_empty() {
            "none".to_owned()
        } else {
            labels.join(" ")
        }
    }

    /// Whether `action` has the keys it has by default.
    pub fn is_default(&self, action: Action) -> bool {
        Self::default().chords(action) == self.chords(action)
    }

    /// The names set that are no action of Muster's, with their keys as
    /// written, kept for a plugin's actions of those names.
    pub fn aside(&self) -> &BTreeMap<String, Vec<String>> {
        &self.aside
    }

    /// What the person is told of the keys that do nothing: each name kept
    /// aside that no panel of a plugin that runs has, and each panel's key
    /// that cannot be its.
    pub fn warnings(&self) -> Vec<KeysWarning> {
        self.aside
            .keys()
            .filter(|name| self.panel_named(name).is_none())
            .map(|name| KeysWarning::NotAnAction(name.clone()))
            .chain(self.dropped.iter().cloned())
            .collect()
    }
}

#[cfg(test)]
mod tests;
