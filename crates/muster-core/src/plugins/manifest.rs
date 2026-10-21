//! A plugin's manifest, `muster-plugin.toml` at its folder's root: what the
//! plugin is, what it needs, and what it runs (`ARCHITECTURE.md` §7a).
//!
//! Read strictly, as Muster's settings are: an unknown key or a value of
//! the wrong kind is an error naming the key (`panels[0].command`), never a
//! default, since what a manifest says is what the person approves
//! (`PRD-15`). Text in, a [`Manifest`] out: the daemon reads the file.

use serde::{Deserialize, Serialize};

use super::hooks::Hook;
use super::placeholders::Placeholder;

mod hooks;

/// What the file is called, at a plugin's folder's root.
pub const FILE_NAME: &str = "muster-plugin.toml";

/// The most a manifest may hold: more is no manifest.
pub const MOST_BYTES: u64 = 64 * 1024;

/// The longest a name, a title or a label is.
const NAME_MAX: usize = 40;

/// The longest a description or a word on getting what it needs is.
const TEXT_MAX: usize = 300;

/// A plugin's manifest, as read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// What Muster knows it by: its folder, its settings, its harness.
    pub id: String,
    /// Its name on screen.
    pub name: String,
    /// Its own version, as it says it.
    pub version: String,
    pub description: Option<String>,
    /// The oldest Muster it works with.
    pub muster: String,
    /// The programs it needs on `PATH`, which Muster never installs.
    pub requires: Vec<String>,
    /// How to get them, in the plugin's words.
    pub how_to_get: Option<String>,
    /// The names of the person's variables it may see.
    pub env: Vec<String>,
    /// The names of the secrets it is given.
    pub secrets: Vec<String>,
    pub panels: Vec<Panel>,
    pub tickets: Option<TicketSource>,
    /// Programs run as something happens.
    pub hooks: Vec<Hook>,
}

/// A terminal program shown beside a session or a workspace.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Panel {
    /// Its id within the plugin.
    pub id: String,
    /// Its panel's title.
    pub title: String,
    pub on: PanelOn,
    /// The program and its arguments, placeholders in them.
    pub command: Vec<String>,
    /// The key it suggests, as `[keys]` writes one; the person sets it.
    #[serde(default)]
    pub key: Option<String>,
}

/// What a panel is beside.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PanelOn {
    Session,
    Workspace,
}

impl PanelOn {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Session => "session",
            Self::Workspace => "workspace",
        }
    }
}

/// A ticket source: one process serving every project's tickets.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TicketSource {
    /// As the Tickets screen names it: `YouTrack`.
    pub name: String,
    /// As a prompt names one of its tickets: `issue`.
    pub noun: String,
    /// The program and its arguments.
    pub command: Vec<String>,
    /// What a project's `[tickets]` holds for it.
    #[serde(default)]
    pub settings: Vec<TicketSetting>,
}

/// A field of a project's `[tickets]`, for a ticket source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct TicketSetting {
    pub key: String,
    pub label: String,
    pub required: bool,
}

/// Why a manifest does not read. Each names its key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("is not valid TOML: {0}")]
    Syntax(String),
    #[error("{key} is not a key of a plugin's manifest here: they are {known}")]
    UnknownKey { key: String, known: String },
    #[error("{key} is missing: {expected}")]
    Missing { key: String, expected: &'static str },
    #[error("{key} must be {expected}")]
    Invalid { key: String, expected: &'static str },
    #[error("id: {id:?} is kept for Muster's own plugins; call it something else")]
    Reserved { id: String },
    #[error("muster: it needs Muster {needs} or later, and this is {running}")]
    TooNew { needs: String, running: String },
    #[error("{key}: {{{word}}} is not a placeholder: they are {known}")]
    UnknownPlaceholder {
        key: String,
        word: String,
        known: String,
    },
    #[error("{key}: {{{word}}} is a session's, and this panel is a workspace's")]
    SessionOnly { key: String, word: String },
    #[error("{key}: {{{word}}} is not told to a hook, which is given names and ids only")]
    HookPlaceholder { key: String, word: String },
    #[error("{key}: {{{word}}} is a session's, and {event} is no session's")]
    NoSession {
        key: String,
        word: String,
        event: &'static str,
    },
    #[error("{key}: {word:?} is not an event: they are {known}")]
    UnknownEvent {
        key: String,
        word: String,
        known: String,
    },
    #[error("{key}: none of the hook's events has one to test; only {carried_by} do")]
    NotCarried {
        key: String,
        carried_by: &'static str,
    },
    #[error(
        "tickets.command: a ticket source takes no placeholder; one process serves every \
         project, told the project in each request"
    )]
    TicketPlaceholder,
    #[error("{key}: {id:?} is another's too")]
    Twice { key: String, id: String },
    #[error("a plugin gives at least one [[panels]], [[hooks]] or a [tickets]")]
    Nothing,
}

const KEYS: [&str; 12] = [
    "id",
    "name",
    "version",
    "description",
    "muster",
    "requires",
    "how_to_get",
    "env",
    "secrets",
    "panels",
    "tickets",
    "hooks",
];
const PANEL_KEYS: [&str; 5] = ["id", "title", "on", "command", "key"];
const TICKET_KEYS: [&str; 4] = ["name", "noun", "command", "settings"];
const SETTING_KEYS: [&str; 3] = ["key", "label", "required"];

/// Keys a project's `[tickets]` keeps for Muster itself.
const MUSTER_S_TICKET_KEYS: [&str; 2] = ["tracker", "mine"];

impl Manifest {
    /// A plugin's manifest, as this Muster reads it.
    pub fn parse(text: &str) -> Result<Self, ManifestError> {
        read(text, super::MUSTER_VERSION, false)
    }

    /// One of Muster's own, built into it, whose ids are kept for them.
    pub fn parse_own(text: &str) -> Result<Self, ManifestError> {
        read(text, super::MUSTER_VERSION, true)
    }

    /// Every command it would run, each with where it is written:
    /// `panels[0].command`, `tickets.command`, `hooks[0].command`.
    pub fn commands(&self) -> Vec<(String, &[String])> {
        let panels = self
            .panels
            .iter()
            .enumerate()
            .map(|(at, panel)| (format!("panels[{at}].command"), panel.command.as_slice()));
        let tickets = self
            .tickets
            .iter()
            .map(|tickets| ("tickets.command".to_owned(), tickets.command.as_slice()));
        let hooks = self
            .hooks
            .iter()
            .enumerate()
            .map(|(at, hook)| (format!("hooks[{at}].command"), hook.command.as_slice()));
        panels.chain(tickets).chain(hooks).collect()
    }
}

/// Reads `text` for a Muster of version `running`; `own` for one of
/// Muster's own plugins.
pub(crate) fn read(text: &str, running: &str, own: bool) -> Result<Manifest, ManifestError> {
    let table: toml::Table =
        toml::from_str(text).map_err(|error| ManifestError::Syntax(error.message().to_owned()))?;
    let top = Read {
        at: String::new(),
        table: &table,
    };
    top.known(&KEYS)?;
    let id = top.required("id", "the plugin's id, as `id = \"zoetrope\"`")?;
    if !super::is_id(&id) {
        return Err(top.invalid(
            "id",
            "1 to 32 of a-z, 0-9 and -, starting with a letter or digit",
        ));
    }
    if !own && super::is_reserved(&id) {
        return Err(ManifestError::Reserved { id });
    }
    let name = top.required("name", "its name on screen")?;
    top.short("name", &name, NAME_MAX, "one line of at most 40 characters")?;
    let version = top.required("version", "its own version, as `version = \"0.1.0\"`")?;
    if version.len() > 32
        || !version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
    {
        return Err(top.invalid(
            "version",
            "a version of at most 32 letters, digits, `.`, `-` and `+`",
        ));
    }
    let description = top.text_within(
        "description",
        TEXT_MAX,
        "one line of at most 300 characters",
    )?;
    let muster = top.required(
        "muster",
        "the oldest Muster it works with, as `muster = \"0.4.0\"`",
    )?;
    let needs = super::version(&muster)
        .ok_or_else(|| top.invalid("muster", "a version of Muster, as `0.4.0`"))?;
    if super::version(running).is_some_and(|running| needs > running) {
        return Err(ManifestError::TooNew {
            needs: muster,
            running: running.to_owned(),
        });
    }
    let requires = top.list(
        "requires",
        "a list of programs' names on PATH, as [\"zoe\"]",
    )?;
    if requires
        .iter()
        .any(|program| program.contains(['/', ' ']) || program.starts_with('-'))
    {
        return Err(top.invalid(
            "requires",
            "a list of programs' names on PATH, as [\"zoe\"], not paths",
        ));
    }
    let how_to_get =
        top.text_within("how_to_get", TEXT_MAX, "one line of at most 300 characters")?;
    let names = "a list of variables' names: letters, digits and _, not starting with a digit, \
                 nor with MUSTER_, which are Muster's own";
    let env = top.list("env", names)?;
    let secrets = top.list("secrets", names)?;
    if !env.iter().chain(&secrets).all(|name| is_variable(name)) {
        let key = if env.iter().all(|name| is_variable(name)) {
            "secrets"
        } else {
            "env"
        };
        return Err(top.invalid(key, names));
    }
    let panels = top.panels()?;
    let tickets = top.tickets()?;
    let hooks = top.hooks()?;
    if panels.is_empty() && tickets.is_none() && hooks.is_empty() {
        return Err(ManifestError::Nothing);
    }
    Ok(Manifest {
        id,
        name,
        version,
        description,
        muster,
        requires,
        how_to_get,
        env,
        secrets,
        panels,
        tickets,
        hooks,
    })
}

/// Whether `name` can be a variable's: letters, digits and `_`, not
/// starting with a digit, and not one of Muster's own (`MUSTER_…`).
fn is_variable(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.starts_with("MUSTER_")
        && name.len() <= 64
}

/// Reads the keys of one table of a manifest, `at` the path to it
/// (`panels[0].`).
struct Read<'a> {
    at: String,
    table: &'a toml::Table,
}

impl<'a> Read<'a> {
    fn key(&self, key: &str) -> String {
        format!("{}{key}", self.at)
    }

    fn invalid(&self, key: &str, expected: &'static str) -> ManifestError {
        ManifestError::Invalid {
            key: self.key(key),
            expected,
        }
    }

    fn known(&self, keys: &[&str]) -> Result<(), ManifestError> {
        match self.table.keys().find(|key| !keys.contains(&key.as_str())) {
            Some(key) => Err(ManifestError::UnknownKey {
                key: self.key(key),
                known: keys.join(", "),
            }),
            None => Ok(()),
        }
    }

    /// The text at `key`, if written: one line, not empty.
    fn text(&self, key: &str) -> Result<Option<String>, ManifestError> {
        match self.table.get(key) {
            None => Ok(None),
            Some(toml::Value::String(text))
                if !text.trim().is_empty() && !text.chars().any(char::is_control) =>
            {
                Ok(Some(text.trim().to_owned()))
            }
            Some(_) => Err(self.invalid(key, "one line of text, not empty")),
        }
    }

    /// [`Self::text`], refused if longer than `most` characters.
    fn text_within(
        &self,
        key: &str,
        most: usize,
        expected: &'static str,
    ) -> Result<Option<String>, ManifestError> {
        let text = self.text(key)?;
        if let Some(text) = &text {
            self.short(key, text, most, expected)?;
        }
        Ok(text)
    }

    fn required(&self, key: &str, expected: &'static str) -> Result<String, ManifestError> {
        self.text(key)?.ok_or_else(|| ManifestError::Missing {
            key: self.key(key),
            expected,
        })
    }

    /// Refuses `text` at `key` if longer than `most` characters.
    fn short(
        &self,
        key: &str,
        text: &str,
        most: usize,
        expected: &'static str,
    ) -> Result<(), ManifestError> {
        if text.chars().count() > most {
            return Err(self.invalid(key, expected));
        }
        Ok(())
    }

    /// The list of one-line texts at `key`; empty if not written.
    fn list(&self, key: &str, expected: &'static str) -> Result<Vec<String>, ManifestError> {
        let Some(value) = self.table.get(key) else {
            return Ok(Vec::new());
        };
        let toml::Value::Array(items) = value else {
            return Err(self.invalid(key, expected));
        };
        items
            .iter()
            .map(|item| match item {
                toml::Value::String(text)
                    if !text.trim().is_empty() && !text.chars().any(char::is_control) =>
                {
                    Ok(text.trim().to_owned())
                }
                _ => Err(self.invalid(key, expected)),
            })
            .collect()
    }

    /// The command at `key`: a program and its arguments, never a shell's
    /// line. The program is a name on `PATH`, a full path, or `./…` inside
    /// the plugin's folder.
    fn command(&self, key: &str) -> Result<Vec<String>, ManifestError> {
        let expected = "a program and its arguments, as [\"zoe\", \"{transcript}\"]: the \
                        program a name on PATH, a full path, or ./… inside the plugin's \
                        folder, each argument text without a NUL";
        let Some(value) = self.table.get(key) else {
            return Err(ManifestError::Missing {
                key: self.key(key),
                expected,
            });
        };
        let toml::Value::Array(items) = value else {
            return Err(self.invalid(key, expected));
        };
        let command: Vec<String> = items
            .iter()
            .map(|item| match item {
                toml::Value::String(text) if !text.contains('\0') => Ok(text.clone()),
                _ => Err(self.invalid(key, expected)),
            })
            .collect::<Result<_, _>>()?;
        let Some(program) = command.first() else {
            return Err(self.invalid(key, expected));
        };
        let inside = program
            .strip_prefix("./")
            .is_some_and(|path| !path.is_empty() && !path.split('/').any(|part| part == ".."));
        let named = !program.is_empty()
            && !program.contains('/')
            && !program.starts_with('-')
            && !program.chars().any(char::is_control);
        let full = program.starts_with('/') && !program.split('/').any(|part| part == "..");
        if !(inside || named || full) || Placeholder::words_in(program).next().is_some() {
            return Err(self.invalid(key, expected));
        }
        Ok(command)
    }

    /// The table `value`, read below `at`.
    fn table_at(
        value: &'a toml::Value,
        at: String,
        expected: &'static str,
    ) -> Result<Read<'a>, ManifestError> {
        match value {
            toml::Value::Table(table) => Ok(Read { at, table }),
            _ => Err(ManifestError::Invalid {
                key: at.trim_end_matches('.').to_owned(),
                expected,
            }),
        }
    }

    /// The tables of the array at `key`: `[[key]]`.
    fn tables(&self, key: &str, expected: &'static str) -> Result<Vec<Read<'a>>, ManifestError> {
        let Some(value) = self.table.get(key) else {
            return Ok(Vec::new());
        };
        let toml::Value::Array(items) = value else {
            return Err(self.invalid(key, expected));
        };
        items
            .iter()
            .enumerate()
            .map(|(at, item)| Self::table_at(item, format!("{}[{at}].", self.key(key)), expected))
            .collect()
    }

    fn panels(&self) -> Result<Vec<Panel>, ManifestError> {
        let mut panels: Vec<Panel> = Vec::new();
        for read in self.tables("panels", "a [[panels]] table for each panel")? {
            read.known(&PANEL_KEYS)?;
            let id = read.required("id", "the panel's id, as `id = \"graph\"`")?;
            if !super::is_id(&id) {
                return Err(read.invalid(
                    "id",
                    "1 to 32 of a-z, 0-9 and -, starting with a letter or digit",
                ));
            }
            if panels.iter().any(|panel| panel.id == id) {
                return Err(ManifestError::Twice {
                    key: read.key("id"),
                    id,
                });
            }
            let title = match read.text("title")? {
                Some(title) => title,
                None => id.clone(),
            };
            read.short(
                "title",
                &title,
                NAME_MAX,
                "one line of at most 40 characters",
            )?;
            let on = match read.text("on")?.as_deref() {
                None | Some("session") => PanelOn::Session,
                Some("workspace") => PanelOn::Workspace,
                Some(_) => return Err(read.invalid("on", "\"session\" or \"workspace\"")),
            };
            let command = read.command("command")?;
            read.placeholders(&command, on)?;
            let key = read.text("key")?;
            if let Some(key) = &key
                && crate::keys::Chord::parse(key).is_err()
            {
                return Err(read.invalid(
                    "key",
                    "a key as [keys] writes one: a character (Z), or a named one (f5), after \
                     ctrl- or alt- if it has them",
                ));
            }
            panels.push(Panel {
                id,
                title,
                on,
                command,
                key,
            });
        }
        Ok(panels)
    }

    /// Refuses a word in braces in `command` that is no placeholder, or a
    /// session's on a workspace's panel.
    fn placeholders(&self, command: &[String], on: PanelOn) -> Result<(), ManifestError> {
        let known = || {
            Placeholder::ALL
                .map(|placeholder| placeholder.to_string())
                .join(" ")
        };
        for word in command
            .iter()
            .flat_map(|argument| Placeholder::words_in(argument))
        {
            match Placeholder::named(word) {
                None => {
                    return Err(ManifestError::UnknownPlaceholder {
                        key: self.key("command"),
                        word: word.to_owned(),
                        known: known(),
                    });
                }
                Some(placeholder) if on == PanelOn::Workspace && placeholder.is_a_session_s() => {
                    return Err(ManifestError::SessionOnly {
                        key: self.key("command"),
                        word: word.to_owned(),
                    });
                }
                Some(_) => {}
            }
        }
        Ok(())
    }

    fn tickets(&self) -> Result<Option<TicketSource>, ManifestError> {
        let Some(value) = self.table.get("tickets") else {
            return Ok(None);
        };
        let read = Self::table_at(value, "tickets.".to_owned(), "a [tickets] table")?;
        read.known(&TICKET_KEYS)?;
        let name = read.required(
            "name",
            "as the Tickets screen names it, as `name = \"YouTrack\"`",
        )?;
        read.short("name", &name, NAME_MAX, "one line of at most 40 characters")?;
        let noun = read.required(
            "noun",
            "as a prompt names one of its tickets, as `noun = \"issue\"`",
        )?;
        if noun.chars().count() > 20 || !noun.chars().all(|c| c.is_alphabetic() || c == ' ') {
            return Err(read.invalid("noun", "a word or two of at most 20 letters"));
        }
        let command = read.command("command")?;
        if command
            .iter()
            .any(|argument| Placeholder::words_in(argument).next().is_some())
        {
            return Err(ManifestError::TicketPlaceholder);
        }
        let mut settings: Vec<TicketSetting> = Vec::new();
        for field in read.tables("settings", "a [[tickets.settings]] table for each field")? {
            field.known(&SETTING_KEYS)?;
            let key = field.required("key", "the field's key in a project's [tickets]")?;
            let keyed = key.len() <= 32
                && key.starts_with(|c: char| c.is_ascii_lowercase())
                && key
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_');
            if !keyed || MUSTER_S_TICKET_KEYS.contains(&key.as_str()) {
                return Err(field.invalid(
                    "key",
                    "lowercase letters, digits and _, starting with a letter, at most 32, and \
                     neither tracker nor mine, which are Muster's own",
                ));
            }
            if settings.iter().any(|setting| setting.key == key) {
                return Err(ManifestError::Twice {
                    key: field.key("key"),
                    id: key,
                });
            }
            let label = field.text("label")?.unwrap_or_else(|| key.clone());
            field.short(
                "label",
                &label,
                NAME_MAX,
                "one line of at most 40 characters",
            )?;
            let required = match field.table.get("required") {
                None => false,
                Some(toml::Value::Boolean(required)) => *required,
                Some(_) => return Err(field.invalid("required", "true or false")),
            };
            settings.push(TicketSetting {
                key,
                label,
                required,
            });
        }
        Ok(Some(TicketSource {
            name,
            noun,
            command,
            settings,
        }))
    }
}

#[cfg(test)]
mod tests;
