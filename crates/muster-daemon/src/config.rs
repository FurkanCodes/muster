//! Reading a project's settings: `muster.toml` in its checkout, with the
//! person's overrides over it (`ARCHITECTURE.md` §12). Parsing is
//! `muster_core::config`'s; this finds and reads the files.

use std::path::{Path, PathBuf};

use muster_core::config::agents::AgentDefinition;
use muster_core::config::preferences::{self, MusterConfig};
use muster_core::config::{ConfigError, ProjectConfig, edit};
use muster_core::settings::{
    LIST_FOLDED, LOOK_COLOURS, LOOK_SESSION_LINES, MUSTER_PREFIX, NOTCH, ONLY_MINE, PANELS,
    PreferenceChange, Preferences, SIDE_BY_SIDE, SettingChange, SettingSource, TICKETS_FROM_KEYS,
    TRACKER,
};

/// The repository's settings file.
pub const FILE_NAME: &str = "muster.toml";

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
        // Written whole: what the last tracker needed goes with it.
        SettingChange::TicketsFrom(from) => {
            let values: Vec<(&str, Option<&str>)> = match &from {
                Some(from) => from.written().to_vec(),
                None => TICKETS_FROM_KEYS.iter().map(|key| (*key, None)).collect(),
            };
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
        },
        agents: config.agents,
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
        let moved = Layout::parse("session | list 28 | (changes / shell)").unwrap();
        change_preference(&config_dir, &PreferenceChange::Panels(moved.clone())).unwrap();
        change_preference(&config_dir, &PreferenceChange::ListFolded(true)).unwrap();
        let read = preferences(&config_dir).unwrap();
        assert_eq!((read.panels, read.list_folded), (moved, true));
        assert_eq!(read.side_by_side_from, 120, "the rest stays as written");
        assert_eq!(
            std::fs::read_to_string(&file).unwrap(),
            "[layout]\nside_by_side_from = 120 # mine\npanels = \"session | list 28 | (changes / shell)\"\n\
             list_folded = true\n"
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
        use muster_core::config::TrackerKind;
        use muster_core::settings::TicketsFrom;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let overrides = dir.path().join("config/projects/p.toml");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(overrides.parent().unwrap()).unwrap();
        // A project on a company's own git server, its address an IP.
        std::fs::write(&overrides, "# mine\n[tickets]\nmine = true\n").unwrap();
        let jira = TicketsFrom {
            tracker: TrackerKind::Jira,
            site: Some("acme.atlassian.net".to_owned()),
            project: Some("app".to_owned()),
            jql: None,
            team: None,
        };
        change(
            &root,
            &overrides,
            &SettingChange::TicketsFrom(Some(jira.clone())),
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\n\
             project = \"app\"\n"
        );
        let (config, from) = load_with_sources(&root, &overrides).unwrap();
        assert_eq!(config.tickets.tracker, Some(TrackerKind::Jira));
        assert_eq!(config.tickets.project.as_deref(), Some("APP"));
        assert!(config.tickets.mine);
        assert_eq!(from.tracker, SettingSource::Yours);

        // Linear instead: Jira's keys go with Jira.
        let linear = TicketsFrom {
            tracker: TrackerKind::Linear,
            site: None,
            project: None,
            jql: None,
            team: Some("ENG".to_owned()),
        };
        change(&root, &overrides, &SettingChange::TicketsFrom(Some(linear))).unwrap();
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "# mine\n[tickets]\nmine = true\ntracker = \"linear\"\nteam = \"ENG\"\n"
        );

        // Jira without its site does not read: refused, nothing written.
        let before = std::fs::read_to_string(&overrides).unwrap();
        let siteless = TicketsFrom { site: None, ..jira };
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
        let (config, from) = load_with_sources(&root, &overrides).unwrap();
        assert_eq!(config.tickets.tracker, None);
        assert_eq!(from.tracker, SettingSource::Default);
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
