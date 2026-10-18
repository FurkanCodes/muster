//! What a workspace's work has changed (`PRODUCT.md` §6): the files not
//! yet committed, and the branch's commits since it left the branch it is
//! to merge into, each file with the lines it adds and removes. The daemon
//! asks git; this reads what git prints, with no IO (`ARC-01`).
//!
//! Paths, commit subjects and diffs come from the repository, so they are
//! text from outside: a subject is put on one line as it is read, a diff
//! is cleaned of control characters ([`FileDiff::cleaned`]), and a path,
//! which names the file to the daemon again, is kept as git gave it and
//! cleaned where it is shown.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::text::{multi_line, one_line};

/// The first protocol that knows a workspace's changes
/// ([`Workspace::changes`](crate::Workspace::changes), the
/// `WorkspaceChanges` and `FileDiff` commands): they are asked only of a
/// daemon that agreed to it (`ARC-09`).
pub const CHANGES_PROTOCOL: u32 = 24;

/// The most commits listed; the rest are counted.
pub const MAX_COMMITS: usize = 50;

/// The most of one file's diff sent, in bytes; what is longer is cut at a
/// line's end, and says so.
pub const MAX_DIFF: usize = 512 * 1024;

/// The most of an untracked file read to count its lines; a longer one is
/// counted as a binary file is, without lines.
pub const MAX_COUNTED: usize = 1024 * 1024;

/// How far into a file git looks for a NUL byte to call it binary.
const BINARY_PROBE: usize = 8000;

/// Which part of a workspace's change a file is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangePart {
    /// Not yet committed: staged, unstaged and untracked, against the last
    /// commit.
    Uncommitted,
    /// What the branch's commits changed since it left its base.
    Branch,
}

/// How a file changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    /// A file that became a link, or the other way round.
    TypeChanged,
    /// Left with conflicts by a merge or rebase.
    Conflicted,
    /// New, and not yet added to git.
    Untracked,
}

impl ChangeKind {
    /// Its mark in a list, as git's own letters: `M`, `A`, `D`, `R`, `?`.
    pub fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Copied => 'C',
            Self::TypeChanged => 'T',
            Self::Conflicted => 'U',
            Self::Untracked => '?',
        }
    }

    /// The kind git's `--name-status` letter stands for; a letter it may
    /// add one day reads as modified.
    fn from_status(status: &str) -> Self {
        match status.chars().next() {
            Some('A') => Self::Added,
            Some('D') => Self::Deleted,
            Some('R') => Self::Renamed,
            Some('C') => Self::Copied,
            Some('T') => Self::TypeChanged,
            Some('U') => Self::Conflicted,
            _ => Self::Modified,
        }
    }
}

/// The lines a change adds and removes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineCounts {
    pub added: u64,
    pub removed: u64,
}

/// One file's change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// Where it is, from the top of the repository, as git gave it.
    pub path: String,
    /// Where it was, for a renamed or copied file.
    #[serde(default)]
    pub from: Option<String>,
    pub kind: ChangeKind,
    /// The lines it adds and removes; `None` for a binary file, or one too
    /// large to count.
    #[serde(default)]
    pub lines: Option<LineCounts>,
}

/// A commit on the branch.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Commit {
    /// Its short id.
    pub id: String,
    /// The first line of its message, on one line.
    pub subject: String,
}

/// Everything a workspace's work has changed, as git said when asked.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Changes {
    /// What the branch is measured against, such as `origin/main`; `None`
    /// where none can be found, which leaves only what is not committed.
    pub base: Option<String>,
    /// Not yet committed, by path.
    pub uncommitted: Vec<FileChange>,
    /// The branch's commits since it left its base, newest first, at most
    /// [`MAX_COMMITS`].
    pub commits: Vec<Commit>,
    /// How many more commits there are than listed.
    #[serde(default)]
    pub more_commits: u64,
    /// What those commits changed together, by path.
    pub branch: Vec<FileChange>,
}

impl Changes {
    /// The files in `part`.
    pub fn part(&self, part: ChangePart) -> &[FileChange] {
        match part {
            ChangePart::Uncommitted => &self.uncommitted,
            ChangePart::Branch => &self.branch,
        }
    }

    /// The file at `path` in `part`, if it is changed there.
    pub fn find(&self, part: ChangePart, path: &str) -> Option<&FileChange> {
        self.part(part).iter().find(|file| file.path == path)
    }

    /// What it all comes to: the files changed in either part, and the
    /// lines both add and remove; `None` when nothing is changed.
    pub fn summary(&self) -> Option<ChangeSummary> {
        let files: BTreeSet<&str> = self
            .uncommitted
            .iter()
            .chain(&self.branch)
            .map(|file| file.path.as_str())
            .collect();
        if files.is_empty() {
            return None;
        }
        let lines = self
            .uncommitted
            .iter()
            .chain(&self.branch)
            .filter_map(|file| file.lines)
            .fold(LineCounts::default(), |sum, lines| LineCounts {
                added: sum.added + lines.added,
                removed: sum.removed + lines.removed,
            });
        Some(ChangeSummary {
            files: u64::try_from(files.len()).unwrap_or(u64::MAX),
            added: lines.added,
            removed: lines.removed,
        })
    }
}

/// What a workspace's change comes to, shown on its line in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeSummary {
    /// Files changed, committed on the branch or not.
    pub files: u64,
    pub added: u64,
    pub removed: u64,
}

impl ChangeSummary {
    /// `+120 −30`; a binary file alone changes no lines, so `1 file`.
    pub fn words(&self) -> String {
        match (self.added, self.removed) {
            (0, 0) if self.files == 1 => "1 file".to_owned(),
            (0, 0) => format!("{} files", self.files),
            (added, 0) => format!("+{added}"),
            (0, removed) => format!("−{removed}"),
            (added, removed) => format!("+{added} −{removed}"),
        }
    }
}

/// One file's diff, as git prints it, cleaned.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileDiff {
    pub text: String,
    /// It was longer than [`MAX_DIFF`], and was cut at a line's end.
    #[serde(default)]
    pub cut: bool,
}

impl FileDiff {
    /// What git printed (with `cut`, as much of it as was read), cleaned
    /// of control characters but tabs and line breaks, so nothing in a
    /// file can reach a terminal as an escape sequence.
    pub fn cleaned(raw: &[u8], cut: bool) -> Self {
        let mut text = multi_line(&String::from_utf8_lossy(raw));
        if cut {
            // A part line, perhaps a part character, ends what was read.
            match text.rfind('\n') {
                Some(end) => text.truncate(end + 1),
                None => text.clear(),
            }
        }
        Self { text, cut }
    }
}

/// Reads `git diff --name-status -z` and `git diff --numstat -z` of the
/// same change into its files, by path.
pub fn file_changes(name_status: &str, numstat: &str) -> Vec<FileChange> {
    let counts = parse_numstat(numstat);
    let mut files: Vec<FileChange> = parse_name_status(name_status)
        .into_iter()
        .map(|(kind, path, from)| {
            let lines = counts
                .iter()
                .find(|count| count.path == path)
                .and_then(|count| count.lines);
            FileChange {
                path,
                from,
                kind,
                lines,
            }
        })
        .collect();
    files.sort_by(|one, other| one.path.cmp(&other.path));
    files
}

/// A file git does not track yet, with its lines counted from
/// `contents` (`None` where it could not be read): none for a binary file
/// or one larger than [`MAX_COUNTED`], whose contents are `None` too.
pub fn untracked(path: String, contents: Option<&[u8]>) -> FileChange {
    let lines = contents.and_then(|bytes| {
        let probe = &bytes[..bytes.len().min(BINARY_PROBE)];
        if probe.contains(&0) {
            return None;
        }
        #[expect(
            clippy::naive_bytecount,
            reason = "a file of at most MAX_COUNTED bytes, counted once: a crate for it would not earn its place (ARC-17)"
        )]
        let breaks = bytes.iter().filter(|byte| **byte == b'\n').count();
        let unfinished = usize::from(bytes.last().is_some_and(|last| *last != b'\n'));
        Some(LineCounts {
            added: u64::try_from(breaks + unfinished).unwrap_or(u64::MAX),
            removed: 0,
        })
    });
    FileChange {
        path,
        from: None,
        kind: ChangeKind::Untracked,
        lines,
    }
}

/// Reads NUL-separated paths, as `git ls-files -z` prints them.
pub fn parse_paths(out: &str) -> Vec<String> {
    out.split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Reads `git log -z --format=%h%x1f%s`: each commit's short id and
/// subject, the subject put on one line.
pub fn parse_commits(out: &str) -> Vec<Commit> {
    out.split('\0')
        .map(|entry| entry.trim_start_matches('\n'))
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            let (id, subject) = entry.split_once('\x1f').unwrap_or((entry, ""));
            Commit {
                id: one_line(id),
                subject: one_line(subject),
            }
        })
        .collect()
}

/// One file's line counts from `--numstat`.
#[derive(Debug, PartialEq, Eq)]
struct NumStat {
    path: String,
    lines: Option<LineCounts>,
}

/// Reads `git diff --numstat -z`: `added\tremoved\tpath\0`, or for a rename
/// `added\tremoved\t\0from\0to\0`; a binary file's counts are `-`.
fn parse_numstat(out: &str) -> Vec<NumStat> {
    let mut fields = out.split('\0');
    let mut stats = Vec::new();
    while let Some(field) = fields.next() {
        let mut parts = field.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let path = if path.is_empty() {
            // A rename: where it was, then where it is.
            let _from = fields.next();
            match fields.next() {
                Some(to) => to.to_owned(),
                None => break,
            }
        } else {
            path.to_owned()
        };
        let lines = match (added.parse(), removed.parse()) {
            (Ok(added), Ok(removed)) => Some(LineCounts { added, removed }),
            _ => None,
        };
        stats.push(NumStat { path, lines });
    }
    stats
}

/// Reads `git diff --name-status -z`: `M\0path\0`, or for a rename or a
/// copy `R100\0from\0to\0`.
fn parse_name_status(out: &str) -> Vec<(ChangeKind, String, Option<String>)> {
    let mut fields = out.split('\0');
    let mut found = Vec::new();
    while let Some(status) = fields.next() {
        if status.is_empty() {
            continue;
        }
        let kind = ChangeKind::from_status(status);
        let Some(path) = fields.next() else {
            break;
        };
        if matches!(kind, ChangeKind::Renamed | ChangeKind::Copied) {
            let Some(to) = fields.next() else {
                break;
            };
            found.push((kind, to.to_owned(), Some(path.to_owned())));
        } else {
            found.push((kind, path.to_owned(), None));
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(added: u64, removed: u64) -> LineCounts {
        LineCounts { added, removed }
    }

    #[test]
    fn files_read_with_their_kinds_and_lines_by_path() {
        let name_status =
            "M\0src/view.rs\0R087\0old name.rs\0new name.rs\0A\0logo.png\0D\0gone.rs\0";
        let numstat = "12\t3\tsrc/view.rs\x004\t1\t\0old name.rs\0new name.rs\0-\t-\tlogo.png\x000\t20\tgone.rs\0";
        let files = file_changes(name_status, numstat);
        assert_eq!(
            files,
            vec![
                FileChange {
                    path: "gone.rs".to_owned(),
                    from: None,
                    kind: ChangeKind::Deleted,
                    lines: Some(lines(0, 20)),
                },
                FileChange {
                    path: "logo.png".to_owned(),
                    from: None,
                    kind: ChangeKind::Added,
                    lines: None,
                },
                FileChange {
                    path: "new name.rs".to_owned(),
                    from: Some("old name.rs".to_owned()),
                    kind: ChangeKind::Renamed,
                    lines: Some(lines(4, 1)),
                },
                FileChange {
                    path: "src/view.rs".to_owned(),
                    from: None,
                    kind: ChangeKind::Modified,
                    lines: Some(lines(12, 3)),
                },
            ]
        );
    }

    #[test]
    fn nothing_changed_reads_as_no_files_and_a_cut_short_listing_loses_only_its_end() {
        assert!(file_changes("", "").is_empty());
        let files = file_changes("M\0a.rs\0R100\0b.rs", "1\t1\ta.rs\0");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a.rs");
    }

    #[test]
    fn an_unknown_status_letter_reads_as_modified() {
        assert_eq!(
            file_changes("X\0odd\0U\0both.rs\0T\0link\0", "")
                .iter()
                .map(|file| file.kind)
                .collect::<Vec<_>>(),
            // By path: both.rs, link, odd.
            [
                ChangeKind::Conflicted,
                ChangeKind::TypeChanged,
                ChangeKind::Modified
            ]
        );
    }

    #[test]
    fn untracked_files_count_their_lines_but_a_binary_one_has_none() {
        let text = untracked("notes.md".to_owned(), Some(b"one\ntwo\nthree"));
        assert_eq!(text.kind, ChangeKind::Untracked);
        assert_eq!(text.lines, Some(lines(3, 0)));
        assert_eq!(
            untracked("a".to_owned(), Some(b"one\n")).lines,
            Some(lines(1, 0))
        );
        assert_eq!(
            untracked("e".to_owned(), Some(b"")).lines,
            Some(lines(0, 0))
        );
        assert_eq!(untracked("bin".to_owned(), Some(b"PK\0\x03")).lines, None);
        assert_eq!(untracked("big".to_owned(), None).lines, None);
    }

    #[test]
    fn commits_read_with_their_subjects_on_one_line() {
        let out = "a1b2c3d\x1fFix the login\0\ne4f5a6b\x1fAdd\tthe \x1b]0;x\x07page\0";
        assert_eq!(
            parse_commits(out),
            vec![
                Commit {
                    id: "a1b2c3d".to_owned(),
                    subject: "Fix the login".to_owned(),
                },
                Commit {
                    id: "e4f5a6b".to_owned(),
                    subject: "Add the ]0;xpage".to_owned(),
                },
            ]
        );
        assert!(parse_commits("").is_empty());
        assert_eq!(parse_paths("a\0b c\0"), ["a", "b c"]);
    }

    #[test]
    fn a_summary_counts_each_file_once_and_every_line() {
        let file = |path: &str, lines| FileChange {
            path: path.to_owned(),
            from: None,
            kind: ChangeKind::Modified,
            lines,
        };
        let changes = Changes {
            base: Some("origin/main".to_owned()),
            uncommitted: vec![file("a.rs", Some(lines(2, 1))), file("logo.png", None)],
            commits: Vec::new(),
            more_commits: 0,
            branch: vec![
                file("a.rs", Some(lines(10, 0))),
                file("b.rs", Some(lines(0, 4))),
            ],
        };
        let summary = changes.summary().unwrap();
        assert_eq!(
            summary,
            ChangeSummary {
                files: 3,
                added: 12,
                removed: 5
            }
        );
        assert_eq!(summary.words(), "+12 −5");
        assert_eq!(Changes::default().summary(), None);
        assert_eq!(
            changes.find(ChangePart::Branch, "b.rs").map(|f| f.lines),
            Some(Some(lines(0, 4)))
        );
        assert!(changes.find(ChangePart::Uncommitted, "b.rs").is_none());
    }

    #[test]
    fn a_summary_says_what_it_can_in_few_words() {
        let words = |files, added, removed| {
            ChangeSummary {
                files,
                added,
                removed,
            }
            .words()
        };
        assert_eq!(words(1, 0, 0), "1 file");
        assert_eq!(words(2, 0, 0), "2 files");
        assert_eq!(words(1, 7, 0), "+7");
        assert_eq!(words(1, 0, 7), "−7");
    }

    #[test]
    fn a_diff_loses_its_escapes_and_a_cut_one_ends_at_a_whole_line() {
        let raw = b"diff --git a/x b/x\r\n+\x1b[2Jcleared\n+half";
        let whole = FileDiff::cleaned(raw, false);
        assert_eq!(whole.text, "diff --git a/x b/x\n+[2Jcleared\n+half");
        assert!(!whole.cut);
        let cut = FileDiff::cleaned(raw, true);
        assert_eq!(cut.text, "diff --git a/x b/x\n+[2Jcleared\n");
        assert!(cut.cut);
        assert_eq!(
            FileDiff::cleaned("é".as_bytes()[..1].as_ref(), true).text,
            ""
        );
    }
}
