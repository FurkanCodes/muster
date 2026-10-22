//! Reading a project's settings: `muster.toml` in its checkout, with the
//! person's overrides over it (`ARCHITECTURE.md` §12). Parsing is
//! `muster_core::config`'s; this finds and reads the files.

use std::path::{Path, PathBuf};

use muster_core::config::agents::AgentDefinition;
use muster_core::config::edit::Written;
use muster_core::config::preferences::{self, MusterConfig};
use muster_core::config::{ConfigError, ProjectConfig, edit};
use muster_core::plugins::PluginEntry;
use muster_core::settings::{
    KEYS, LIST_FOLDED, LOOK_COLOURS, LOOK_SESSION_LINES, MUSTER_PREFIX, NAMES, NOTCH,
    NOTIFICATIONS, ONLY_MINE, PANELS, PreferenceChange, Preferences, SHARED_BUILDS, SIDE_BY_SIDE,
    SettingChange, SettingSource, TRACKER, TicketsFrom,
};

/// The repository's settings file.
pub const FILE_NAME: &str = "muster.toml";

/// Where Muster's own settings keep the plugins: `[plugins.<id>]`.
const PLUGINS: &str = "plugins";

/// The settings, and the files they came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub config: ProjectConfig,
    pub files: Vec<PathBuf>,
}

/// Reads the settings of the repository at `root`, with the overrides at
/// `overrides`; either file may be missing.
pub fn load(root: &Path, overrides: &Path) -> Result<Loaded, ConfigError> {
    let repository_file = root.join(FILE_NAME);
    let repository = read(&repository_file)?;
    let local = read(overrides)?;
    let files = [
        (&repository, &repository_file),
        (&local, &overrides.to_path_buf()),
    ]
    .into_iter()
    .filter(|(text, _)| text.is_some())
    .map(|(_, path)| path.clone())
    .collect();
    let config = ProjectConfig::parse(
        repository.as_deref(),
        local.as_deref(),
        (
            &repository_file.display().to_string(),
            &overrides.display().to_string(),
        ),
    )?;
    Ok(Loaded { config, files })
}

/// Which file sets the settings the person changes on the Settings screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sources {
    pub only_mine: SettingSource,
    pub tracker: SettingSource,
    pub muster_prefix: SettingSource,
}

/// The settings, and where the ones the person changes come from.
pub fn load_with_sources(
    root: &Path,
    overrides: &Path,
) -> Result<(ProjectConfig, Sources), ConfigError> {
    let repository = read(&root.join(FILE_NAME))?;
    let local = read(overrides)?;
    let config = load(root, overrides)?.config;
    let source = |key| edit::source_of(repository.as_deref(), local.as_deref(), key);
    Ok((
        config,
        Sources {
            only_mine: source(ONLY_MINE),
            tracker: source(TRACKER),
            muster_prefix: source(MUSTER_PREFIX),
        },
    ))
}

/// Makes `change` in the person's overrides at `overrides`, keeping the
/// rest of that file as written. The repository's `muster.toml` is only
/// read (`ARC-05`). Refused, writing nothing, if the settings would not
/// read with the change.
pub fn change(root: &Path, overrides: &Path, change: &SettingChange) -> Result<(), ConfigError> {
    let repository_file = root.join(FILE_NAME);
    let repository = read(&repository_file)?;
    let local = read(overrides)?;
    let shown = overrides.display().to_string();
    let invalid = |message: String| ConfigError::Invalid {
        file: shown.clone(),
        message,
    };
    let changed = match change {
        SettingChange::OnlyMyTickets(on) => {
            let (table, key) = ONLY_MINE;
            edit::set_flag(local.as_deref(), table, key, *on)
        }
        SettingChange::MusterPrefix(on) => {
            let (table, key) = MUSTER_PREFIX;
            edit::set_flag(local.as_deref(), table, key, *on)
        }
        // Written whole: what the last tracker needed goes with it, every
        // key of the person's `[tickets]` but `mine`.
        SettingChange::TicketsFrom(from) => {
            let before = edit::keys_of(local.as_deref(), TRACKER.0);
            let mut values: Vec<(&str, Option<&str>)> = before
                .iter()
                .filter(|key| *key != ONLY_MINE.1)
                .map(|key| (key.as_str(), None))
                .collect();
            for (key, value) in from.iter().flat_map(TicketsFrom::written) {
                values.retain(|(written, _)| *written != key);
                values.push((key, Some(value)));
            }
            edit::set_texts(local.as_deref(), TRACKER.0, &values)
        }
    }
    .map_err(|error| invalid(error.to_string()))?;
    ProjectConfig::parse(
        repository.as_deref(),
        Some(&changed),
        (&repository_file.display().to_string(), &shown),
    )?;
    write(overrides, &changed).map_err(|error| invalid(format!("cannot be written: {error}")))
}

/// Muster's own settings, from `settings.toml` in the person's settings
/// folder `config_dir`.
pub fn preferences(config_dir: &Path) -> Result<Preferences, ConfigError> {
    own_settings(config_dir).map(|own| own.preferences)
}

/// Muster's own settings as clients are told them, and the coding agents
/// the person added there as the daemon starts them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OwnSettings {
    pub preferences: Preferences,
    pub agents: Vec<AgentDefinition>,
    /// What reads in them but does nothing (`MusterConfig::warnings`).
    pub warnings: Vec<String>,
}

/// Muster's own settings in `config_dir`, with the agents the person added.
pub fn own_settings(config_dir: &Path) -> Result<OwnSettings, ConfigError> {
    let file = config_dir.join(preferences::FILE_NAME);
    let config = MusterConfig::parse(read(&file)?.as_deref(), &file.display().to_string())?;
    Ok(OwnSettings {
        preferences: Preferences {
            notch: config.notch,
            file,
            agents: config.agents.iter().map(AgentDefinition::added).collect(),
            problem: None,
            side_by_side_from: config.side_by_side_from,
            palette: config.palette,
            session_lines: config.session_lines,
            panels: config.panels,
            list_folded: config.list_folded,
            naming: config.naming,
            keys: config.keys,
            notifications: config.notifications,
            telling: config.telling,
            shared_builds: config.shared_builds,
        },
        agents: config.agents,
        warnings: config.warnings,
    })
}

/// Makes `change` in Muster's own settings in `config_dir`, keeping the
/// rest of the file as written; refused, writing nothing, if they would not
/// read with it.
pub fn change_preference(config_dir: &Path, change: &PreferenceChange) -> Result<(), ConfigError> {
    let file = config_dir.join(preferences::FILE_NAME);
    let shown = file.display().to_string();
    let invalid = |message: String| ConfigError::Invalid {
        file: shown.clone(),
        message,
    };
    let text = read(&file)?;
    let changed = match change {
        PreferenceChange::Notch(on) => {
            let (table, key) = NOTCH;
            edit::set_flag_in(text.as_deref(), preferences::NEW_FILE, table, key, *on)
        }
        PreferenceChange::SideBySideFrom(columns) => {
            let (table, key) = SIDE_BY_SIDE;
            let columns = i64::from(*columns);
            edit::set_number_in(text.as_deref(), preferences::NEW_FILE, table, key, columns)
        }
        PreferenceChange::Palette(palette) => {
            let (table, key) = LOOK_COLOURS;
            let value = palette.key();
            edit::set_text_in(text.as_deref(), preferences::NEW_FILE, table, key, value)
        }
        PreferenceChange::SessionLines(lines) => {
            let (table, key) = LOOK_SESSION_LINES;
            let count = i64::from(lines.count());
            edit::set_number_in(text.as_deref(), preferences::NEW_FILE, table, key, count)
        }
        PreferenceChange::Panels(layout) => {
            let (table, key) = PANELS;
            let value = layout.to_string();
            edit::set_text_in(text.as_deref(), preferences::NEW_FILE, table, key, &value)
        }
        PreferenceChange::ListFolded(folded) => {
            let (table, key) = LIST_FOLDED;
            edit::set_flag_in(text.as_deref(), preferences::NEW_FILE, table, key, *folded)
        }
        PreferenceChange::NamesByAgent(on) => edit::set_flag_in(
            text.as_deref(),
            preferences::NEW_FILE,
            NAMES,
            "by_agent",
            *on,
        ),
        // Kept aside as it reads, a name that is neither an action of
        // Muster's nor a plugin's panel's is never set from a client: only
        // taken away.
        PreferenceChange::Keys {
            action,
            keys: Some(_),
        } if muster_core::keys::Action::named(action).is_none()
            && !muster_core::keys::is_panel_action(action) =>
        {
            return Err(invalid(format!(
                "keys.{action} is not something a key does"
            )));
        }
        PreferenceChange::Keys {
            action,
            keys: Some(keys),
        } => edit::set_list_in(text.as_deref(), preferences::NEW_FILE, KEYS, action, keys),
        PreferenceChange::Keys { action, keys: None } => {
            edit::remove_in(text.as_deref(), KEYS, action)
        }
        PreferenceChange::AllKeysBack => edit::remove_table_in(text.as_deref(), KEYS),
        PreferenceChange::Notifications(on) => {
            let (table, key) = NOTIFICATIONS;
            edit::set_flag_in(text.as_deref(), preferences::NEW_FILE, table, key, *on)
        }
        PreferenceChange::SharedBuilds(on) => {
            let (table, key) = SHARED_BUILDS;
            edit::set_flag_in(text.as_deref(), preferences::NEW_FILE, table, key, *on)
        }
        PreferenceChange::Tell { kind, tell } => {
            let (table, _) = NOTIFICATIONS;
            let sound = format!("{}_sound", kind.key());
            edit::set_flag_in(
                text.as_deref(),
                preferences::NEW_FILE,
                table,
                kind.key(),
                tell.on,
            )
            .and_then(|flagged| {
                edit::set_text_in(
                    Some(&flagged),
                    preferences::NEW_FILE,
                    table,
                    &sound,
                    tell.sound.key(),
                )
            })
        }
        PreferenceChange::NamedBy { agent, model } => edit::set_texts_in(
            text.as_deref(),
            preferences::NEW_FILE,
            NAMES,
            &[("agent", agent.as_deref()), ("model", model.as_deref())],
        ),
    }
    .map_err(|error| invalid(error.to_string()))?;
    MusterConfig::parse(Some(&changed), &shown)?;
    write(&file, &changed).map_err(|error| invalid(format!("cannot be written: {error}")))
}

/// The plugins the person installed, linked or turned off, as Muster's
/// own settings in `config_dir` hold them (`[plugins.<id>]`).
pub fn plugin_entries(config_dir: &Path) -> Result<Vec<PluginEntry>, ConfigError> {
    let file = config_dir.join(preferences::FILE_NAME);
    MusterConfig::parse(read(&file)?.as_deref(), &file.display().to_string())
        .map(|config| config.plugins)
}

/// Sets or takes out each of `values` in the plugin `id`'s
/// `[plugins.<id>]`, or with `None` takes the entry out whole, in Muster's
/// own settings in `config_dir`, keeping the rest of the file as written;
/// refused, writing nothing, if they would not read with it.
pub fn change_plugin(
    config_dir: &Path,
    id: &str,
    values: Option<&[(&str, Option<Written<'_>>)]>,
) -> Result<(), ConfigError> {
    let file = config_dir.join(preferences::FILE_NAME);
    let shown = file.display().to_string();
    let invalid = |message: String| ConfigError::Invalid {
        file: shown.clone(),
        message,
    };
    let text = read(&file)?;
    let entry = (PLUGINS, id);
    let changed = match values {
        Some(values) => edit::set_entry_in(text.as_deref(), preferences::NEW_FILE, entry, values),
        None => edit::remove_entry_in(text.as_deref(), entry),
    }
    .map_err(|error| invalid(error.to_string()))?;
    MusterConfig::parse(Some(&changed), &shown)?;
    write(&file, &changed).map_err(|error| invalid(format!("cannot be written: {error}")))
}

/// Whether the branches Muster makes in the project at `root` start with
/// `muster/`, read off the caller's loop; settings that cannot be read
/// leave them as they are by default.
pub(crate) async fn muster_prefix(root: PathBuf, overrides: PathBuf) -> bool {
    tokio::task::spawn_blocking(move || load(&root, &overrides))
        .await
        .ok()
        .and_then(Result::ok)
        .is_none_or(|loaded| loaded.config.branches.muster_prefix)
}

/// Writes `text` to `path` whole: into a file beside it, then moved over
/// it, so a reader never sees half of it.
fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("it has no folder"))?;
    std::fs::create_dir_all(parent)?;
    let name = path.file_name().map_or_else(
        || "settings".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temporary = parent.join(format!(".{name}.{}.new", std::process::id()));
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temporary);
    })
}

/// A file's text; `None` if it does not exist.
fn read(path: &Path) -> Result<Option<String>, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(ConfigError::Invalid {
            file: path.display().to_string(),
            message: format!("cannot be read: {error}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn muster_s_own_settings_are_read_and_changed_in_the_person_s_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        assert_eq!(
            preferences(&config_dir).unwrap(),
            Preferences {
                notch: true,
                file: file.clone(),
                side_by_side_from: 100,
                ..Preferences::default()
            }
        );

        // No file yet: one is made, saying what it is.
        change_preference(&config_dir, &PreferenceChange::Notch(false)).unwrap();
        assert!(!preferences(&config_dir).unwrap().notch);
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.starts_with(preferences::NEW_FILE), "{text}");

        // The person's comments stay.
        std::fs::write(&file, "# mine\n[notch]\nshow = false # for now\n").unwrap();
        change_preference(&config_dir, &PreferenceChange::Notch(true)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "# mine\n[notch]\nshow = true # for now\n"
        );

        // A file that does not read is left alone, and said so.
        std::fs::write(&file, "[notch]\nshwo = true\n").unwrap();
        let error = change_preference(&config_dir, &PreferenceChange::Notch(false))
            .unwrap_err()
            .to_string();
        assert!(error.contains("shwo"), "{error}");
        assert!(preferences(&config_dir).is_err());
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[notch]\nshwo = true\n"
        );
    }

    #[test]
    fn the_width_a_session_sits_beside_the_list_from_is_the_person_s_own() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&file, "[notch]\nshow = false # for now\n").unwrap();
        change_preference(&config_dir, &PreferenceChange::SideBySideFrom(120)).unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!(read.side_by_side_from, 120);
        assert!(!read.notch, "the rest stays as written");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[notch]\nshow = false # for now\n\n[layout]\nside_by_side_from = 120\n"
        );
        change_preference(&config_dir, &PreferenceChange::SideBySideFrom(0)).unwrap();
        assert_eq!(preferences(&config_dir).unwrap().side_by_side_from, 0);
    }

    #[test]
    fn the_panels_and_the_folded_list_are_the_person_s_own() {
        use muster_core::layout::Layout;
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&file, "[layout]\nside_by_side_from = 120 # mine\n").unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!((read.panels, read.list_folded), (Layout::default(), false));
        let moved = Layout::parse("session | list 28 | (changes / shell / plugin)").unwrap();
        change_preference(&config_dir, &PreferenceChange::Panels(moved.clone())).unwrap();
        change_preference(&config_dir, &PreferenceChange::ListFolded(true)).unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!((read.panels, read.list_folded), (moved, true));
        assert_eq!(read.side_by_side_from, 120, "the rest stays as written");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[layout]\nside_by_side_from = 120 # mine\npanels = \"session | list 28 | (changes / shell / plugin)\"\n\
             list_folded = true\n"
        );
    }

    #[test]
    fn which_notifications_and_their_sounds_are_written_beside_the_rest() {
        use muster_core::telling::{Kind, Sound, Tell};
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let file = config_dir.join("settings.toml");
        std::fs::write(&file, "# mine\n[notifications]\nshow = true\n").unwrap();
        let tell = |kind, on, sound| PreferenceChange::Tell {
            kind,
            tell: Tell { on, sound },
        };
        change_preference(&config_dir, &tell(Kind::Finish, false, Sound::Silent)).unwrap();
        change_preference(
            &config_dir,
            &tell(Kind::Question, true, Sound::Named("Glass".to_owned())),
        )
        .unwrap();
        let read = preferences(&config_dir).unwrap();
        assert!(read.notifications);
        assert!(!read.telling.finishes.on);
        assert_eq!(read.telling.finishes.sound, Sound::Silent);
        assert_eq!(
            read.telling.questions.sound,
            Sound::Named("Glass".to_owned())
        );
        assert_eq!(read.telling.errors, Tell::default());
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.starts_with("# mine\n"), "{written}");
        assert!(
            written.contains("finishes = false\nfinishes_sound = \"none\"\n"),
            "{written}"
        );
        assert!(written.contains("questions_sound = \"Glass\""), "{written}");
    }

    #[test]
    fn the_person_s_keys_are_written_taken_back_and_put_back_together() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        let keys = |action: &str, keys: Option<&[&str]>| PreferenceChange::Keys {
            action: action.to_owned(),
            keys: keys.map(|keys| keys.iter().map(|key| (*key).to_owned()).collect()),
        };
        change_preference(&config_dir, &keys("new_session", Some(&["N"]))).unwrap();
        change_preference(&config_dir, &keys("leave_session", Some(&["Q", "alt-q"]))).unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!(read.keys["leave_session"], ["Q", "alt-q"]);
        assert!(
            std::fs::read_to_string(&file)
                .unwrap()
                .contains("[keys]\nnew_session = [\"N\"]\n"),
        );
        // A key another action has is refused, and nothing is written.
        let before = std::fs::read_to_string(&file).unwrap();
        let error = change_preference(&config_dir, &keys("rename", Some(&["d"]))).unwrap_err();
        assert!(error.to_string().contains("show_changes"), "{error}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        change_preference(&config_dir, &keys("new_session", None)).unwrap();
        assert!(
            !preferences(&config_dir)
                .unwrap()
                .keys
                .contains_key("new_session")
        );
        change_preference(&config_dir, &PreferenceChange::AllKeysBack).unwrap();
        assert!(preferences(&config_dir).unwrap().keys.is_empty());
        assert!(!std::fs::read_to_string(&file).unwrap().contains("[keys]"));
    }

    #[test]
    fn a_removed_plugin_s_key_leaves_muster_s_own_settings_readable() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            &file,
            "[notch]\nshow = false\n\n[keys]\nzoetrope_graph = \"Z\"\nrename = \"F2\"\n",
        )
        .unwrap();
        let own = own_settings(&config_dir).unwrap();
        assert!(!own.preferences.notch, "the rest as written");
        assert_eq!(own.preferences.keys["zoetrope_graph"], ["Z"]);
        assert_eq!(own.warnings.len(), 1, "{:?}", own.warnings);
        // The file still takes changes, the key kept as it is.
        change_preference(&config_dir, &PreferenceChange::Notch(true)).unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(written.contains("zoetrope_graph = \"Z\""), "{written}");
        // No client sets a key for an action Muster lacks, but one can take
        // it away.
        let keys = |keys: Option<Vec<String>>| PreferenceChange::Keys {
            action: "zoetrope_graph".to_owned(),
            keys,
        };
        let error = change_preference(&config_dir, &keys(Some(vec!["Y".to_owned()]))).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("keys.zoetrope_graph is not something a key does"),
            "{error}"
        );
        change_preference(&config_dir, &keys(None)).unwrap();
        let own = own_settings(&config_dir).unwrap();
        assert!(own.warnings.is_empty() && !own.preferences.keys.contains_key("zoetrope_graph"));
    }

    #[test]
    fn a_key_for_a_plugin_s_panel_is_set_and_taken_away_by_its_name_prd_15() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        let keys = |keys: Option<Vec<String>>| PreferenceChange::Keys {
            action: "plugin.zoetrope.graph".to_owned(),
            keys,
        };
        change_preference(&config_dir, &keys(Some(vec!["G".to_owned()]))).unwrap();
        let written = std::fs::read_to_string(config_dir.join("settings.toml")).unwrap();
        assert!(
            written.contains("\"plugin.zoetrope.graph\" = [\"G\"]"),
            "{written}"
        );
        let own = own_settings(&config_dir).unwrap();
        assert_eq!(own.preferences.keys["plugin.zoetrope.graph"], ["G"]);
        change_preference(&config_dir, &keys(None)).unwrap();
        let own = own_settings(&config_dir).unwrap();
        assert!(own.preferences.keys.is_empty());
    }

    #[test]
    fn naming_by_an_agent_is_the_person_s_own() {
        use muster_core::settings::Naming;
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        assert_eq!(preferences(&config_dir).unwrap().naming, Naming::default());
        change_preference(&config_dir, &PreferenceChange::NamesByAgent(false)).unwrap();
        change_preference(
            &config_dir,
            &PreferenceChange::NamedBy {
                agent: Some("codex".to_owned()),
                model: Some("o4".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(
            preferences(&config_dir).unwrap().naming,
            Naming {
                by_agent: false,
                agent: Some("codex".to_owned()),
                model: Some("o4".to_owned()),
            }
        );
        // Each session's own again, with its own model: the keys go.
        change_preference(
            &config_dir,
            &PreferenceChange::NamedBy {
                agent: None,
                model: None,
            },
        )
        .unwrap();
        let written = std::fs::read_to_string(&file).unwrap();
        assert!(
            written.ends_with("[names]\nby_agent = false\n"),
            "{written}"
        );
        assert!(
            written.starts_with("# Your own settings for Muster"),
            "{written}"
        );
    }

    #[test]
    fn the_look_is_the_person_s_own_and_written_beside_the_rest() {
        use muster_core::settings::{Palette, SessionLines};
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join("config");
        let file = config_dir.join("settings.toml");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(&file, "[layout]\nside_by_side_from = 120 # mine\n").unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!(
            (read.palette, read.session_lines),
            (Palette::Muster, SessionLines::Two)
        );
        change_preference(&config_dir, &PreferenceChange::Palette(Palette::Terminal)).unwrap();
        change_preference(
            &config_dir,
            &PreferenceChange::SessionLines(SessionLines::One),
        )
        .unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!(
            (read.palette, read.session_lines, read.side_by_side_from),
            (Palette::Terminal, SessionLines::One, 120)
        );
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[layout]\nside_by_side_from = 120 # mine\n\n[look]\ncolours = \"terminal\"\n\
             session_lines = 1\n"
        );
        change_preference(&config_dir, &PreferenceChange::Palette(Palette::Muster)).unwrap();
        assert_eq!(preferences(&config_dir).unwrap().palette, Palette::Muster);
    }

    #[test]
    fn both_files_are_read_and_either_may_be_missing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let overrides = dir.path().join("config/projects/p.toml");
        std::fs::create_dir_all(&root).unwrap();

        let none = load(&root, &overrides).unwrap();
        assert_eq!(none.config, ProjectConfig::default());
        assert!(none.files.is_empty());

        std::fs::write(
            root.join(FILE_NAME),
            "[checks]\ncommands = [\"make test\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(overrides.parent().unwrap()).unwrap();
        std::fs::write(&overrides, "[project]\nmemory = \"8g\"\n").unwrap();
        let both = load(&root, &overrides).unwrap();
        assert_eq!(both.config.checks.commands, ["make test"]);
        assert_eq!(both.config.memory_bytes, 8 << 30);
        assert_eq!(both.files, [root.join(FILE_NAME), overrides.clone()]);

        std::fs::write(&overrides, "[project]\nmemroy = \"8g\"\n").unwrap();
        let error = load(&root, &overrides).unwrap_err().to_string();
        assert!(error.contains("memroy"), "{error}");
    }

    #[test]
    fn a_change_goes_into_the_person_s_file_only_and_keeps_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let overrides = dir.path().join("config/projects/p.toml");
        std::fs::create_dir_all(&root).unwrap();
        let repository = "[tickets]\nmine = true\n";
        std::fs::write(root.join(FILE_NAME), repository).unwrap();
        let (config, from) = load_with_sources(&root, &overrides).unwrap();
        assert!(config.tickets.mine);
        assert_eq!(from.only_mine, SettingSource::Project);

        // No file of the person's yet: one is made, folders and all.
        change(&root, &overrides, &SettingChange::OnlyMyTickets(false)).unwrap();
        let written = std::fs::read_to_string(&overrides).unwrap();
        assert!(written.starts_with(edit::NEW_FILE), "{written}");
        let (config, from) = load_with_sources(&root, &overrides).unwrap();
        assert!(!config.tickets.mine);
        assert_eq!(from.only_mine, SettingSource::Yours);
        assert_eq!(
            std::fs::read_to_string(root.join(FILE_NAME)).unwrap(),
            repository,
            "the repository's file is never written"
        );

        std::fs::write(&overrides, "# kept\n[project]\nmemory = \"8g\"\n").unwrap();
        change(&root, &overrides, &SettingChange::OnlyMyTickets(true)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# kept\n[project]\nmemory = \"8g\"\n\n[tickets]\nmine = true\n"
        );
        let leftovers: Vec<_> = std::fs::read_dir(overrides.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, ["p.toml"]);
    }

    #[test]
    fn where_the_tickets_are_is_written_whole_and_can_follow_the_remote_again() {
        use muster_core::settings::TicketsFrom;

        let from = |tracker: &str, fields: &[(&str, &str)]| TicketsFrom {
            tracker: tracker.to_owned(),
            fields: fields
                .iter()
                .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
                .collect(),
        };
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let overrides = dir.path().join("config/projects/p.toml");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(overrides.parent().unwrap()).unwrap();
        // A project on a company's own git server, its address an IP.
        std::fs::write(&overrides, "# mine\n[tickets]\nmine = true\n").unwrap();
        let jira = from(
            "jira",
            &[("site", "acme.atlassian.net"), ("project", "app")],
        );
        change(
            &root,
            &overrides,
            &SettingChange::TicketsFrom(Some(jira.clone())),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\ntracker = \"jira\"\nproject = \"app\"\n\
             site = \"acme.atlassian.net\"\n"
        );
        let (config, sources) = load_with_sources(&root, &overrides).unwrap();
        assert_eq!(config.tickets.tracker.as_deref(), Some("jira"));
        assert_eq!(config.tickets.field("project"), Some("APP"));
        assert!(config.tickets.mine);
        assert_eq!(sources.tracker, SettingSource::Yours);

        // Linear instead: Jira's keys go with Jira.
        let linear = from("linear", &[("team", "ENG")]);
        change(&root, &overrides, &SettingChange::TicketsFrom(Some(linear))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\ntracker = \"linear\"\nteam = \"ENG\"\n"
        );

        // Anyone's plugin, with keys of its own, which go with it too.
        let theirs = from("youtrack", &[("board", "WEB")]);
        change(&root, &overrides, &SettingChange::TicketsFrom(Some(theirs))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\ntracker = \"youtrack\"\nboard = \"WEB\"\n"
        );

        // Jira without its site does not read: refused, nothing written.
        let before = std::fs::read_to_string(&overrides).unwrap();
        let mut siteless = jira;
        siteless.fields.remove("site");
        let error = change(
            &root,
            &overrides,
            &SettingChange::TicketsFrom(Some(siteless)),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("tickets.site"), "{error}");
        assert_eq!(std::fs::read_to_string(&overrides).unwrap(), before);

        // Back to the git remote: only the person's other settings stay.
        change(&root, &overrides, &SettingChange::TicketsFrom(None)).unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\n"
        );
        let (config, sources) = load_with_sources(&root, &overrides).unwrap();
        assert_eq!(config.tickets.tracker, None);
        assert_eq!(sources.tracker, SettingSource::Default);
    }

    #[test]
    fn a_change_the_settings_would_not_read_with_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let overrides = dir.path().join("p.toml");
        // Already broken: the reason is given, and the file left alone.
        std::fs::write(&overrides, "[tickets]\nmien = true\n").unwrap();
        let error = change(dir.path(), &overrides, &SettingChange::OnlyMyTickets(true))
            .unwrap_err()
            .to_string();
        assert!(error.contains("mien"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "[tickets]\nmien = true\n"
        );
        std::fs::write(&overrides, "tickets = \"jira\"\n").unwrap();
        let error = change(dir.path(), &overrides, &SettingChange::OnlyMyTickets(true))
            .unwrap_err()
            .to_string();
        assert!(error.contains("not a table"), "{error}");
    }

    #[test]
    fn an_unreadable_file_is_an_error_not_a_default() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the file should be cannot be read as one.
        std::fs::create_dir(dir.path().join(FILE_NAME)).unwrap();
        let error = load(dir.path(), &dir.path().join("none.toml")).unwrap_err();
        assert!(error.to_string().contains("cannot be read"), "{error}");
    }
}
