//! The plugins the person installed, linked or turned off, as Muster's own
//! `settings.toml` holds them: a `[plugins.<id>]` for each, with where it
//! is from, whether it is on, and the hash of the manifest they approved
//! (`ARCHITECTURE.md` §7a, §12). Read strictly, naming the key that is
//! wrong; whether its folder and manifest are still as approved is the
//! daemon's to look at.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Where a plugin is from.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    /// One of Muster's own, built into it.
    Builtin,
    /// Installed from a git repository.
    Git {
        /// The repository's address, as the person gave it.
        url: String,
        /// The branch or tag named, if one was; else its default branch.
        #[serde(default)]
        git_ref: Option<String>,
        /// The commit installed.
        commit: String,
    },
    /// A folder linked where it is, for its author.
    Folder { path: PathBuf },
}

/// One plugin's entry, `[plugins.<id>]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginEntry {
    pub id: String,
    pub source: PluginSource,
    /// Whether the person has it on (`enabled`, on unless written).
    pub enabled: bool,
    /// The SHA-256 of the manifest as the person approved it; `None` for
    /// one never approved, and for Muster's own, which need none.
    pub approved: Option<String>,
}

/// Why `[plugins]` does not read. Each says which key.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error("plugins must be a table, with a [plugins.<id>] for each plugin")]
    NotATable,
    #[error(
        "plugins.{id}: a plugin's id is 1 to 32 of a-z, 0-9 and -, starting with a letter or \
         digit"
    )]
    BadId { id: String },
    #[error("plugins.{id} must be a table of its settings: [plugins.{id}]")]
    NotAPlugin { id: String },
    #[error("plugins.{id}.{key} is not a setting of a plugin: they are {known}")]
    UnknownKey {
        id: String,
        key: String,
        known: String,
    },
    #[error("plugins.{id}.{key} must be {expected}")]
    Invalid {
        id: String,
        key: &'static str,
        expected: &'static str,
    },
    #[error("plugins.{id} names a git repository and a folder; a plugin is from one of them")]
    Both { id: String },
    #[error("plugins.{id}.commit is missing: the commit installed from git")]
    NoCommit { id: String },
}

/// The keys of an entry.
const KEYS: [&str; 6] = ["git", "ref", "commit", "folder", "enabled", "approved"];

/// Reads the `plugins` table of Muster's own settings, ordered by id.
pub fn parse(plugins: &toml::Value) -> Result<Vec<PluginEntry>, RegistryError> {
    let toml::Value::Table(plugins) = plugins else {
        return Err(RegistryError::NotATable);
    };
    plugins.iter().map(|(id, value)| entry(id, value)).collect()
}

fn entry(id: &str, value: &toml::Value) -> Result<PluginEntry, RegistryError> {
    if !super::is_id(id) {
        return Err(RegistryError::BadId { id: id.to_owned() });
    }
    let toml::Value::Table(table) = value else {
        return Err(RegistryError::NotAPlugin { id: id.to_owned() });
    };
    if let Some(key) = table.keys().find(|key| !KEYS.contains(&key.as_str())) {
        return Err(RegistryError::UnknownKey {
            id: id.to_owned(),
            key: key.clone(),
            known: KEYS.join(", "),
        });
    }
    let invalid = |key: &'static str, expected: &'static str| RegistryError::Invalid {
        id: id.to_owned(),
        key,
        expected,
    };
    let text = |key: &'static str, expected: &'static str, fits: &dyn Fn(&str) -> bool| match table
        .get(key)
    {
        None => Ok(None),
        Some(toml::Value::String(text)) if fits(text) => Ok(Some(text.clone())),
        Some(_) => Err(invalid(key, expected)),
    };
    let git = text(
        "git",
        "a git repository's address: https://, ssh://, user@host:path, file:// or a folder's \
         full path, without a password",
        &|text| super::check_address(text).is_ok(),
    )?;
    let git_ref = text("ref", "a branch or tag", &super::is_git_ref)?;
    let commit = text(
        "commit",
        "a commit's id, as git prints it",
        &super::is_commit,
    )?;
    let folder = text("folder", "a folder's full path", &|text| {
        text.starts_with('/') && !text.chars().any(char::is_control)
    })?;
    let approved = text(
        "approved",
        "the SHA-256 of a manifest, as Muster writes it",
        &super::is_sha256,
    )?;
    let enabled = match table.get("enabled") {
        None => true,
        Some(toml::Value::Boolean(enabled)) => *enabled,
        Some(_) => return Err(invalid("enabled", "true or false")),
    };
    let source = match (git, folder) {
        (Some(_), Some(_)) => return Err(RegistryError::Both { id: id.to_owned() }),
        (Some(url), None) => PluginSource::Git {
            url,
            git_ref,
            commit: commit.ok_or_else(|| RegistryError::NoCommit { id: id.to_owned() })?,
        },
        (None, folder) => {
            if git_ref.is_some() {
                return Err(invalid("ref", "written only with git"));
            }
            if commit.is_some() {
                return Err(invalid("commit", "written only with git"));
            }
            match folder {
                Some(path) => PluginSource::Folder {
                    path: PathBuf::from(path),
                },
                None => PluginSource::Builtin,
            }
        }
    };
    Ok(PluginEntry {
        id: id.to_owned(),
        source,
        enabled,
        approved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(text: &str) -> Result<Vec<PluginEntry>, RegistryError> {
        let table: toml::Table = toml::from_str(text).unwrap();
        parse(&table["plugins"])
    }

    const COMMIT: &str = "4f1c2ab0d3e4f5a6b7c8d9e0f1a2b3c4d5e6f7a8";

    fn hash() -> String {
        "9b".repeat(32)
    }

    #[test]
    fn each_kind_of_entry_reads_ordered_by_id() {
        let text = format!(
            "[plugins.zoetrope]\ngit = \"https://github.com/someone/zoetrope\"\nref = \"v0.1\"\n\
             commit = \"{COMMIT}\"\nenabled = true\napproved = \"{}\"\n\n\
             [plugins.my-panel]\nfolder = \"/Users/me/my-panel\"\nenabled = false\n\
             approved = \"{}\"\n\n[plugins.jira]\nenabled = false\n",
            hash(),
            hash()
        );
        let entries = read(&text).unwrap();
        assert_eq!(
            entries,
            [
                PluginEntry {
                    id: "jira".to_owned(),
                    source: PluginSource::Builtin,
                    enabled: false,
                    approved: None,
                },
                PluginEntry {
                    id: "my-panel".to_owned(),
                    source: PluginSource::Folder {
                        path: PathBuf::from("/Users/me/my-panel")
                    },
                    enabled: false,
                    approved: Some(hash()),
                },
                PluginEntry {
                    id: "zoetrope".to_owned(),
                    source: PluginSource::Git {
                        url: "https://github.com/someone/zoetrope".to_owned(),
                        git_ref: Some("v0.1".to_owned()),
                        commit: COMMIT.to_owned(),
                    },
                    enabled: true,
                    approved: Some(hash()),
                },
            ]
        );
    }

    #[test]
    fn an_entry_is_on_unless_written_off_and_unapproved_unless_written() {
        let entries = read("[plugins.p]\nfolder = \"/p\"\n").unwrap();
        assert!(entries[0].enabled);
        assert_eq!(entries[0].approved, None);
    }

    #[test]
    fn what_is_wrong_is_named_by_its_key() {
        for (text, says) in [
            ("plugins = 3", "plugins must be a table"),
            (
                "[plugins]\nzoe = 3",
                "plugins.zoe must be a table of its settings",
            ),
            (
                "[plugins.Zoe]\nfolder = \"/z\"",
                "plugins.Zoe: a plugin's id is",
            ),
            (
                "[plugins.z]\nfoldr = \"/z\"",
                "plugins.z.foldr is not a setting of a plugin",
            ),
            (
                "[plugins.z]\nfolder = \"z\"",
                "plugins.z.folder must be a folder's full path",
            ),
            (
                "[plugins.z]\nenabled = \"yes\"",
                "plugins.z.enabled must be true or false",
            ),
            (
                "[plugins.z]\nfolder = \"/z\"\napproved = \"abc\"",
                "plugins.z.approved must be",
            ),
            (
                "[plugins.z]\ngit = \"http://h/z\"\ncommit = \"x\"",
                "plugins.z.git must be",
            ),
            (
                "[plugins.z]\ngit = \"https://me:pw@h/z\"",
                "plugins.z.git must be",
            ),
            (
                "[plugins.z]\ngit = \"https://h/z\"",
                "plugins.z.commit is missing",
            ),
            (
                "[plugins.z]\ngit = \"https://h/z\"\ncommit = \"a1b2\"",
                "plugins.z.commit must be",
            ),
            (
                "[plugins.z]\ngit = \"https://h/z\"\nref = \"-x\"",
                "plugins.z.ref must be",
            ),
            (
                "[plugins.z]\nfolder = \"/z\"\nref = \"main\"",
                "plugins.z.ref must be written only with git",
            ),
            (
                "[plugins.z]\nfolder = \"/z\"\ngit = \"/y\"",
                "plugins.z names a git repository and a folder",
            ),
        ] {
            let error = read(text).unwrap_err().to_string();
            assert!(error.starts_with(says), "{text}: {error}");
        }
    }
}
