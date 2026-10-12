//! The settings a person changes on the UI's Settings screen, as the
//! protocol carries them (`ARCHITECTURE.md` §12). A change goes into the
//! person's own overrides of the project's `muster.toml`, never into the
//! repository (`ARC-05`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
}

/// A setting that is on or off, and where that comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toggle {
    pub on: bool,
    pub from: SettingSource,
}

/// Where a setting's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingSource {
    /// Nothing sets it: Muster's default.
    Default,
    /// The project's `muster.toml`, in its repository.
    Project,
    /// The person's own settings for the project, which win.
    Yours,
}

/// A change the person makes to a project's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingChange {
    /// List only the tickets assigned to the person, or every open one.
    OnlyMyTickets(bool),
}

impl SettingChange {
    /// The table and key it sets: `[tickets] mine`.
    pub fn key(self) -> (&'static str, &'static str) {
        match self {
            Self::OnlyMyTickets(_) => ONLY_MINE,
        }
    }

    /// The value it sets.
    pub fn value(self) -> bool {
        match self {
            Self::OnlyMyTickets(on) => on,
        }
    }
}

/// Where only-my-tickets is written: `[tickets] mine`.
pub const ONLY_MINE: (&str, &str) = ("tickets", "mine");
