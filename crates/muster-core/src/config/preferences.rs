//! Muster's own settings: the person's for all of Muster rather than any
//! project's, in `settings.toml` beside the projects' overrides in their
//! settings folder. Read strictly, as a project's are; parsing does no IO.

use serde::Deserialize;

use super::ConfigError;

/// What the file is called, in the person's settings folder.
pub const FILE_NAME: &str = "settings.toml";

/// What a new file starts with, so a person who finds it knows what it is.
pub const NEW_FILE: &str = "# Your own settings for Muster as a whole.\n# Muster's Settings \
                            screen writes here, and so can you.\n";

/// Muster's own settings as written; what is not written is Muster's
/// default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MusterConfig {
    /// Show the agents in the MacBook's notch while the person looks
    /// elsewhere (`[notch] show`).
    pub notch: bool,
}

impl Default for MusterConfig {
    fn default() -> Self {
        Self { notch: true }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    notch: Notch,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Notch {
    show: Option<bool>,
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
        Ok(Self {
            notch: read.notch.show.unwrap_or(Self::default().notch),
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
}
