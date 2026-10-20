//! Muster's own settings: the person's for all of Muster rather than any
//! project's, in `settings.toml` beside the projects' overrides in their
//! settings folder: whether the notch shows (`[notch] show`), the coding
//! agents they added (`[agents.<name>]`, [`super::agents`]), the UI's
//! layout and look (`[layout]`, `[look]`), what they are told of
//! (`[notifications]`), and the plugins they installed (`[plugins.<id>]`,
//! [`crate::plugins::registry`]). Read strictly, as a project's are;
//! parsing does no IO.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::ConfigError;
use super::agents::{self, AgentDefinition};
use crate::layout::Layout;
use crate::plugins::registry::{self, PluginEntry};
use crate::settings::{Naming, Palette, SessionLines};
use crate::telling::{Kind, Sound, Tell, Telling};

/// What the file is called, in the person's settings folder.
pub const FILE_NAME: &str = "settings.toml";

/// What a new file starts with, so a person who finds it knows what it is.
pub const NEW_FILE: &str = "# Your own settings for Muster as a whole.\n# Muster's Settings \
                            screen writes here, and so can you.\n";

/// Muster's own settings as written; what is not written is Muster's
/// default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MusterConfig {
    /// Show the agents in the MacBook's notch while the person looks
    /// elsewhere (`[notch] show`).
    pub notch: bool,
    /// The coding agents the person added (`[agents.<name>]`), by name.
    pub agents: Vec<AgentDefinition>,
    /// The fewest columns a window has for a session to sit beside the
    /// list; 0 at any width (`[layout] side_by_side_from`).
    pub side_by_side_from: u16,
    /// Whose colours the UI draws in (`[look] colours`).
    pub palette: Palette,
    /// How many lines a session's line takes (`[look] session_lines`).
    pub session_lines: SessionLines,
    /// Where the panels are (`[layout] panels`).
    pub panels: Layout,
    /// Whether the list is folded away (`[layout] list_folded`).
    pub list_folded: bool,
    /// How chats are named by an agent (`[names]`).
    pub naming: Naming,
    /// The keys the person set, by action, as written (`[keys]`).
    pub keys: BTreeMap<String, Vec<String>>,
    /// Whether the person is told of what newly needs them while they look
    /// away (`[notifications] show`).
    pub notifications: bool,
    /// Which kinds the person is told of, and how each sounds (the rest of
    /// `[notifications]`).
    pub telling: Telling,
    /// The plugins the person installed, linked or turned off
    /// (`[plugins.<id>]`), by id.
    pub plugins: Vec<PluginEntry>,
}

impl Default for MusterConfig {
    fn default() -> Self {
        Self {
            notch: true,
            agents: Vec::new(),
            side_by_side_from: crate::settings::SIDE_BY_SIDE_FROM,
            palette: Palette::default(),
            session_lines: SessionLines::default(),
            panels: Layout::default(),
            list_folded: false,
            naming: Naming::default(),
            keys: BTreeMap::new(),
            notifications: true,
            telling: Telling::default(),
            plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    notch: Notch,
    /// Read by [`agents::parse`], which names the key that is wrong.
    agents: Option<toml::Value>,
    #[serde(default)]
    layout: LayoutTable,
    #[serde(default)]
    look: Look,
    #[serde(default)]
    names: Names,
    /// Each action's key, or its keys: `new_session = "N"`,
    /// `leave_session = ["Q", "ctrl-]"]`.
    #[serde(default)]
    keys: BTreeMap<String, OneOrMore>,
    #[serde(default)]
    notifications: Notifications,
    /// Read by [`registry::parse`], which names the key that is wrong.
    plugins: Option<toml::Value>,
}

impl File {
    /// The agents and the plugins the person added, each read by its own
    /// module, which names the key that is wrong.
    fn tables(&self, file: &str) -> Result<(Vec<AgentDefinition>, Vec<PluginEntry>), ConfigError> {
        let agents = match &self.agents {
            Some(written) => agents::parse(written).map_err(|error| ConfigError::Agent {
                file: file.to_owned(),
                error,
            })?,
            None => Vec::new(),
        };
        let plugins = match &self.plugins {
            Some(written) => registry::parse(written).map_err(|error| ConfigError::Plugin {
                file: file.to_owned(),
                error,
            })?,
            None => Vec::new(),
        };
        Ok((agents, plugins))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Notifications {
    show: Option<bool>,
    questions: Option<bool>,
    finishes: Option<bool>,
    errors: Option<bool>,
    questions_sound: Option<String>,
    finishes_sound: Option<String>,
    errors_sound: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OneOrMore {
    One(String),
    More(Vec<String>),
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Names {
    by_agent: Option<bool>,
    agent: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Notch {
    show: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutTable {
    side_by_side_from: Option<u16>,
    panels: Option<String>,
    list_folded: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Look {
    colours: Option<String>,
    session_lines: Option<u16>,
}

impl MusterConfig {
    /// Reads the file's `text`, `None` when there is none; `file` names it
    /// in messages.
    pub fn parse(text: Option<&str>, file: &str) -> Result<Self, ConfigError> {
        let Some(text) = text else {
            return Ok(Self::default());
        };
        let table: toml::Table = toml::from_str(text).map_err(|error| ConfigError::Syntax {
            file: file.to_owned(),
            message: error.message().to_owned(),
        })?;
        let read = File::deserialize(table).map_err(|error| ConfigError::Invalid {
            file: file.to_owned(),
            message: error.message().to_owned(),
        })?;
        let (agents, plugins) = read.tables(file)?;
        let invalid = |message: &str| ConfigError::Invalid {
            file: file.to_owned(),
            message: message.to_owned(),
        };
        let palette = match read.look.colours.as_deref() {
            None => Palette::default(),
            Some(written) => Palette::parse(written)
                .ok_or_else(|| invalid("look.colours must be \"muster\" or \"terminal\""))?,
        };
        let session_lines = match read.look.session_lines {
            None => SessionLines::default(),
            Some(count) => SessionLines::from_count(count)
                .ok_or_else(|| invalid("look.session_lines must be 1 or 2"))?,
        };
        let panels = match read.layout.panels.as_deref() {
            None => Layout::default(),
            Some(written) => Layout::parse(written)
                .map_err(|error| invalid(&format!("layout.panels: {error}")))?,
        };
        let one_line = |key: &str, text: Option<String>| match text {
            Some(text) if text.trim().is_empty() || text.chars().any(char::is_control) => {
                Err(invalid(&format!("names.{key} must be one line of text")))
            }
            text => Ok(text.map(|text| text.trim().to_owned())),
        };
        let keys: BTreeMap<String, Vec<String>> = read
            .keys
            .into_iter()
            .map(|(action, keys)| {
                let keys = match keys {
                    OneOrMore::One(key) => vec![key],
                    OneOrMore::More(keys) => keys,
                };
                (action, keys)
            })
            .collect();
        crate::keys::Keymap::with(&keys).map_err(|error| invalid(&error.to_string()))?;
        let default = Self::default();
        let naming = Naming {
            by_agent: read.names.by_agent.unwrap_or(default.naming.by_agent),
            agent: one_line("agent", read.names.agent)?,
            model: one_line("model", read.names.model)?,
        };
        let written = &read.notifications;
        let mut telling = Telling::default();
        for (kind, on, sound) in [
            (Kind::Question, written.questions, &written.questions_sound),
            (Kind::Finish, written.finishes, &written.finishes_sound),
            (Kind::Error, written.errors, &written.errors_sound),
        ] {
            let sound = match sound {
                None => Sound::Default,
                Some(sound) => Sound::parse(sound).map_err(|error| {
                    invalid(&format!("notifications.{}_sound: {error}", kind.key()))
                })?,
            };
            let tell = Tell {
                on: on.unwrap_or(true),
                sound,
            };
            telling = telling.with(kind, tell);
        }
        Ok(Self {
            notch: read.notch.show.unwrap_or(default.notch),
            agents,
            side_by_side_from: read
                .layout
                .side_by_side_from
                .unwrap_or(default.side_by_side_from),
            palette,
            session_lines,
            panels,
            list_folded: read.layout.list_folded.unwrap_or(default.list_folded),
            naming,
            keys,
            notifications: read.notifications.show.unwrap_or(default.notifications),
            telling,
            plugins,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_written_shows_the_notch() {
        assert_eq!(
            MusterConfig::parse(None, "f").unwrap(),
            MusterConfig::default()
        );
        assert!(
            MusterConfig::parse(Some("# nothing yet\n"), "f")
                .unwrap()
                .notch
        );
        assert!(
            !MusterConfig::parse(Some("[notch]\nshow = false\n"), "f")
                .unwrap()
                .notch
        );
    }

    #[test]
    fn the_person_s_agents_read_with_the_rest() {
        let read = MusterConfig::parse(
            Some("[notch]\nshow = false\n\n[agents.my-agent]\nprogram = \"my-agent\"\n"),
            "settings.toml",
        )
        .unwrap();
        assert!(!read.notch);
        assert_eq!(read.agents.len(), 1);
        assert_eq!(read.agents[0].name, "my-agent");
        let error = MusterConfig::parse(
            Some("[agents.my-agent]\nprogram = \"x\"\ncolor = \"red\"\n"),
            "settings.toml",
        )
        .unwrap_err();
        assert!(matches!(error, ConfigError::Agent { .. }), "{error:?}");
        let error = error.to_string();
        assert!(
            error.starts_with("settings.toml: agents.my-agent.color"),
            "{error}"
        );
    }

    #[test]
    fn the_person_s_plugins_read_with_the_rest_and_a_wrong_key_is_named() {
        let read = MusterConfig::parse(
            Some(
                "[notch]\nshow = false\n\n[plugins.zoetrope]\nfolder = \"/p/zoetrope\"\n\
                 enabled = false\n",
            ),
            "settings.toml",
        )
        .unwrap();
        assert!(!read.notch);
        assert_eq!(read.plugins.len(), 1);
        assert_eq!(read.plugins[0].id, "zoetrope");
        assert!(!read.plugins[0].enabled);
        assert_eq!(MusterConfig::parse(None, "f").unwrap().plugins, []);
        let error = MusterConfig::parse(
            Some("[plugins.zoetrope]\nfolder = \"/p\"\nenable = true\n"),
            "settings.toml",
        )
        .unwrap_err();
        assert!(matches!(error, ConfigError::Plugin { .. }), "{error:?}");
        assert!(
            error
                .to_string()
                .starts_with("settings.toml: plugins.zoetrope.enable is not a setting"),
            "{error}"
        );
    }

    #[test]
    fn a_session_sits_beside_the_list_from_a_hundred_columns_unless_written() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.side_by_side_from, 100);
        let written =
            MusterConfig::parse(Some("[layout]\nside_by_side_from = 120\n"), "f").unwrap();
        assert_eq!(written.side_by_side_from, 120);
        assert!(written.notch, "the rest as by default");
        let any = MusterConfig::parse(Some("[layout]\nside_by_side_from = 0\n"), "f").unwrap();
        assert_eq!(any.side_by_side_from, 0);
        // Not a width: said, naming the file.
        let error = MusterConfig::parse(Some("[layout]\nside_by_side_from = -4\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("s.toml"), "{error}");
    }

    #[test]
    fn the_panels_are_as_written_or_muster_s_own() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.panels, Layout::default());
        assert!(!nothing.list_folded);
        let written = MusterConfig::parse(
            Some("[layout]\npanels = \"session | list 30 | (changes / shell)\"\nlist_folded = true\n"),
            "f",
        )
        .unwrap();
        assert_eq!(
            written.panels.to_string(),
            "session | list 30 | (changes / shell)"
        );
        assert!(written.list_folded);
        let error = MusterConfig::parse(Some("[layout]\npanels = \"list | session\"\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("s.toml"), "{error}");
        assert!(
            error.contains("layout.panels: changes is not in the layout"),
            "{error}"
        );
    }

    #[test]
    fn an_agent_names_chats_unless_the_person_says_not_and_by_the_agent_they_pick() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.naming, Naming::default());
        assert!(nothing.naming.by_agent);
        let written = MusterConfig::parse(
            Some("[names]\nby_agent = false\nagent = \"codex\"\nmodel = \" o4 \"\n"),
            "f",
        )
        .unwrap();
        assert_eq!(
            written.naming,
            Naming {
                by_agent: false,
                agent: Some("codex".to_owned()),
                model: Some("o4".to_owned()),
            }
        );
        let error = MusterConfig::parse(Some("[names]\nmodel = \"\"\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("names.model must be one line"), "{error}");
    }

    #[test]
    fn the_person_s_keys_read_one_or_more_and_a_clash_is_named() {
        let read = MusterConfig::parse(
            Some("[keys]\nnew_session = \"N\"\nleave_session = [\"Q\", \"alt-q\"]\n"),
            "f",
        )
        .unwrap();
        assert_eq!(read.keys["new_session"], ["N"]);
        assert_eq!(read.keys["leave_session"], ["Q", "alt-q"]);
        let error = MusterConfig::parse(Some("[keys]\nnew_session = \"d\"\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("s.toml"), "{error}");
        assert!(
            error.contains("keys.new_session: d is show_changes's"),
            "{error}"
        );
        let error = MusterConfig::parse(Some("[keys]\nfly = \"f\"\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("keys.fly is not something a key does"),
            "{error}"
        );
    }

    #[test]
    fn a_misspelt_key_is_an_error_naming_the_file() {
        let error = MusterConfig::parse(Some("[notch]\nshwo = false\n"), "settings.toml")
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("settings.toml") && error.contains("shwo"),
            "{error}"
        );
        let error = MusterConfig::parse(Some("[notch\n"), "settings.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("not valid TOML"), "{error}");
    }

    #[test]
    fn the_look_is_muster_s_own_on_two_lines_unless_written() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.palette, Palette::Muster);
        assert_eq!(nothing.session_lines, SessionLines::Two);
        let written = MusterConfig::parse(
            Some("[look]\ncolours = \"terminal\"\nsession_lines = 1\n"),
            "f",
        )
        .unwrap();
        assert_eq!(written.palette, Palette::Terminal);
        assert_eq!(written.session_lines, SessionLines::One);
        for (text, says) in [
            ("[look]\ncolours = \"neon\"\n", "look.colours must be"),
            (
                "[look]\nsession_lines = 3\n",
                "look.session_lines must be 1 or 2",
            ),
            ("[look]\ntheme = \"dark\"\n", "theme"),
        ] {
            let error = MusterConfig::parse(Some(text), "s.toml")
                .unwrap_err()
                .to_string();
            assert!(error.starts_with("s.toml"), "{error}");
            assert!(error.contains(says), "{error}");
        }
    }

    #[test]
    fn each_kind_is_told_with_the_mac_s_sound_unless_written() {
        let nothing = MusterConfig::parse(Some("[notifications]\nshow = true\n"), "f").unwrap();
        assert_eq!(nothing.telling, Telling::default());
        let written = MusterConfig::parse(
            Some(
                "[notifications]\nfinishes = false\nquestions_sound = \"Glass\"\n\
                 errors_sound = \"none\"\n",
            ),
            "f",
        )
        .unwrap();
        assert!(!written.telling.finishes.on);
        assert_eq!(
            written.telling.questions,
            Tell {
                on: true,
                sound: Sound::Named("Glass".to_owned())
            }
        );
        assert_eq!(written.telling.errors.sound, Sound::Silent);
        let error = MusterConfig::parse(
            Some("[notifications]\nerrors_sound = \"../x.wav\"\n"),
            "settings.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("notifications.errors_sound"), "{error}");
    }
}
