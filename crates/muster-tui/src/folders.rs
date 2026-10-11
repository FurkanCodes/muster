//! Listing the folders in a folder, for picking a project. The runtime
//! calls it on a blocking thread and hands the result to the app, which
//! does no IO itself.

use std::path::Path;

/// A folder inside the one listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Folder {
    pub(crate) name: String,
    /// It is a git repository: it has a `.git` of its own.
    pub(crate) git: bool,
}

/// The folders in `dir`, by name, ignoring case; links to folders count.
/// Files are left out, and so are names that are not UTF-8, which a path
/// typed into the dialog could not name anyway, or that hold control
/// characters, which drawn would reach the person's terminal as commands.
pub(crate) fn list(dir: &Path) -> Result<Vec<Folder>, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("cannot read it: {error}"))?;
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
                git: !is_protected(&path) && path.join(".git").exists(),
                name,
            })
        })
        .collect();
    folders.sort_by_cached_key(|folder| (folder.name.to_lowercase(), folder.name.clone()));
    Ok(folders)
}

/// Whether looking inside `path` makes macOS ask the person whether their
/// terminal may read it: the home folder's Desktop, Documents and Downloads.
/// Listing the home folder must not raise three of those questions just to
/// mark repositories, so these are not looked into until opened.
fn is_protected(path: &Path) -> bool {
    if !cfg!(target_os = "macos") {
        return false;
    }
    let Some(home) = std::env::var_os("HOME") else {
        return false;
    };
    path.parent() == Some(Path::new(&home))
        && path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| matches!(name, "Desktop" | "Documents" | "Downloads"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_folders_are_listed_by_name_with_repositories_marked() {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["beta", "Alpha", "repo/.git", ".hidden", "evil\x1b[2J"] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
        }
        std::fs::write(dir.path().join("file.txt"), "not a folder").unwrap();
        std::os::unix::fs::symlink(dir.path().join("beta"), dir.path().join("link")).unwrap();

        let folder = |name: &str, git| Folder {
            name: name.to_owned(),
            git,
        };
        assert_eq!(
            list(dir.path()).unwrap(),
            [
                folder(".hidden", false),
                folder("Alpha", false),
                folder("beta", false),
                folder("link", false),
                folder("repo", true),
            ]
        );
        assert!(list(&dir.path().join("missing")).is_err());
    }
}
