//! A project's worktrees building into one folder (`PRODUCT.md` §6,
//! `Preferences::shared_builds`).
//!
//! Cargo builds each checkout into a `target` folder of its own, so every
//! worktree compiles and keeps all of its project's dependencies again:
//! gigabytes a worktree. Cargo reads a `.cargo/config.toml` in each folder
//! above the one it builds in, so Muster writes one in the folder holding a
//! project's worktrees (`<worktrees>/<project>/`), naming one folder beside
//! them, [`BUILDS`], for all their builds. Only Cargo reads it, so it is
//! written for every project, which also serves one that takes up Rust
//! later. The repository is never touched: the person's own checkout builds
//! as before, and a project's own `.cargo/config.toml` or `CARGO_TARGET_DIR`
//! still comes first, as Cargo takes the nearest. A config Muster did not
//! write is left alone, and so is the folder it names.
//!
//! Builds no longer shared are moved aside at once, so a build starting
//! meanwhile never writes into a folder being deleted, and deleted off the
//! caller's loop ([`remove`]); any left from a daemon that stopped first
//! are deleted as the next settles ([`leftovers`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The first line of the config Muster writes, by which it knows its own.
const MARK: &str = "# Written by Muster";

/// The builds' folder, beside the worktrees; Cargo reads a config's
/// relative path from the folder holding its `.cargo`.
pub(crate) const BUILDS: &str = ".target";

/// What a folder of builds moved aside to be deleted is called, then a
/// number.
const ASIDE: &str = ".target-removed-";

const CONFIG: &str = "# Written by Muster: the worktrees Muster makes for this project build\n\
# into one folder beside them, .target, so their dependencies are built\n\
# and kept once rather than once a worktree. Muster removes this file and\n\
# the builds as the project's last worktree goes, or as you turn it off on\n\
# Muster's Settings screen ([worktrees] shared_builds). A project's own\n\
# .cargo/config.toml, and CARGO_TARGET_DIR, still come first.\n\
[build]\n\
target-dir = \".target\"\n";

/// Numbers the folders moved aside, so no two are named alike.
static MOVED: AtomicU64 = AtomicU64::new(0);

fn config(group: &Path) -> PathBuf {
    group.join(".cargo").join("config.toml")
}

/// What `file` holds; `None` where there is none.
async fn written(file: &Path) -> std::io::Result<Option<String>> {
    match tokio::fs::read_to_string(file).await {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Has the worktrees in `group` build into one folder: Muster's config is
/// written there, unless the person has one of their own.
pub(crate) async fn share(group: &Path) -> std::io::Result<()> {
    let file = config(group);
    match written(&file).await? {
        Some(text) if text == CONFIG || !text.starts_with(MARK) => Ok(()),
        _ => {
            tokio::fs::create_dir_all(group.join(".cargo")).await?;
            tokio::fs::write(&file, CONFIG).await
        }
    }
}

/// Has the worktrees in `group` build each into its own folder again:
/// Muster's config goes, and its builds are moved aside; returns where, for
/// [`remove`]. Nothing changes where the config is not Muster's.
pub(crate) async fn stop_sharing(group: &Path) -> std::io::Result<Option<PathBuf>> {
    let file = config(group);
    if !written(&file)
        .await?
        .is_some_and(|text| text.starts_with(MARK))
    {
        return Ok(None);
    }
    tokio::fs::remove_file(&file).await?;
    // Only if nothing else is in it.
    let _ = tokio::fs::remove_dir(group.join(".cargo")).await;
    let builds = group.join(BUILDS);
    if !tokio::fs::try_exists(&builds).await? {
        return Ok(None);
    }
    let aside = group.join(format!(
        "{ASIDE}{}-{}",
        std::process::id(),
        MOVED.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::rename(&builds, &aside).await?;
    Ok(Some(aside))
}

/// The folders of builds moved aside in `group` and never deleted, as by a
/// daemon that stopped first.
pub(crate) async fn leftovers(group: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(group).await else {
        return found;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_name().to_string_lossy().starts_with(ASIDE) {
            found.push(entry.path());
        }
    }
    found
}

/// Deletes builds moved aside.
pub(crate) async fn remove(aside: PathBuf) {
    if let Err(error) = tokio::fs::remove_dir_all(&aside).await {
        tracing::warn!(%error, path = %aside.display(), "could not delete builds no longer shared");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_config_names_one_folder_beside_the_worktrees_and_goes_with_it() {
        let dir = tempfile::tempdir().unwrap();
        let group = dir.path();
        share(group).await.unwrap();
        let text = std::fs::read_to_string(config(group)).unwrap();
        assert!(text.starts_with(MARK) && text.ends_with("target-dir = \".target\"\n"));
        // Again changes nothing; an older one of Muster's is brought up to date.
        share(group).await.unwrap();
        std::fs::write(config(group), format!("{MARK}: older\n")).unwrap();
        share(group).await.unwrap();
        assert_eq!(std::fs::read_to_string(config(group)).unwrap(), CONFIG);

        std::fs::create_dir_all(group.join(BUILDS).join("debug")).unwrap();
        let aside = stop_sharing(group).await.unwrap().expect("moved aside");
        assert!(!config(group).exists() && !group.join(".cargo").exists());
        assert!(!group.join(BUILDS).exists() && aside.join("debug").exists());
        assert_eq!(leftovers(group).await, std::slice::from_ref(&aside));
        remove(aside).await;
        assert!(leftovers(group).await.is_empty());
        assert_eq!(stop_sharing(group).await.unwrap(), None, "nothing left");
    }

    #[tokio::test]
    async fn the_person_s_own_config_and_builds_are_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let group = dir.path();
        std::fs::create_dir_all(group.join(".cargo")).unwrap();
        let own = "[build]\ntarget-dir = \".target\"\n";
        std::fs::write(config(group), own).unwrap();
        std::fs::create_dir_all(group.join(BUILDS)).unwrap();
        share(group).await.unwrap();
        assert_eq!(std::fs::read_to_string(config(group)).unwrap(), own);
        assert_eq!(stop_sharing(group).await.unwrap(), None);
        assert!(config(group).exists() && group.join(BUILDS).exists());
    }
}
