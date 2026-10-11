//! What to run for a session (`ARCHITECTURE.md` §7).
//!
//! Two harnesses so far: `shell`, the person's own shell in the workspace,
//! and `claude`, Claude Code. A harness says how to start, resume and hear
//! from its program; the daemon does the rest.

pub(crate) mod claude;
pub(crate) mod transcript;

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
    kind: Kind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Shell,
    Claude,
}

const SHELL: Harness = Harness {
    id: "shell",
    label: "Shell",
    kind: Kind::Shell,
};

const CLAUDE: Harness = Harness {
    id: "claude",
    label: "Claude Code",
    kind: Kind::Claude,
};

/// The harness called `id`, if Muster knows it.
pub(crate) fn find(id: &str) -> Option<Harness> {
    [SHELL, CLAUDE].into_iter().find(|harness| harness.id == id)
}

impl Harness {
    /// Whether the program reports its state through hooks, and so needs a
    /// session id and a hook token at launch.
    pub(crate) fn has_hooks(self) -> bool {
        self.kind == Kind::Claude
    }

    /// Whether it can take a first prompt on its command line.
    pub(crate) fn takes_prompt(self) -> bool {
        self.kind == Kind::Claude
    }

    /// Whether a session can be resumed after the daemon restarts.
    pub(crate) fn can_resume(self) -> bool {
        self.kind == Kind::Claude
    }
}

/// Where a session's hooks report to, and the token that names the session.
#[derive(Clone, Debug)]
pub(crate) struct HookEndpoint {
    pub(crate) socket: PathBuf,
    pub(crate) token: String,
}

/// Everything needed to start a session's program.
#[derive(Clone, Debug)]
pub(crate) struct Launch {
    pub(crate) session: SessionId,
    /// The harness's own session id: chosen by Muster for a new session,
    /// the saved one for a resumed session.
    pub(crate) agent_session: Option<String>,
    pub(crate) resume: bool,
    pub(crate) prompt: Option<String>,
    pub(crate) hooks: Option<HookEndpoint>,
    pub(crate) cwd: PathBuf,
    pub(crate) size: TermSize,
}

/// How to start `harness` for `launch`.
pub(crate) fn spawn_spec(harness: Harness, launch: Launch) -> SpawnSpec {
    let mut env = vec![
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ("MUSTER_SESSION".to_owned(), launch.session.to_string()),
    ];
    if let Some(hooks) = &launch.hooks {
        env.push((
            "MUSTER_HOOK_SOCKET".to_owned(),
            hooks.socket.to_string_lossy().into_owned(),
        ));
        env.push(("MUSTER_HOOK_TOKEN".to_owned(), hooks.token.clone()));
    }
    let (program, args) = match harness.kind {
        Kind::Shell => (
            std::env::var("SHELL")
                .ok()
                .filter(|shell| !shell.is_empty())
                .unwrap_or_else(|| "/bin/sh".to_owned()),
            Vec::new(),
        ),
        Kind::Claude => ("claude".to_owned(), claude::arguments(&launch)),
    };
    SpawnSpec {
        program,
        args,
        cwd: launch.cwd,
        env,
        env_remove: scrubbed(std::env::vars().map(|(key, _)| key)),
        size: launch.size,
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

/// A secret that names one session to the hook receiver (`ARC-11`).
pub(crate) fn new_token() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_harnesses_are_found() {
        assert_eq!(find("shell"), Some(SHELL));
        assert_eq!(find("claude"), Some(CLAUDE));
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
            "CLAUDE_CONFIG_DIR",
        ]
        .map(str::to_owned);
        assert_eq!(
            scrubbed(keys.into_iter()),
            ["CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "TERM_SESSION_ID"]
        );
    }

    #[test]
    fn tokens_are_long_random_and_distinct() {
        let (one, two) = (new_token().unwrap(), new_token().unwrap());
        assert_eq!(one.len(), 64);
        assert!(one.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(one, two);
    }

    #[test]
    fn hooks_reach_the_program_through_its_environment() {
        let spec = spawn_spec(
            CLAUDE,
            Launch {
                session: SessionId::new(),
                agent_session: Some("id".to_owned()),
                resume: false,
                prompt: None,
                hooks: Some(HookEndpoint {
                    socket: PathBuf::from("/run/hooks.sock"),
                    token: "secret".to_owned(),
                }),
                cwd: PathBuf::from("/work"),
                size: TermSize::default(),
            },
        );
        assert_eq!(spec.program, "claude");
        let value = |key: &str| {
            spec.env
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.as_str())
        };
        assert_eq!(value("MUSTER_HOOK_SOCKET"), Some("/run/hooks.sock"));
        assert_eq!(value("MUSTER_HOOK_TOKEN"), Some("secret"));
    }
}
