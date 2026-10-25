//! The settings a person changes on the UI's Settings screen, as the
//! protocol carries them (`ARCHITECTURE.md` §12). A change goes into the
//! person's own overrides of the project's `muster.toml`, never into the
//! repository (`ARC-05`).

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::layout::Layout;

/// A project's settings that the person can change, as they stand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ProjectSettings {
    /// Where its tickets come from, for the person: `GitHub issues, as its
    /// git remote says`, `Jira at acme.atlassian.net, project APP`, or why
    /// that cannot be told.
    pub tickets_from: String,
    /// Only the tickets assigned to the person signed in, in any tracker
    /// (`[tickets] mine`).
    pub only_mine: Toggle,
    /// The person's own settings file for the project, where changes go.
    pub file: PathBuf,
    /// Where its tickets are as written, the person's settings over the
    /// repository's `muster.toml`; `None` where no tracker is written and the
    /// git remote tells.
    #[serde(default)]
    pub tickets: Option<TicketsFrom>,
    /// Which file names the tracker (`[tickets] tracker`).
    #[serde(default)]
    pub tickets_set_by: SettingSource,
    /// Nothing says where its tickets are, and its git remote cannot tell:
    /// `tickets_from` says why, and the person has to choose.
    #[serde(default)]
    pub tickets_untold: bool,
    /// Whether the branches Muster makes start with `muster/` (`[branches]
    /// muster_prefix`).
    #[serde(default)]
    pub muster_prefix: Option<Toggle>,
}

/// A setting that is on or off, and where that comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Toggle {
    pub on: bool,
    pub from: SettingSource,
}

/// Where a setting's value comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    /// Nothing sets it: Muster's default.
    #[default]
    Default,
    /// The project's `muster.toml`, in its repository.
    Project,
    /// The person's own settings for the project, which win.
    Yours,
}

/// Where a project's tickets are, as `[tickets]` writes it: the ticket
/// plugin, by its id, and what it needs to find them, as its manifest
/// declares (`[[tickets.settings]]`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TicketsFrom {
    /// The ticket plugin: `github`, `gitlab`, `jira`, `linear`, or
    /// anyone's.
    pub tracker: String,
    /// What `[tickets]` says for it, by key: Jira's `site` and `project`.
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
}

impl TicketsFrom {
    /// Each key of `[tickets]` that says where the tickets are, with what
    /// this sets it to.
    pub fn written(&self) -> Vec<(&str, &str)> {
        std::iter::once(("tracker", self.tracker.as_str()))
            .chain(
                self.fields
                    .iter()
                    .map(|(key, value)| (key.as_str(), value.as_str())),
            )
            .collect()
    }
}

/// A change the person makes to a project's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum SettingChange {
    /// List only the tickets assigned to the person, or every open one.
    OnlyMyTickets(bool),
    /// Where the tickets are; `None` follows the repository's `muster.toml` and
    /// the git remote again.
    TicketsFrom(Option<TicketsFrom>),
    /// Whether the branches Muster makes start with `muster/`.
    MusterPrefix(bool),
}

/// Where the branches' prefix is written: `[branches] muster_prefix`.
pub const MUSTER_PREFIX: (&str, &str) = ("branches", "muster_prefix");

/// Muster's own settings, the person's for all of Muster rather than a
/// project's, as they stand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(default)]
pub struct Preferences {
    /// Show the agents in the MacBook's notch while no Muster window has
    /// the person's eyes (`[notch] show`).
    pub notch: bool,
    /// Where they are written: the person's own file, never a repository.
    pub file: PathBuf,
    /// The coding agents the person added there (`[agents.<name>]`), by name,
    /// offered beside Muster's own.
    pub agents: Vec<AddedAgent>,
    /// Why the file does not read, naming the key, while it does not: the rest
    /// are then Muster's defaults.
    pub problem: Option<String>,
    /// The fewest columns a window has for a session's terminal to sit beside
    /// the list; in a narrower one, the list or the session fills it. 0 keeps
    /// them side by side at any width (`[layout] side_by_side_from`).
    pub side_by_side_from: u16,
    /// Whose colours the UI draws in (`[look] colours`).
    pub palette: Palette,
    /// How many lines a session's line in the lists takes (`[look]
    /// session_lines`).
    pub session_lines: SessionLines,
    /// Where the UI's panels are (`[layout] panels`).
    pub panels: Layout,
    /// Whether the list is folded away, the session taking its place (`[layout]
    /// list_folded`).
    pub list_folded: bool,
    /// The projects in the order the person dragged them to (`[layout]
    /// projects`), the others after them as the daemon lists them; empty
    /// until the person moves one. An id of a project since removed is
    /// passed over.
    pub projects: Vec<crate::ProjectId>,
    /// How chats are named by an agent (`[names]`).
    pub naming: Naming,
    /// The keys the person set, by action, as written (`[keys]`,
    /// `muster_core::keys`): the actions not named keep Muster's.
    pub keys: std::collections::BTreeMap<String, Vec<String>>,
    /// Whether the person is told of what newly needs them while they look away
    /// (`[notifications] show`); on unless they turn it off.
    pub notifications: bool,
    /// Which kinds the person is told of, and how each sounds
    /// (`[notifications]` `questions`, `finishes`, `errors` and their
    /// `_sound`s).
    pub telling: crate::telling::Telling,
    /// Whether the worktrees Muster makes for a Rust project build into one
    /// folder of the project's, so its dependencies are built and kept once
    /// rather than once a worktree (`[worktrees] shared_builds`); on unless the
    /// person turns it off.
    pub shared_builds: bool,
    /// How far agents may set agents working through Muster's command line
    /// (`[delegation]`, `PRD-01`).
    pub delegation: crate::delegation::Delegation,
}

/// How chats are named by an agent (`PRODUCT.md` §6): the first line of the
/// first prompt at once, then a few words an agent writes.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Naming {
    /// Whether an agent names chats (`[names] by_agent`); on unless the
    /// person turns it off.
    pub by_agent: bool,
    /// The agent that names them, by its id; `None` for each session's
    /// own (`[names] agent`).
    #[serde(default)]
    pub agent: Option<String>,
    /// The model it names them with; `None` for its own choice
    /// (`[names] model`).
    #[serde(default)]
    pub model: Option<String>,
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            by_agent: true,
            agent: None,
            model: None,
        }
    }
}

/// Where naming by an agent is written: `[names] by_agent`, `agent` and
/// `model`.
pub const NAMES: &str = "names";

impl Default for Preferences {
    fn default() -> Self {
        Self {
            notch: true,
            file: PathBuf::new(),
            agents: Vec::new(),
            problem: None,
            side_by_side_from: SIDE_BY_SIDE_FROM,
            palette: Palette::default(),
            session_lines: SessionLines::default(),
            panels: Layout::default(),
            list_folded: false,
            projects: Vec::new(),
            naming: Naming::default(),
            keys: std::collections::BTreeMap::new(),
            notifications: true,
            telling: crate::telling::Telling::default(),
            shared_builds: true,
            delegation: crate::delegation::Delegation::default(),
        }
    }
}

/// Whose colours the UI draws in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum Palette {
    /// Muster's own, in full colour: soft text, one accent, the statuses'
    /// colours, its shades taken from the terminal's background, so it
    /// reads the same in any terminal.
    #[default]
    Muster,
    /// The terminal's own sixteen, so the UI follows the person's theme.
    Terminal,
}

impl Palette {
    /// As `[look] colours` writes it.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Muster => "muster",
            Self::Terminal => "terminal",
        }
    }

    /// `[look] colours` as written: `muster` or `terminal`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "muster" => Some(Self::Muster),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}

/// How many lines a session's line in the lists takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "lowercase")]
pub enum SessionLines {
    /// Its name on the first, whole, and what it is doing and for how long
    /// under it.
    #[default]
    Two,
    /// All of it on one, so twice as many fit.
    One,
}

impl SessionLines {
    /// How many lines it is: as `[look] session_lines` writes it.
    #[must_use]
    pub const fn count(self) -> u16 {
        match self {
            Self::Two => 2,
            Self::One => 1,
        }
    }

    /// `[look] session_lines` as written: 1 or 2.
    #[must_use]
    pub const fn from_count(count: u16) -> Option<Self> {
        match count {
            2 => Some(Self::Two),
            1 => Some(Self::One),
            _ => None,
        }
    }
}

/// Where the palette is written: `[look] colours`.
pub const LOOK_COLOURS: (&str, &str) = ("look", "colours");

/// Where the lines a session takes are written: `[look] session_lines`.
pub const LOOK_SESSION_LINES: (&str, &str) = ("look", "session_lines");

/// A coding agent the person added in Muster's own settings, as clients offer
/// it beside Muster's own (`muster_core::config::agents`). Only what a client
/// shows: never its program's environment.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AddedAgent {
    /// What sessions name it by: a harness id, like `claude`.
    pub name: String,
    /// What it is called on screen.
    pub label: String,
    /// One character, shown where Muster's own agents show theirs.
    pub mark: String,
    /// The mark's colour: a name from the terminal's sixteen (`green`,
    /// `light-blue`) or `#rrggbb`.
    #[serde(default)]
    pub colour: Option<String>,
    /// Whether it takes a first prompt, as a ticket's agent is given its
    /// ticket.
    pub takes_prompt: bool,
    /// Whether it can be asked one question, and so name chats.
    #[serde(default)]
    pub asks: bool,
}

/// From how many columns a session sits beside the list unless the person
/// says otherwise: the 34 the list takes at least, its border and a
/// session 64 columns wide, which still holds an agent's prompt and a
/// diff's lines without wrapping them; narrower, the session would wrap
/// what it draws.
pub const SIDE_BY_SIDE_FROM: u16 = 100;

/// The widths the Settings screen offers for
/// [`Preferences::side_by_side_from`], narrowest first: 0 for any width.
pub const SIDE_BY_SIDE_CHOICES: [u16; 8] = [0, 80, 90, 100, 110, 120, 140, 160];

/// A change the person makes to Muster's own settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum PreferenceChange {
    /// Show the agents in the notch, or leave it alone.
    Notch(bool),
    /// From how many columns a session sits beside the list; 0 at any width.
    SideBySideFrom(u16),
    /// Whose colours the UI draws in.
    Palette(Palette),
    /// How many lines a session's line in the lists takes.
    SessionLines(SessionLines),
    /// Where the panels are.
    Panels(Layout),
    /// Fold the list away, or bring it back.
    ListFolded(bool),
    /// The projects in this order, first to last; empty, in the order they
    /// were added (protocol 56).
    ProjectOrder(Vec<crate::ProjectId>),
    /// Have an agent name chats, or not.
    NamesByAgent(bool),
    /// The agent that names chats (`None`: each session's own) and its model
    /// (`None`: its own choice).
    NamedBy {
        agent: Option<String>,
        model: Option<String>,
    },
    /// An action's keys, as written (`ctrl-]`), or `None` for Muster's own.
    Keys {
        action: String,
        keys: Option<Vec<String>>,
    },
    /// Every key as Muster has it.
    AllKeysBack,
    /// Tell the person of what newly needs them while they look away, or not.
    Notifications(bool),
    /// Whether `kind` is told, and how it sounds.
    Tell {
        kind: crate::telling::Kind,
        tell: crate::telling::Tell,
    },
    /// Have the worktrees of a Rust project build into one folder, or each into
    /// its own.
    SharedBuilds(bool),
    /// Let agents set agents working through Muster's command line, or not
    /// (`PRD-01`).
    AgentsStartAgents(bool),
}

/// Where sharing builds is written: `[worktrees] shared_builds`.
pub const SHARED_BUILDS: (&str, &str) = ("worktrees", "shared_builds");

/// Where telling the person is written: `[notifications] show`.
pub const NOTIFICATIONS: (&str, &str) = ("notifications", "show");

/// Where the person's keys are written: `[keys]`.
pub const KEYS: &str = "keys";

/// Where the panels are written: `[layout] panels`.
pub const PANELS: (&str, &str) = ("layout", "panels");

/// Where the projects' order is written: `[layout] projects`.
pub const PROJECTS: (&str, &str) = ("layout", "projects");

/// Where the list's folding is written: `[layout] list_folded`.
pub const LIST_FOLDED: (&str, &str) = ("layout", "list_folded");

/// Where it is written: `[layout] side_by_side_from`.
pub const SIDE_BY_SIDE: (&str, &str) = ("layout", "side_by_side_from");

/// Where showing the notch is written: `[notch] show`.
pub const NOTCH: (&str, &str) = ("notch", "show");

/// Where only-my-tickets is written: `[tickets] mine`.
pub const ONLY_MINE: (&str, &str) = ("tickets", "mine");

/// Where the tracker is written: `[tickets] tracker`.
pub const TRACKER: (&str, &str) = ("tickets", "tracker");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_from_an_older_daemon_still_read() {
        // Protocol 16 sent no word of where the tickets are written.
        #[derive(Serialize)]
        struct Old {
            tickets_from: String,
            only_mine: Toggle,
            file: PathBuf,
        }
        let old = Old {
            tickets_from: "GitHub issues, as its git remote says".to_owned(),
            only_mine: Toggle {
                on: true,
                from: SettingSource::Yours,
            },
            file: PathBuf::from("/c/p.toml"),
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let read: ProjectSettings = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read.tickets, None);
        assert_eq!(read.tickets_set_by, SettingSource::Default);
        assert!(read.only_mine.on);
    }

    #[test]
    fn muster_s_settings_from_an_older_daemon_read_in_muster_s_own_look() {
        // Protocol 34 sent no word of the look.
        #[derive(Serialize)]
        struct Old {
            notch: bool,
            file: PathBuf,
            side_by_side_from: u16,
        }
        let old = Old {
            notch: false,
            file: PathBuf::from("/c/settings.toml"),
            side_by_side_from: 120,
        };
        let bytes = rmp_serde::to_vec_named(&old).unwrap();
        let read: Preferences = rmp_serde::from_slice(&bytes).unwrap();
        assert_eq!(read.palette, Palette::Muster);
        assert_eq!(read.session_lines, SessionLines::Two);
        assert_eq!(read.side_by_side_from, 120);
    }

    #[test]
    fn the_look_reads_as_written_and_nothing_else() {
        for palette in [Palette::Muster, Palette::Terminal] {
            assert_eq!(Palette::parse(palette.key()), Some(palette));
        }
        assert_eq!(Palette::parse(" Terminal "), Some(Palette::Terminal));
        assert_eq!(Palette::parse("solarized"), None);
        for lines in [SessionLines::Two, SessionLines::One] {
            assert_eq!(SessionLines::from_count(lines.count()), Some(lines));
        }
        assert_eq!(SessionLines::from_count(3), None);
    }

    fn jira() -> TicketsFrom {
        TicketsFrom {
            tracker: "jira".to_owned(),
            fields: BTreeMap::from([
                ("site".to_owned(), "acme.atlassian.net".to_owned()),
                ("project".to_owned(), "APP".to_owned()),
            ]),
        }
    }

    #[test]
    fn where_tickets_are_goes_by_the_tracker_s_id_with_its_fields() {
        let jira = SettingChange::TicketsFrom(Some(jira()));
        let bytes = rmp_serde::to_vec_named(&jira).unwrap();
        assert_eq!(
            rmp_serde::from_slice::<SettingChange>(&bytes).unwrap(),
            jira
        );
        assert!(String::from_utf8_lossy(&bytes).contains("jira"));
        let SettingChange::TicketsFrom(Some(from)) = jira else {
            unreachable!()
        };
        assert_eq!(
            from.written(),
            [
                ("tracker", "jira"),
                ("project", "APP"),
                ("site", "acme.atlassian.net"),
            ]
        );
    }
}
