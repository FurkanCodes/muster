//! Where the work in two workspaces of a project meets (`PRODUCT.md` §6):
//! the files both change, and of those, the ones whose changes would
//! conflict were both merged, as git's own merge says without touching
//! either workspace. Only read: nothing changes anywhere (`PRD-14`).
//!
//! The daemon asks git and keeps what it found on each workspace
//! (`Workspace::clashes`); this module reads git's answers and puts them
//! together.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::ids::WorkspaceId;

/// The most files of one clash sent; [`Clash::files`] counts them all.
pub const MAX_SHOWN: usize = 20;

/// Another workspace of the project whose work changes files this one's
/// changes too.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Clash {
    /// The other workspace.
    pub with: WorkspaceId,
    /// How many files both change.
    pub files: u64,
    /// How many of them would conflict, were both merged.
    pub conflicts: u64,
    /// The first [`MAX_SHOWN`] of them, those that would conflict first.
    pub shown: Vec<ClashFile>,
}

/// A file both workspaces' work changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ClashFile {
    /// Where it is in the repository, as git names it.
    pub path: String,
    /// Whether merging both would conflict there.
    pub conflict: bool,
}

impl Clash {
    /// Whether merging both would conflict anywhere.
    pub const fn would_conflict(&self) -> bool {
        self.conflicts > 0
    }
}

/// The paths both `one` and `other` change: the same file, or a file in
/// one where the other changes something inside a folder of that name.
pub fn overlap(one: &BTreeSet<String>, other: &BTreeSet<String>) -> BTreeSet<String> {
    let mut shared: BTreeSet<String> = one.intersection(other).cloned().collect();
    for (paths, others) in [(one, other), (other, one)] {
        for path in paths {
            shared.extend(
                folders(path)
                    .filter(|folder| others.contains(*folder))
                    .map(str::to_owned),
            );
        }
    }
    shared
}

/// The folders `path` is in, innermost first: `a/b` and `a` for `a/b/c`.
fn folders(path: &str) -> impl Iterator<Item = &str> {
    path.match_indices('/').rev().map(|(at, _)| &path[..at])
}

/// Reads `git merge-tree --write-tree --name-only -z --no-messages`: the
/// paths git could not merge. A file git moved aside to make room for a
/// folder (`docs~<commit>`), `merged` being the two commits merged, is
/// named where it was.
pub fn parse_conflicts(out: &str, merged: [&str; 2]) -> BTreeSet<String> {
    out.split('\0')
        // The merged tree's id comes first.
        .skip(1)
        .filter(|path| !path.is_empty())
        .map(|path| {
            merged
                .iter()
                .find_map(|side| {
                    path.strip_suffix(*side)
                        .and_then(|rest| rest.strip_suffix('~'))
                })
                .unwrap_or(path)
                .to_owned()
        })
        .collect()
}

/// What two workspaces' work comes to where it meets, `shared` the paths
/// both change and `conflicted` those git could not merge; `None` where
/// they change nothing alike. A conflict in a file only one of them
/// changes is not theirs but between one and the branch the other left
/// from, and is not counted.
pub fn clash(
    with: WorkspaceId,
    shared: &BTreeSet<String>,
    conflicted: &BTreeSet<String>,
) -> Option<Clash> {
    if shared.is_empty() {
        return None;
    }
    let conflicts = |path: &str| {
        conflicted.contains(path)
            || folders(path).any(|folder| conflicted.contains(folder))
            || conflicted
                .iter()
                .any(|other| folders(other).any(|folder| folder == path))
    };
    let mut files: Vec<ClashFile> = shared
        .iter()
        .map(|path| ClashFile {
            path: path.clone(),
            conflict: conflicts(path),
        })
        .collect();
    files.sort_by_key(|file| !file.conflict);
    let conflicts = files.iter().filter(|file| file.conflict).count();
    let count = files.len();
    files.truncate(MAX_SHOWN);
    Some(Clash {
        with,
        files: u64::try_from(count).unwrap_or(u64::MAX),
        conflicts: u64::try_from(conflicts).unwrap_or(u64::MAX),
        shown: files,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(paths: &[&str]) -> BTreeSet<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    #[test]
    fn the_same_file_or_a_file_where_the_other_has_a_folder_overlaps() {
        let one = set(&["README.md", "docs", "docs/old.md", "src/a.rs"]);
        let other = set(&["README.md", "docs/new.md", "src/b.rs", "src/a.rs.orig"]);
        assert_eq!(overlap(&one, &other), set(&["README.md", "docs"]));
        assert_eq!(overlap(&other, &one), set(&["README.md", "docs"]));
        assert!(overlap(&set(&["src/a.rs"]), &set(&["src/b.rs"])).is_empty());
    }

    #[test]
    fn git_s_conflicts_are_read_with_a_file_moved_aside_named_where_it_was() {
        let one = "a".repeat(40);
        let other = "b".repeat(40);
        let out = format!("{}\0docs~{one}\0f.txt\0", "c".repeat(40));
        assert_eq!(
            parse_conflicts(&out, [&one, &other]),
            set(&["docs", "f.txt"])
        );
        // A clean merge names only its tree.
        assert!(parse_conflicts(&format!("{one}\0"), [&one, &other]).is_empty());
        // A name with a tilde of its own stays as it is.
        let out = format!("{}\0notes~draft\0", "c".repeat(40));
        assert_eq!(parse_conflicts(&out, [&one, &other]), set(&["notes~draft"]));
    }

    #[test]
    fn a_clash_lists_conflicts_first_and_only_those_in_files_both_change() {
        let with = WorkspaceId::new();
        let shared = set(&["README.md", "docs", "src/a.rs"]);
        // `lib.rs` conflicts with the branch the other left from, not
        // with the other's work.
        let conflicted = set(&["docs/new.md", "lib.rs", "src/a.rs"]);
        let found = clash(with, &shared, &conflicted).unwrap();
        assert_eq!(found.with, with);
        assert_eq!((found.files, found.conflicts), (3, 2));
        assert!(found.would_conflict());
        let shown: Vec<(&str, bool)> = found
            .shown
            .iter()
            .map(|file| (file.path.as_str(), file.conflict))
            .collect();
        assert_eq!(
            shown,
            [("docs", true), ("src/a.rs", true), ("README.md", false)]
        );
        assert_eq!(clash(with, &BTreeSet::new(), &conflicted), None);
    }

    #[test]
    fn only_so_many_files_are_sent_but_all_are_counted() {
        let shared: BTreeSet<String> = (0..50).map(|n| format!("f{n:02}")).collect();
        let conflicted = set(&["f49"]);
        let found = clash(WorkspaceId::new(), &shared, &conflicted).unwrap();
        assert_eq!((found.files, found.conflicts), (50, 1));
        assert_eq!(found.shown.len(), MAX_SHOWN);
        assert_eq!(found.shown[0].path, "f49");
    }
}
