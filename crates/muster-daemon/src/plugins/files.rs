//! The plugins' files under `<data>/plugins` (`ARCHITECTURE.md` §7a):
//! previews' clones in `.incoming`, each installed plugin in
//! `<id>/<commit>/`, every folder private to the person (0700). A linked
//! folder is the author's, and nothing here touches it.

use std::fs::{DirBuilder, Permissions};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
use std::path::{Path, PathBuf};

use muster_core::plugins::{PluginEntry, PluginSource};

/// Where previews' clones go, in the plugins' folder `dir`.
pub(crate) fn incoming(dir: &Path) -> PathBuf {
    dir.join(".incoming")
}

/// Makes `dir` if needed, private to the person.
pub(crate) fn private_dir(dir: &Path) -> std::io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    std::fs::set_permissions(dir, Permissions::from_mode(0o700))
}

/// Moves the folder `from` to `to`, which must not be there, making its
/// parent if needed; `to` is left private to the person.
pub(crate) fn place(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        private_dir(parent)?;
    }
    if to.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            format!("{} is there already", to.display()),
        ));
    }
    std::fs::rename(from, to)?;
    std::fs::set_permissions(to, Permissions::from_mode(0o700))
}

/// Removes the folder `path` and all in it; one already gone is no
/// failure.
pub(crate) fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        done => done,
    }
}

/// Removes the folder `path` off the async workers, logging a failure.
pub(crate) async fn remove_later(path: PathBuf) {
    let shown = path.display().to_string();
    match tokio::task::spawn_blocking(move || remove(&path)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::warn!(%error, folder = shown, "could not remove a plugin's folder");
        }
        Err(error) => {
            tracing::warn!(%error, folder = shown, "could not remove a plugin's folder");
        }
    }
}

/// Removes what the plugins' folder `dir` holds that no entry of
/// `entries` names, as the daemon starts: previews' clones, a commit an
/// update left, a plugin removed while the daemon was not running. Each
/// entry keeps its own folder (`<id>/`), and one from git only its
/// commit's folder in it. What it removed, for the log.
pub(crate) fn sweep(dir: &Path, entries: &[PluginEntry]) -> Vec<PathBuf> {
    let mut removed = Vec::new();
    let mut gone = |path: PathBuf| match remove(&path) {
        Ok(()) => removed.push(path),
        Err(error) => {
            tracing::warn!(%error, folder = %path.display(), "could not remove a plugin's folder");
        }
    };
    let Ok(listing) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    for item in listing.filter_map(Result::ok) {
        let path = item.path();
        let name = item.file_name().to_string_lossy().into_owned();
        if !item.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        if path == incoming(dir) {
            gone(path);
            continue;
        }
        if !muster_core::plugins::is_id(&name) {
            continue;
        }
        let Some(entry) = entries.iter().find(|entry| entry.id == name) else {
            gone(path);
            continue;
        };
        let commit = match &entry.source {
            PluginSource::Git { commit, .. } => Some(commit.as_str()),
            PluginSource::Folder { .. } | PluginSource::Builtin => None,
        };
        let Ok(inside) = std::fs::read_dir(&path) else {
            continue;
        };
        for item in inside.filter_map(Result::ok) {
            let name = item.file_name().to_string_lossy().into_owned();
            if muster_core::plugins::is_commit(&name) && Some(name.as_str()) != commit {
                gone(item.path());
            }
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, source: PluginSource) -> PluginEntry {
        PluginEntry {
            id: id.to_owned(),
            source,
            enabled: true,
            approved: None,
        }
    }

    #[test]
    fn what_no_entry_names_is_swept_away_as_the_daemon_starts() {
        let dir = tempfile::tempdir().unwrap();
        let (old, new) = ("a1".repeat(20), "b2".repeat(20));
        for folder in [
            ".incoming/abc".to_owned(),
            format!("kept/{old}"),
            format!("kept/{new}"),
            format!("removed/{new}"),
            "linked/state".to_owned(),
            "Not-An-Id/x".to_owned(),
        ] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        let entries = [
            entry(
                "kept",
                PluginSource::Git {
                    url: "https://h/k".to_owned(),
                    git_ref: None,
                    commit: new.clone(),
                },
            ),
            entry(
                "linked",
                PluginSource::Folder {
                    path: PathBuf::from("/elsewhere"),
                },
            ),
        ];
        let mut removed = sweep(dir.path(), &entries);
        removed.sort();
        assert_eq!(
            removed,
            [
                dir.path().join(".incoming"),
                dir.path().join("kept").join(&old),
                dir.path().join("removed"),
            ]
        );
        assert!(dir.path().join("kept").join(&new).is_dir());
        assert!(dir.path().join("linked/state").is_dir());
        // Not a plugin's: left alone.
        assert!(dir.path().join("Not-An-Id/x").is_dir());
    }

    #[test]
    fn a_placed_folder_is_private_and_never_placed_over_another() {
        use std::os::unix::fs::MetadataExt as _;
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from");
        std::fs::create_dir_all(&from).unwrap();
        let to = dir.path().join("plugins/zoe/commit");
        place(&from, &to).unwrap();
        assert!(!from.exists());
        for folder in [&to, &dir.path().join("plugins/zoe")] {
            assert_eq!(std::fs::metadata(folder).unwrap().mode() & 0o777, 0o700);
        }
        std::fs::create_dir_all(&from).unwrap();
        assert!(place(&from, &to).is_err());
        assert!(from.exists());
        remove(&to).unwrap();
        remove(&to).unwrap();
    }
}
