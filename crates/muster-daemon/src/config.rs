//! Reading a project's settings: `muster.toml` in its checkout, with the
//! person's overrides over it (`ARCHITECTURE.md` §12). Parsing is
//! `muster_core::config`'s; this finds and reads the files.

use std::path::{Path, PathBuf};

use muster_core::config::{ConfigError, ProjectConfig, edit};
use muster_core::settings::{ONLY_MINE, SettingChange, SettingSource};

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

/// The settings, and where only-my-tickets comes from.
pub fn load_with_sources(
    root: &Path,
    overrides: &Path,
) -> Result<(ProjectConfig, SettingSource), ConfigError> {
    let repository = read(&root.join(FILE_NAME))?;
    let local = read(overrides)?;
    let config = load(root, overrides)?.config;
    let only_mine = edit::source_of(repository.as_deref(), local.as_deref(), ONLY_MINE);
    Ok((config, only_mine))
}

/// Makes `change` in the person's overrides at `overrides`, keeping the
/// rest of that file as written. The repository's `muster.toml` is only
/// read (`ARC-05`). Refused, writing nothing, if the settings would not
/// read with the change.
pub fn change(root: &Path, overrides: &Path, change: SettingChange) -> Result<(), ConfigError> {
    let repository_file = root.join(FILE_NAME);
    let repository = read(&repository_file)?;
    let local = read(overrides)?;
    let shown = overrides.display().to_string();
    let invalid = |message: String| ConfigError::Invalid {
        file: shown.clone(),
        message,
    };
    let (table, key) = change.key();
    let changed = edit::set_flag(local.as_deref(), table, key, change.value()).map_err(invalid)?;
    ProjectConfig::parse(
        repository.as_deref(),
        Some(&changed),
        (&repository_file.display().to_string(), &shown),
    )?;
    write(overrides, &changed).map_err(|error| invalid(format!("cannot be written: {error}")))
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
        assert_eq!(from, SettingSource::Project);

        // No file of the person's yet: one is made, folders and all.
        change(&root, &overrides, SettingChange::OnlyMyTickets(false)).unwrap();
        let written = std::fs::read_to_string(&overrides).unwrap();
        assert!(written.starts_with(edit::NEW_FILE), "{written}");
        let (config, from) = load_with_sources(&root, &overrides).unwrap();
        assert!(!config.tickets.mine);
        assert_eq!(from, SettingSource::Yours);
        assert_eq!(
            std::fs::read_to_string(root.join(FILE_NAME)).unwrap(),
            repository,
            "the repository's file is never written"
        );

        std::fs::write(&overrides, "# kept\n[project]\nmemory = \"8g\"\n").unwrap();
        change(&root, &overrides, SettingChange::OnlyMyTickets(true)).unwrap();
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
    fn a_change_the_settings_would_not_read_with_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let overrides = dir.path().join("p.toml");
        // Already broken: the reason is given, and the file left alone.
        std::fs::write(&overrides, "[tickets]\nmien = true\n").unwrap();
        let error = change(dir.path(), &overrides, SettingChange::OnlyMyTickets(true))
            .unwrap_err()
            .to_string();
        assert!(error.contains("mien"), "{error}");
        assert_eq!(
            std::fs::read_to_string(&overrides).unwrap(),
            "[tickets]\nmien = true\n"
        );
        std::fs::write(&overrides, "tickets = \"jira\"\n").unwrap();
        let error = change(dir.path(), &overrides, SettingChange::OnlyMyTickets(true))
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
