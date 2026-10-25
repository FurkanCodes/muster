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
use crate::delegation::Delegation;
use crate::layout::Layout;
use crate::plugins::registry::{self, PluginEntry};
use crate::settings::{Naming, Palette, Preferences, SessionLines};
use crate::telling::{Kind, Sound, Tell, Telling};

/// What the file is called, in the person's settings folder.
pub const FILE_NAME: &str = "settings.toml";

/// What a new file starts with, so a person who finds it knows what it is.
pub const NEW_FILE: &str = "# Your own settings for Muster as a whole.\n# Muster's Settings \
                            screen writes here, and so can you.\n";

/// Muster's own settings as written; what is not written is Muster's
/// default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MusterConfig {
    /// The settings as clients are told them ([`Preferences`]), but where
    /// they are written, which the reader of the file fills in.
    pub preferences: Preferences,
    /// The coding agents the person added (`[agents.<name>]`), by name,
    /// whole: [`Preferences::agents`] has what clients are shown of each.
    pub agents: Vec<AgentDefinition>,
    /// The plugins the person installed, linked or turned off
    /// (`[plugins.<id>]`), by id.
    pub plugins: Vec<PluginEntry>,
    /// What reads but does nothing, for the person: a key set for an
    /// action Muster does not know, as a removed plugin's (`[keys]`).
    pub warnings: Vec<String>,
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
    #[serde(default)]
    worktrees: Worktrees,
    #[serde(default)]
    delegation: DelegationTable,
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
    /// The keys of names written with dots and no quotes, which TOML reads
    /// as tables: `plugin.zoetrope.graph = "G"`.
    Dotted(BTreeMap<String, OneOrMore>),
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
struct Worktrees {
    shared_builds: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct DelegationTable {
    allowed: Option<bool>,
    at_once: Option<u8>,
    depth: Option<u8>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct LayoutTable {
    side_by_side_from: Option<u16>,
    panels: Option<String>,
    list_folded: Option<bool>,
    projects: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Look {
    colours: Option<String>,
    session_lines: Option<u16>,
}

/// The projects' order as written (`[layout] projects`): each a project's
/// id, refused where one does not read as one.
fn project_order(
    written: Option<Vec<String>>,
    file: &str,
) -> Result<Vec<crate::ProjectId>, ConfigError> {
    written
        .unwrap_or_default()
        .iter()
        .map(|id| {
            id.parse().map_err(|error| ConfigError::Invalid {
                file: file.to_owned(),
                message: format!("layout.projects: {error}"),
            })
        })
        .collect()
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
        let projects = project_order(read.layout.projects, file)?;
        let one_line = |key: &str, text: Option<String>| match text {
            Some(text) if !crate::text::is_one_line(&text) => {
                Err(invalid(&format!("names.{key} must be one line of text")))
            }
            text => Ok(text.map(|text| text.trim().to_owned())),
        };
        let (keys, warnings) = keys(read.keys, file)?;
        let default = Preferences::default();
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
        let delegation = &read.delegation;
        let preferences = Preferences {
            notch: read.notch.show.unwrap_or(default.notch),
            agents: agents.iter().map(AgentDefinition::added).collect(),
            side_by_side_from: read
                .layout
                .side_by_side_from
                .unwrap_or(default.side_by_side_from),
            palette,
            session_lines,
            panels,
            list_folded: read.layout.list_folded.unwrap_or(default.list_folded),
            projects,
            naming,
            keys,
            notifications: read.notifications.show.unwrap_or(default.notifications),
            telling,
            shared_builds: read
                .worktrees
                .shared_builds
                .unwrap_or(default.shared_builds),
            delegation: Delegation {
                allowed: delegation.allowed.unwrap_or(default.delegation.allowed),
                at_once: delegation.at_once.unwrap_or(default.delegation.at_once),
                depth: delegation.depth.unwrap_or(default.delegation.depth),
            },
            ..default
        };
        Ok(Self {
            preferences,
            agents,
            plugins,
            warnings,
        })
    }
}

/// Each action's keys as written, by its name (`[keys]`).
type Keys = BTreeMap<String, Vec<String>>;

/// `written`'s keys into `keys`, each under its whole name, a name written
/// with dots as it reads with quotes (`"plugin.zoetrope.graph"`).
fn flatten(written: BTreeMap<String, OneOrMore>, above: &str, keys: &mut Keys) {
    for (name, written) in written {
        let name = format!("{above}{name}");
        match written {
            OneOrMore::One(key) => {
                keys.insert(name, vec![key]);
            }
            OneOrMore::More(more) => {
                keys.insert(name, more);
            }
            OneOrMore::Dotted(below) => flatten(below, &format!("{name}."), keys),
        }
    }
}

/// The person's keys as `written`, each action's in a list, and what of
/// them does nothing; refused, naming the key, where Muster's own keys
/// would not read with them (`Keymap::with`).
fn keys(
    written: BTreeMap<String, OneOrMore>,
    file: &str,
) -> Result<(Keys, Vec<String>), ConfigError> {
    let mut keys = Keys::new();
    flatten(written, "", &mut keys);
    let keymap = crate::keys::Keymap::with(&keys).map_err(|error| ConfigError::Invalid {
        file: file.to_owned(),
        message: error.to_string(),
    })?;
    let warnings = keymap
        .warnings()
        .iter()
        .map(|warning| format!("{file}: {warning}"))
        .collect();
    Ok((keys, warnings))
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
                .preferences
                .notch
        );
        assert!(
            !MusterConfig::parse(Some("[notch]\nshow = false\n"), "f")
                .unwrap()
                .preferences
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
        assert!(!read.preferences.notch);
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
        assert!(!read.preferences.notch);
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
    fn a_panel_s_key_reads_with_its_name_quoted_or_not() {
        for written in [
            "[keys]\n\"plugin.zoetrope.graph\" = \"G\"\n",
            "[keys]\nplugin.zoetrope.graph = \"G\"\n",
            "[keys.plugin.zoetrope]\ngraph = [\"G\"]\n",
        ] {
            let read = MusterConfig::parse(Some(written), "settings.toml").unwrap();
            assert_eq!(
                read.preferences.keys["plugin.zoetrope.graph"],
                ["G"],
                "{written}"
            );
            assert_eq!(read.preferences.keys.len(), 1);
        }
    }

    #[test]
    fn a_key_for_an_action_no_longer_there_leaves_the_file_readable() {
        // A key the person set for a plugin's panel, then removed the
        // plugin: the file reads, the rest of it as written, and says why
        // the key does nothing.
        let read = MusterConfig::parse(
            Some("[notch]\nshow = false\n\n[keys]\nrename = \"F2\"\nzoetrope_graph = \"Z\"\n"),
            "settings.toml",
        )
        .unwrap();
        assert!(!read.preferences.notch);
        assert_eq!(
            read.preferences.keys["zoetrope_graph"],
            ["Z"],
            "kept as written"
        );
        assert_eq!(read.preferences.keys["rename"], ["F2"]);
        assert_eq!(read.warnings.len(), 1);
        assert!(
            read.warnings[0].starts_with("settings.toml: keys.zoetrope_graph is not something"),
            "{:?}",
            read.warnings
        );
        // A key of Muster's own that is wrong still is.
        assert!(MusterConfig::parse(Some("[keys]\nrename = \"not a key\"\n"), "f").is_err());
    }

    #[test]
    fn a_session_sits_beside_the_list_from_a_hundred_columns_unless_written() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.preferences.side_by_side_from, 100);
        let written =
            MusterConfig::parse(Some("[layout]\nside_by_side_from = 120\n"), "f").unwrap();
        assert_eq!(written.preferences.side_by_side_from, 120);
        assert!(written.preferences.notch, "the rest as by default");
        let any = MusterConfig::parse(Some("[layout]\nside_by_side_from = 0\n"), "f").unwrap();
        assert_eq!(any.preferences.side_by_side_from, 0);
        // Not a width: said, naming the file.
        let error = MusterConfig::parse(Some("[layout]\nside_by_side_from = -4\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("s.toml"), "{error}");
    }

    #[test]
    fn the_panels_are_as_written_or_muster_s_own() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.preferences.panels, Layout::default());
        assert!(!nothing.preferences.list_folded);
        let written = MusterConfig::parse(
            Some("[layout]\npanels = \"session | list 30 | (changes / shell)\"\nlist_folded = true\n"),
            "f",
        )
        .unwrap();
        // Written before the plugin's panel: it goes where the default
        // puts it, under the shell.
        assert_eq!(
            written.preferences.panels.to_string(),
            "session | list 30 | (changes / shell / plugin)"
        );
        assert!(written.preferences.list_folded);
        let old_default = MusterConfig::parse(
            Some("[layout]\npanels = \"list | session 60% | (changes / shell)\"\n"),
            "f",
        )
        .unwrap();
        assert_eq!(old_default.preferences.panels, Layout::default());
        let error = MusterConfig::parse(
            Some("[layout]\npanels = \"list | list | session\"\n"),
            "s.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(error.starts_with("s.toml"), "{error}");
        assert!(
            error.contains("layout.panels: list is in the layout twice"),
            "{error}"
        );
    }

    #[test]
    fn the_projects_order_is_as_written_or_as_they_were_added() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert!(nothing.preferences.projects.is_empty());
        let (one, two) = (crate::ProjectId::new(), crate::ProjectId::new());
        let written = MusterConfig::parse(
            Some(&format!("[layout]\nprojects = [\"{two}\", \"{one}\"]\n")),
            "f",
        )
        .unwrap();
        assert_eq!(written.preferences.projects, [two, one]);
        // An id that does not read is said, naming the key.
        let error = MusterConfig::parse(Some("[layout]\nprojects = [\"muster\"]\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.contains("layout.projects"), "{error}");
    }

    #[test]
    fn an_agent_names_chats_unless_the_person_says_not_and_by_the_agent_they_pick() {
        let nothing = MusterConfig::parse(None, "f").unwrap();
        assert_eq!(nothing.preferences.naming, Naming::default());
        assert!(nothing.preferences.naming.by_agent);
        let written = MusterConfig::parse(
            Some("[names]\nby_agent = false\nagent = \"codex\"\nmodel = \" o4 \"\n"),
            "f",
        )
        .unwrap();
        assert_eq!(
            written.preferences.naming,
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
        assert_eq!(read.preferences.keys["new_session"], ["N"]);
        assert_eq!(read.preferences.keys["leave_session"], ["Q", "alt-q"]);
        let error = MusterConfig::parse(Some("[keys]\nnew_session = \"d\"\n"), "s.toml")
            .unwrap_err()
            .to_string();
        assert!(error.starts_with("s.toml"), "{error}");
        assert!(
            error.contains("keys.new_session: d is show_changes's"),
            "{error}"
        );
        // A name that is no action reads, and is warned of.
        let read = MusterConfig::parse(Some("[keys]\nfly = \"f\"\n"), "s.toml").unwrap();
        assert!(
            read.warnings[0].contains("keys.fly is not something a key does"),
            "{:?}",
            read.warnings
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
        assert_eq!(nothing.preferences.palette, Palette::Muster);
        assert_eq!(nothing.preferences.session_lines, SessionLines::Two);
        let written = MusterConfig::parse(
            Some("[look]\ncolours = \"terminal\"\nsession_lines = 1\n"),
            "f",
        )
        .unwrap();
        assert_eq!(written.preferences.palette, Palette::Terminal);
        assert_eq!(written.preferences.session_lines, SessionLines::One);
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
        assert_eq!(nothing.preferences.telling, Telling::default());
        let written = MusterConfig::parse(
            Some(
                "[notifications]\nfinishes = false\nquestions_sound = \"Glass\"\n\
                 errors_sound = \"none\"\n",
            ),
            "f",
        )
        .unwrap();
        assert!(!written.preferences.telling.finishes.on);
        assert_eq!(
            written.preferences.telling.questions,
            Tell {
                on: true,
                sound: Sound::Named("Glass".to_owned())
            }
        );
        assert_eq!(written.preferences.telling.errors.sound, Sound::Silent);
        let error = MusterConfig::parse(
            Some("[notifications]\nerrors_sound = \"../x.wav\"\n"),
            "settings.toml",
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("notifications.errors_sound"), "{error}");
    }
}
