//! What to run for a session (`ARCHITECTURE.md` §7).
//!
//! Only the built-in `shell` harness exists so far: the person's own shell in
//! the workspace. Agent harnesses, described as data, arrive with Claude Code.

use std::path::PathBuf;

use muster_core::SessionId;
use muster_core::protocol::TermSize;

use crate::pty::SpawnSpec;

/// A harness Muster can start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Harness {
    pub(crate) id: &'static str,
    /// The default title of its sessions.
    pub(crate) label: &'static str,
}

const SHELL: Harness = Harness {
    id: "shell",
    label: "Shell",
};

/// The harness called `id`, if Muster knows it.
pub(crate) fn find(id: &str) -> Option<Harness> {
    [SHELL].into_iter().find(|harness| harness.id == id)
}

/// How to start `harness` for `session` in `cwd`.
pub(crate) fn spawn_spec(
    harness: Harness,
    session: SessionId,
    cwd: PathBuf,
    size: TermSize,
) -> SpawnSpec {
    debug_assert_eq!(harness, SHELL);
    let program = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())
        .unwrap_or_else(|| "/bin/sh".to_owned());
    SpawnSpec {
        program,
        args: Vec::new(),
        cwd,
        env: vec![
            ("TERM".to_owned(), "xterm-256color".to_owned()),
            ("COLORTERM".to_owned(), "truecolor".to_owned()),
            ("MUSTER_SESSION".to_owned(), session.to_string()),
        ],
        env_remove: scrubbed(std::env::vars().map(|(key, _)| key)),
        size,
    }
}

/// The daemon's variables a session must not inherit: those that tell an
/// agent it is running inside another agent's session, and those naming the
/// terminal tab the daemon happened to be started from.
fn scrubbed(keys: impl Iterator<Item = String>) -> Vec<String> {
    keys.filter(|key| {
        key == "CLAUDECODE"
            || key.starts_with("CLAUDE_CODE_")
            || key == "TERM_SESSION_ID"
            || key == "ITERM_SESSION_ID"
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_harnesses_are_found() {
        assert_eq!(find("shell"), Some(SHELL));
        assert_eq!(find("nonsense"), None);
    }

    #[test]
    fn parent_agent_and_terminal_variables_are_scrubbed() {
        let keys = [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "TERM_SESSION_ID",
            "PATH",
            "HOME",
            "MUSTER_RUNTIME_DIR",
        ]
        .map(str::to_owned);
        assert_eq!(
            scrubbed(keys.into_iter()),
            ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "TERM_SESSION_ID"]
        );
    }
}
