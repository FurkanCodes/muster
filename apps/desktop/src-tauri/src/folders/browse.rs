//! The folder browser under the path field, standing in for the Mac's
//! folder panel in builds for the end-to-end tests (`super::PickedFolder::
//! Unavailable`), as the TUI's picker does (`app::picker`): a folder's
//! folders, git repositories marked; only the names of folders browsed to
//! (`ARC-24`). The webview's mock stands in for it in a browser.

use std::path::{Path, PathBuf};

use serde::Serialize;
use ts_rs::TS;

/// A folder inside the one listed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub(crate) struct Folder {
    pub name: String,
    /// Its whole path, as `AddProject` takes it.
    pub path: PathBuf,
    /// It is a git repository: it has a `.git` of its own.
    pub git: bool,
}

/// The folder listed, `~` taken as the home folder, and its folders by
/// name, ignoring case. Hidden ones are listed too: the browser shows them
/// once a `.` is typed, as the TUI's does.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub(crate) struct Folders {
    pub path: PathBuf,
    pub folders: Vec<Folder>,
}

/// Why a folder could not be listed, in words the person can act on.
#[derive(Debug, Serialize, thiserror::Error)]
#[error("Cannot read {path}: {reason}")]
pub(crate) struct FolderError {
    path: String,
    reason: String,
}

fn failure(path: &Path, reason: impl std::fmt::Display) -> FolderError {
    FolderError {
        path: path.display().to_string(),
        reason: reason.to_string(),
    }
}

/// A typed path: `~` is the home folder; anything else must be absolute,
/// as the app has no folder it was started in.
fn expand(typed: &str, home: Option<&Path>) -> Result<PathBuf, FolderError> {
    let typed = typed.trim();
    let path = match typed.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with('/') => {
            let home =
                home.ok_or_else(|| failure(Path::new(typed), "the home folder is unknown"))?;
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(typed),
    };
    if !path.is_absolute() {
        return Err(failure(&path, "start it with / or ~/"));
    }
    Ok(path)
}

/// The folders in `dir`. Files are left out, and so are names that are not
/// UTF-8, which a path typed into the field could not name, or that hold
/// control characters. Links to folders count.
fn read(dir: &Path, home: Option<&Path>) -> Result<Folders, FolderError> {
    let entries = std::fs::read_dir(dir).map_err(|error| failure(dir, error))?;
    let mut folders: Vec<Folder> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry
                .file_name()
                .into_string()
                .ok()
                .filter(|name| !name.contains(char::is_control))?;
            let path = entry.path();
            path.is_dir().then(|| Folder {
                git: !is_protected(&path, home) && path.join(".git").exists(),
                name,
                path,
            })
        })
        .collect();
    folders.sort_by_cached_key(|folder| (folder.name.to_lowercase(), folder.name.clone()));
    Ok(Folders {
        path: dir.to_owned(),
        folders,
    })
}

/// Whether looking inside `path` makes macOS ask the person whether the app
/// may read it: the home folder's Desktop, Documents and Downloads. Listing
/// the home folder must not raise three of those questions just to mark
/// repositories, so these are not looked into until opened.
fn is_protected(path: &Path, home: Option<&Path>) -> bool {
    cfg!(target_os = "macos")
        && home.is_some_and(|home| path.parent() == Some(home))
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "Desktop" | "Documents" | "Downloads"))
}

/// Lists the folders in the folder typed, off the webview's thread. A
/// folder it cannot read is refused, never shown as empty.
#[cfg(feature = "e2e")]
#[tauri::command]
pub(crate) async fn list_folders(path: String) -> Result<Folders, FolderError> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dir = expand(&path, home.as_deref())?;
    let listed = dir.clone();
    tauri::async_runtime::spawn_blocking(move || read(&listed, home.as_deref()))
        .await
        .map_err(|error| failure(&dir, error))?
}

#[cfg(test)]
mod tests {
    use super::*;

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    fn names(folders: &Folders) -> Vec<(&str, bool)> {
        folders
            .folders
            .iter()
            .map(|folder| (folder.name.as_str(), folder.git))
            .collect()
    }

    #[test]
    fn arc24_only_folders_are_listed_by_name_with_repositories_marked() -> Result {
        let dir = tempfile::tempdir()?;
        for folder in ["beta", "Alpha", "repo/.git", ".hidden", "evil\x1b[2J"] {
            std::fs::create_dir_all(dir.path().join(folder))?;
        }
        std::fs::create_dir(dir.path().join("worktree"))?;
        std::fs::write(dir.path().join("worktree/.git"), "gitdir: elsewhere")?;
        std::fs::write(dir.path().join("file.txt"), "not a folder")?;
        std::os::unix::fs::symlink(dir.path().join("beta"), dir.path().join("link"))?;

        let listed = read(dir.path(), None)?;
        assert_eq!(listed.path, dir.path());
        assert_eq!(
            names(&listed),
            [
                (".hidden", false),
                ("Alpha", false),
                ("beta", false),
                ("link", false),
                ("repo", true),
                ("worktree", true),
            ]
        );
        assert_eq!(listed.folders[1].path, dir.path().join("Alpha"));
        Ok(())
    }

    #[test]
    fn arc24_a_missing_folder_or_a_file_is_refused() -> Result {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("file");
        std::fs::write(&file, "")?;
        assert!(read(&file, None).is_err());
        let missing = read(&dir.path().join("missing"), None)
            .err()
            .ok_or("listed")?;
        assert!(missing.to_string().starts_with("Cannot read "));
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn arc24_a_folder_it_cannot_read_is_refused() -> Result {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("private");
        std::fs::create_dir(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000))?;
        let result = read(&path, None);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700))?;
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn arc24_home_is_tilde_and_a_relative_path_is_refused() -> Result {
        let home = Path::new("/Users/you");
        assert_eq!(expand("~", Some(home))?, home);
        assert_eq!(expand("~/", Some(home))?, home);
        assert_eq!(expand(" ~/code/ ", Some(home))?, home.join("code/"));
        assert_eq!(expand("/opt", Some(home))?, Path::new("/opt"));
        assert!(expand("code", Some(home)).is_err());
        assert!(expand("~other/code", Some(home)).is_err());
        assert!(expand("~/code", None).is_err());
        Ok(())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn arc24_documents_is_not_looked_into_to_mark_it() -> Result {
        let home = tempfile::tempdir()?;
        std::fs::create_dir_all(home.path().join("Documents/.git"))?;
        std::fs::create_dir_all(home.path().join("code/.git"))?;
        let listed = read(home.path(), Some(home.path()))?;
        assert_eq!(names(&listed), [("code", true), ("Documents", false)]);
        Ok(())
    }
}
