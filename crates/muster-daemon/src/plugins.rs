//! The plugins the person installed (`ARCHITECTURE.md` §7a, `PRD-15`): the
//! registry the Supervisor owns (`ARC-02`), read from Muster's own
//! settings and the plugins' folders, and the files behind it under
//! `<data>/plugins`. Nothing of a plugin runs here: git fetches it, and
//! Muster reads its manifest.
//!
//! Each plugin's manifest is hashed as the registry is read, as the daemon
//! starts, when the person has Muster's settings read again, and for each
//! listing: one that is not the manifest the person approved is listed as
//! changed, and cannot run until approved again.

mod change;
mod fetch;
mod files;
mod previews;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use muster_core::config::ConfigError;
use muster_core::plugins::{Manifest, ManifestError, PluginEntry, PluginSource, manifest};
use muster_core::protocol::plugins::{
    PluginChange, PluginDetails, PluginInfo, PluginList, PluginProblem,
};
use muster_core::protocol::{ErrorBody, ErrorCode};
use muster_core::time::Timestamp;

pub(crate) use self::fetch::{Fetched, Job, fetch};
pub(crate) use self::files::remove_later;

/// One of Muster's own plugins, built into it: its manifest, compiled in,
/// whose id is its own (`Manifest::parse_own`). Spoken to through the same
/// protocol as anyone's (`ARC-23`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct Builtin {
    pub(crate) manifest: &'static str,
}

/// Muster's own plugins. None yet: its ticket sources move here.
pub(crate) const BUILTINS: &[Builtin] = &[];

/// Why a change to the plugins was refused, said for the person.
#[derive(Debug, thiserror::Error)]
pub(crate) enum PluginError {
    #[error("{0}")]
    Address(#[from] muster_core::plugins::AddressError),
    #[error("{0:?} is not a branch or tag git would fetch")]
    Ref(String),
    #[error("could not fetch the plugin: {0}")]
    Fetch(String),
    #[error("{} has no muster-plugin.toml, which says what a plugin is and runs", path.display())]
    NoManifest { path: PathBuf },
    #[error("{}: {error}", path.display())]
    Manifest { path: PathBuf, error: ManifestError },
    #[error("{} is more than 64 KiB, which no manifest is", path.display())]
    TooLarge { path: PathBuf },
    #[error("{} is a link; a manifest must be a file of the plugin's own", path.display())]
    Link { path: PathBuf },
    #[error("{} is not a folder", path.display())]
    NotAFolder { path: PathBuf },
    #[error(
        "a plugin called {id} is installed already: `muster plugin update {id}` updates it, \
         or remove it first"
    )]
    Installed { id: String },
    #[error("{id} is one of Muster's own: it can be turned off, not replaced or removed")]
    Own { id: String },
    #[error("there is no plugin called {id}")]
    NoPlugin { id: String },
    #[error(
        "that preview is no longer held: it was installed, let go, or kept longer than ten \
         minutes; preview it again"
    )]
    NoPreview,
    #[error("{id} cannot be turned on: {problem}")]
    CannotRun { id: String, problem: PluginProblem },
    #[error("its manifest calls it {found} now, not {id}; remove {id} and install it afresh")]
    Renamed { id: String, found: String },
    #[error("Muster's own settings: {0}")]
    Settings(#[from] ConfigError),
    #[error("{what}: {error}")]
    Io { what: String, error: std::io::Error },
}

impl PluginError {
    /// The refusal as a client is told it (`ARC-07`).
    pub(crate) fn body(&self) -> ErrorBody {
        let code = match self {
            Self::NoPlugin { .. } | Self::NoPreview => ErrorCode::NotFound,
            Self::Installed { .. }
            | Self::Own { .. }
            | Self::CannotRun { .. }
            | Self::Renamed { .. } => ErrorCode::Conflict,
            Self::Io { .. } => ErrorCode::Internal,
            _ => ErrorCode::BadRequest,
        };
        ErrorBody::new(code, self.to_string())
    }

    fn io(what: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        let what = what.into();
        move |error| Self::Io { what, error }
    }
}

/// A plugin as last read: its entry in Muster's settings, if it has one
/// (one of Muster's own need not), and what the person is told of it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Known {
    entry: Option<PluginEntry>,
    info: PluginInfo,
}

/// The registry as read from the files, to be taken by [`Registry::take`].
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Reading {
    known: Vec<Known>,
    /// Why Muster's own settings do not read, if they do not.
    problem: Option<String>,
}

/// The plugins, owned by the Supervisor (`ARC-02`).
#[derive(Debug)]
pub(crate) struct Registry {
    /// `<data>/plugins`.
    dir: PathBuf,
    /// The person's settings folder, with Muster's own `settings.toml`.
    config_dir: PathBuf,
    known: Vec<Known>,
    problem: Option<String>,
    previews: previews::Previews,
    /// The person's last change to each plugin, while this daemon runs;
    /// one leaves with its plugin (`ARC-14`).
    changes: HashMap<String, PluginChange>,
}

impl Registry {
    /// The registry of plugins under `data_dir`, with Muster's own settings
    /// in `config_dir`, before it is read.
    pub(crate) fn new(data_dir: &Path, config_dir: PathBuf) -> Self {
        Self {
            dir: data_dir.join("plugins"),
            config_dir,
            known: Vec::new(),
            problem: None,
            previews: previews::Previews::default(),
            changes: HashMap::new(),
        }
    }

    /// Where previews' clones go.
    pub(crate) fn incoming(&self) -> PathBuf {
        files::incoming(&self.dir)
    }

    /// Reads the registry from the files: blocking IO, small.
    pub(crate) fn read(&self) -> Reading {
        read(&self.dir, &self.config_dir)
    }

    /// Takes `reading` as the registry; whether anything the person is told
    /// changed.
    pub(crate) fn take(&mut self, reading: Reading) -> bool {
        let before = self.infos();
        self.known = reading.known;
        self.problem = reading.problem;
        // A plugin removed by hand takes what was known of it along.
        let known = &self.known;
        self.changes
            .retain(|id, _| known.iter().any(|known| known.info.id == *id));
        self.infos() != before
    }

    /// The plugins' folder and Muster's settings folder, for reading off
    /// the loop.
    pub(crate) fn folders(&self) -> (PathBuf, PathBuf) {
        (self.dir.clone(), self.config_dir.clone())
    }

    /// Each plugin, as the person is told of it.
    pub(crate) fn infos(&self) -> Vec<PluginInfo> {
        self.known
            .iter()
            .map(|known| PluginInfo {
                changed: self.changes.get(&known.info.id).copied(),
                ..known.info.clone()
            })
            .collect()
    }

    /// Every plugin, as `List` is answered at `now`.
    pub(crate) fn list(&self, now: Timestamp) -> PluginList {
        PluginList {
            plugins: self.infos(),
            now,
            problem: self.problem.clone(),
        }
    }

    fn find(&self, id: &str) -> Result<&Known, PluginError> {
        self.known
            .iter()
            .find(|known| known.info.id == id)
            .ok_or_else(|| PluginError::NoPlugin { id: id.to_owned() })
    }

    /// How many previews are held, for tests.
    #[cfg(test)]
    pub(crate) fn previews(&self) -> usize {
        self.previews.len()
    }
}

/// The registry as the daemon starts: the last daemon's previews let go,
/// and what no entry names swept from the plugins' folder `dir` (`ARC-14`),
/// unless Muster's own settings in `config_dir` do not read, when nothing
/// there is known to be let go.
pub(crate) fn start(dir: &Path, config_dir: &Path) -> Reading {
    if let Err(error) = files::remove(&files::incoming(dir)) {
        tracing::warn!(%error, "could not let go of the last daemon's previews");
    }
    if let Ok(entries) = crate::config::plugin_entries(config_dir) {
        for path in files::sweep(dir, &entries) {
            tracing::info!(folder = %path.display(), "removed a plugin's folder no plugin uses");
        }
    }
    read(dir, config_dir)
}

/// The registry in `dir` and Muster's own settings in `config_dir`: each
/// of Muster's own plugins, then each the person added, by id.
pub(crate) fn read(dir: &Path, config_dir: &Path) -> Reading {
    let (entries, problem) = match crate::config::plugin_entries(config_dir) {
        Ok(entries) => (entries, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let mut known: Vec<Known> = BUILTINS
        .iter()
        .map(|builtin| {
            let read = Manifest::parse_own(builtin.manifest);
            let id = read.as_ref().map_or("?", |manifest| manifest.id.as_str());
            let entry = entries.iter().find(|entry| entry.id == id).cloned();
            own(builtin, entry)
        })
        .collect();
    for entry in entries {
        if known.iter().any(|known| known.info.id == entry.id) {
            continue;
        }
        known.push(look(dir, entry));
    }
    known.sort_by(|a, b| a.info.id.cmp(&b.info.id));
    Reading { known, problem }
}

/// One of Muster's own, with the person's entry for it, if any.
fn own(builtin: &Builtin, entry: Option<PluginEntry>) -> Known {
    let enabled = entry.as_ref().is_none_or(|entry| entry.enabled);
    let (id, problem, details) = match Manifest::parse_own(builtin.manifest) {
        Ok(manifest) => {
            let details =
                PluginDetails::of(&manifest, sha256(builtin.manifest.as_bytes()), on_path);
            (manifest.id, None, Some(details))
        }
        Err(error) => (
            "?".to_owned(),
            Some(PluginProblem::Unreadable {
                message: error.to_string(),
            }),
            None,
        ),
    };
    Known {
        entry,
        info: PluginInfo {
            id,
            source: PluginSource::Builtin,
            enabled,
            problem,
            details,
            changed: None,
        },
    }
}

/// A plugin the person added, as its folder holds it now.
fn look(dir: &Path, entry: PluginEntry) -> Known {
    let folder = folder_of(dir, &entry);
    let (problem, details) = match &folder {
        None => (Some(PluginProblem::Unknown), None),
        Some(folder) => match read_manifest(folder) {
            Err(PluginError::NoManifest { path } | PluginError::NotAFolder { path }) => {
                (Some(PluginProblem::Missing { path }), None)
            }
            Err(error) => (
                Some(PluginProblem::Unreadable {
                    message: error.to_string(),
                }),
                None,
            ),
            Ok((manifest, hash)) => {
                let problem = match &entry.approved {
                    None => Some(PluginProblem::NotApproved),
                    Some(approved) if *approved != hash => Some(PluginProblem::Changed),
                    Some(_) if manifest.id != entry.id => Some(PluginProblem::Unreadable {
                        message: format!("its manifest calls it {}", manifest.id),
                    }),
                    Some(_) => None,
                };
                (problem, Some(PluginDetails::of(&manifest, hash, on_path)))
            }
        },
    };
    Known {
        info: PluginInfo {
            id: entry.id.clone(),
            source: entry.source.clone(),
            enabled: entry.enabled,
            problem,
            details,
            changed: None,
        },
        entry: Some(entry),
    }
}

/// Where the plugin of `entry` is: its commit's folder in `dir`, or the
/// folder it is linked from; `None` for one naming neither, which is no
/// plugin of Muster's own (those are read apart).
fn folder_of(dir: &Path, entry: &PluginEntry) -> Option<PathBuf> {
    match &entry.source {
        PluginSource::Git { commit, .. } => Some(dir.join(&entry.id).join(commit)),
        PluginSource::Folder { path } => Some(path.clone()),
        PluginSource::Builtin => None,
    }
}

/// The manifest at the root of `folder`, and the SHA-256 of its bytes. A
/// link, a file over 64 KiB or one that does not read is refused.
pub(crate) fn read_manifest(folder: &Path) -> Result<(Manifest, String), PluginError> {
    use std::io::Read as _;
    if !folder.is_dir() {
        return Err(PluginError::NotAFolder {
            path: folder.to_owned(),
        });
    }
    let path = folder.join(manifest::FILE_NAME);
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(PluginError::NoManifest { path });
        }
        Err(error) => return Err(PluginError::io(path.display().to_string())(error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(PluginError::Link { path });
    }
    if !metadata.is_file() {
        return Err(PluginError::NoManifest { path });
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .and_then(|file| file.take(manifest::MOST_BYTES + 1).read_to_end(&mut bytes))
        .map_err(PluginError::io(path.display().to_string()))?;
    if bytes.len() as u64 > manifest::MOST_BYTES {
        return Err(PluginError::TooLarge { path });
    }
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return Err(PluginError::Manifest {
            path,
            error: ManifestError::Syntax("it is not UTF-8 text".to_owned()),
        });
    };
    let manifest = Manifest::parse(text).map_err(|error| PluginError::Manifest {
        path: path.clone(),
        error,
    })?;
    Ok((manifest, sha256(&bytes)))
}

/// `bytes`' SHA-256, in lowercase hexadecimal, as an approval is written.
pub(crate) fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    muster_core::hex::encode(&sha2::Sha256::digest(bytes))
}

/// Whether `program` is a file anyone may run in a folder of the daemon's
/// `PATH`, which sessions get too.
pub(crate) fn on_path(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    let Some(paths) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&paths).any(|dir| {
        std::fs::metadata(dir.join(program))
            .is_ok_and(|found| found.is_file() && found.permissions().mode() & 0o111 != 0)
    })
}

#[cfg(test)]
mod tests;
