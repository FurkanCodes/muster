//! Reading a project's settings: `muster.toml` in its checkout, with the
//! person's overrides over it (`ARCHITECTURE.md` §12). Parsing is
//! `muster_core::config`'s; this finds and reads the files.

use std::path::{Path, PathBuf};

use muster_core::config::{ConfigError, ProjectConfig};

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
    fn an_unreadable_file_is_an_error_not_a_default() {
        let dir = tempfile::tempdir().unwrap();
        // A directory where the file should be cannot be read as one.
        std::fs::create_dir(dir.path().join(FILE_NAME)).unwrap();
        let error = load(dir.path(), &dir.path().join("none.toml")).unwrap_err();
        assert!(error.to_string().contains("cannot be read"), "{error}");
    }
}
