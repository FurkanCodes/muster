//! The settings a person changes on the UI's Settings screen, as the
//! protocol carries them (`ARCHITECTURE.md` §12). A change goes into the
//! person's own overrides of the project's `muster.toml`, never into the
//! repository (`ARC-05`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::TrackerKind;
use crate::layout::Layout;

/// A project's settings that the person can change, as they stand.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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
    /// repository's `muster.toml`; `None` where no tracker is written and
    /// the git remote tells. From protocol 17.
    #[serde(default)]
    pub tickets: Option<TicketsFrom>,
    /// Which file names the tracker (`[tickets] tracker`). From protocol 17.
    #[serde(default)]
    pub tickets_set_by: SettingSource,
    /// Nothing says where its tickets are, and its git remote cannot tell:
    /// `tickets_from` says why, and the person has to choose. From protocol
    /// 17.
    #[serde(default)]
    pub tickets_untold: bool,
    /// Whether the branches Muster makes start with `muster/`
    /// (`[branches] muster_prefix`). From protocol 20.
    #[serde(default)]
    pub muster_prefix: Option<Toggle>,
}

/// A setting that is on or off, and where that comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toggle {
    pub on: bool,
    pub from: SettingSource,
}

/// Where a setting's value comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Where a project's tickets are, as `[tickets]` writes it: the tracker,
/// and what it needs to find them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketsFrom {
    pub tracker: TrackerKind,
    /// Jira: the site, `acme.atlassian.net`, or a Data Center's address.
    #[serde(default)]
    pub site: Option<String>,
    /// Jira: the project whose open tickets are listed, by its key.
    #[serde(default)]
    pub project: Option<String>,
    /// Jira: a query of the person's own, instead of the project.
    #[serde(default)]
    pub jql: Option<String>,
    /// Linear: the team whose open issues are listed, by its key.
    #[serde(default)]
    pub team: Option<String>,
}

impl TicketsFrom {
    /// Each key of `[tickets]` that says where the tickets are, with what
    /// this sets it to; `None` leaves it unwritten.
    pub fn written(&self) -> [(&'static str, Option<&str>); 5] {
        [
            ("tracker", Some(self.tracker.key())),
            ("site", self.site.as_deref()),
            ("project", self.project.as_deref()),
            ("jql", self.jql.as_deref()),
            ("team", self.team.as_deref()),
        ]
    }
}

/// The keys of `[tickets]` that say where the tickets are.
pub const TICKETS_FROM_KEYS: [&str; 5] = ["tracker", "site", "project", "jql", "team"];

/// The first protocol a daemon takes [`SettingChange::TicketsFrom`] in; an
/// older one is never sent it (`ARC-09`).
pub const TICKETS_FROM_PROTOCOL: u32 = 17;

/// A change the person makes to a project's settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingChange {
    /// List only the tickets assigned to the person, or every open one.
    OnlyMyTickets(bool),
    /// Where the tickets are; `None` follows the repository's `muster.toml`
    /// and the git remote again. From protocol 17.
    TicketsFrom(Option<TicketsFrom>),
    /// Whether the branches Muster makes start with `muster/`. From
    /// protocol 20.
    MusterPrefix(bool),
}

/// The first protocol whose daemon takes `SettingChange::MusterPrefix`.
pub const MUSTER_PREFIX_PROTOCOL: u32 = 20;

/// Where the branches' prefix is written: `[branches] muster_prefix`.
pub const MUSTER_PREFIX: (&str, &str) = ("branches", "muster_prefix");

/// Muster's own settings, the person's for all of Muster rather than a
/// project's, as they stand. From protocol 23.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    /// Show the agents in the MacBook's notch while no Muster window has
    /// the person's eyes (`[notch] show`).
    pub notch: bool,
    /// Where they are written: the person's own file, never a repository.
    pub file: PathBuf,
    /// The coding agents the person added there (`[agents.<name>]`), by
    /// name, offered beside Muster's own. From protocol 29.
    #[serde(default)]
    pub agents: Vec<AddedAgent>,
    /// Why the file does not read, naming the key, while it does not: the
    /// rest are then Muster's defaults. From protocol 29.
    #[serde(default)]
    pub problem: Option<String>,
    /// The fewest columns a window has for a session's terminal to sit
    /// beside the list; in a narrower one, the list or the session fills
    /// it. 0 keeps them side by side at any width (`[layout]
    /// side_by_side_from`). From protocol 32.
    #[serde(default = "default_side_by_side_from")]
    pub side_by_side_from: u16,
    /// Whose colours the UI draws in (`[look] colours`). From protocol 35.
    #[serde(default)]
    pub palette: Palette,
    /// How many lines a session's line in the lists takes (`[look]
    /// session_lines`). From protocol 35.
    #[serde(default)]
    pub session_lines: SessionLines,
    /// Where the UI's panels are (`[layout] panels`). From protocol 36.
    #[serde(default)]
    pub panels: Layout,
    /// Whether the list is folded away, the session taking its place
    /// (`[layout] list_folded`). From protocol 36.
    #[serde(default)]
    pub list_folded: bool,
}

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
        }
    }
}

/// Whose colours the UI draws in. From protocol 35.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// How many lines a session's line in the lists takes. From protocol 35.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// The first protocol whose daemon keeps [`Preferences::palette`] and
/// [`Preferences::session_lines`] and takes [`PreferenceChange::Palette`]
/// and [`PreferenceChange::SessionLines`] (`ARC-09`).
pub const LOOK_PROTOCOL: u32 = 35;

/// Where the palette is written: `[look] colours`.
pub const LOOK_COLOURS: (&str, &str) = ("look", "colours");

/// Where the lines a session takes are written: `[look] session_lines`.
pub const LOOK_SESSION_LINES: (&str, &str) = ("look", "session_lines");

/// A coding agent the person added in Muster's own settings, as clients
/// offer it beside Muster's own (`muster_core::config::agents`). Only what
/// a client shows: never its program's environment. From protocol 29.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
}

/// The first protocol whose daemon knows the agents the person added and
/// reads Muster's own settings again when asked
/// ([`crate::protocol::Command::ReadPreferences`]).
pub const AGENTS_PROTOCOL: u32 = 29;
/// From how many columns a session sits beside the list unless the person
/// says otherwise: the 34 the list takes at least, its border and a
/// session 64 columns wide, which still holds an agent's prompt and a
/// diff's lines without wrapping them; narrower, the session would wrap
/// what it draws.
pub const SIDE_BY_SIDE_FROM: u16 = 100;

fn default_side_by_side_from() -> u16 {
    SIDE_BY_SIDE_FROM
}

/// The widths the Settings screen offers for
/// [`Preferences::side_by_side_from`], narrowest first: 0 for any width.
pub const SIDE_BY_SIDE_CHOICES: [u16; 8] = [0, 80, 90, 100, 110, 120, 140, 160];

/// A change the person makes to Muster's own settings. From protocol 23.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreferenceChange {
    /// Show the agents in the notch, or leave it alone.
    Notch(bool),
    /// From how many columns a session sits beside the list; 0 at any
    /// width. From protocol 32.
    SideBySideFrom(u16),
    /// Whose colours the UI draws in. From protocol 35.
    Palette(Palette),
    /// How many lines a session's line in the lists takes. From protocol
    /// 35.
    SessionLines(SessionLines),
    /// Where the panels are. From protocol 36.
    Panels(Layout),
    /// Fold the list away, or bring it back. From protocol 36.
    ListFolded(bool),
}

/// The first protocol whose daemon keeps [`Preferences::panels`] and
/// [`Preferences::list_folded`] and takes [`PreferenceChange::Panels`] and
/// [`PreferenceChange::ListFolded`] (`ARC-09`).
pub const LAYOUT_PROTOCOL: u32 = 36;

/// Where the panels are written: `[layout] panels`.
pub const PANELS: (&str, &str) = ("layout", "panels");

/// Where the list's folding is written: `[layout] list_folded`.
pub const LIST_FOLDED: (&str, &str) = ("layout", "list_folded");

/// The first protocol whose daemon keeps
/// [`Preferences::side_by_side_from`] and takes
/// [`PreferenceChange::SideBySideFrom`] (`ARC-09`).
pub const SIDE_BY_SIDE_PROTOCOL: u32 = 32;

/// Where it is written: `[layout] side_by_side_from`.
pub const SIDE_BY_SIDE: (&str, &str) = ("layout", "side_by_side_from");

/// The first protocol whose daemon knows who looks at its clients, shows
/// a session in one, and keeps Muster's own settings (`ARC-09`).
pub const NOTCH_PROTOCOL: u32 = 23;

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

    #[test]
    fn where_tickets_are_goes_by_the_tracker_s_key() {
        let jira = SettingChange::TicketsFrom(Some(TicketsFrom {
            tracker: TrackerKind::Jira,
            site: Some("acme.atlassian.net".to_owned()),
            project: Some("APP".to_owned()),
            jql: None,
            team: None,
        }));
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
                ("tracker", Some("jira")),
                ("site", Some("acme.atlassian.net")),
                ("project", Some("APP")),
                ("jql", None),
                ("team", None),
            ]
        );
    }
}
