//! Whether Claude Code still has a session's transcript, which `--resume`
//! needs. Resuming without one ends in "No conversation found" and a dead
//! terminal, so such a session starts fresh instead.
//!
//! The transcript is `<projects>/<folder slug>/<session id>.jsonl`. Every
//! slug folder is searched rather than computing the slug, so its exact
//! rules do not matter. Adapted from Nebula's `registry.rs` (MIT, Copyright
//! (c) 2026 Cody Seibert; see NOTICE).

use std::path::{Path, PathBuf};

/// Where Claude Code keeps transcripts: `$CLAUDE_CONFIG_DIR/projects`, or
/// `~/.claude/projects`.
pub(crate) fn projects_dir() -> Option<PathBuf> {
    let var = |key| std::env::var_os(key).filter(|value| !value.is_empty());
    var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".claude")))
        .map(|config| config.join("projects"))
}

/// `Some(true)` if a transcript for `session_id` exists under `projects`,
/// `Some(false)` if not, and `None` if `projects` could not be read, which
/// is no verdict.
pub(crate) fn exists(projects: &Path, session_id: &str) -> Option<bool> {
    let file = format!("{session_id}.jsonl");
    // An id that is not one plain file name names no transcript.
    if Path::new(&file).file_name() != Some(std::ffi::OsStr::new(&file)) {
        return Some(false);
    }
    let slugs = std::fs::read_dir(projects).ok()?;
    Some(
        slugs
            .flatten()
            .any(|slug| slug.path().join(&file).is_file()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transcript_is_found_under_any_slug() {
        let dir = tempfile::tempdir().unwrap();
        let slug = dir.path().join("-Users-u-app");
        std::fs::create_dir(&slug).unwrap();
        std::fs::write(slug.join("abc.jsonl"), "{}\n").unwrap();
        assert_eq!(exists(dir.path(), "abc"), Some(true));
        assert_eq!(exists(dir.path(), "def"), Some(false));
    }

    #[test]
    fn an_unreadable_directory_is_no_verdict() {
        assert_eq!(exists(Path::new("/nonexistent/projects"), "abc"), None);
    }

    #[test]
    fn an_id_that_is_not_a_plain_name_has_no_transcript() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(exists(dir.path(), "../escape"), Some(false));
        assert_eq!(exists(dir.path(), "a/b"), Some(false));
    }
}
