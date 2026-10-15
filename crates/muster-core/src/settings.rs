//! The settings a person changes on the UI's Settings screen, as the
//! protocol carries them (`ARCHITECTURE.md` §12). A change goes into the
//! person's own overrides of the project's `muster.toml`, never into the
//! repository (`ARC-05`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::TrackerKind;

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
