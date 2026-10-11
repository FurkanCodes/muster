//! What to run for a session (`ARCHITECTURE.md` §7).
//!
//! Two harnesses so far: `shell`, the person's own shell in the workspace,
//! and `claude`, Claude Code. A harness says how to start, resume and hear
//! from its program; the daemon does the rest.

pub(crate) mod claude;
pub(crate) mod claude_stream;
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

    /// Whether the program suspends itself on Ctrl+Z and waits to be
    /// continued (see `PtyHandle::resume_if_suspended`).
    pub(crate) fn suspends_itself(self) -> bool {
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

/// Variables a Claude Code session, or the app hosting it, sets for what it
/// runs: they say a program runs inside that session (one inheriting
/// `CLAUDE_CODE_CHILD_SESSION` stops saving its transcript, so it could not
/// be resumed), reach for its sockets, or carry its account and effort.
/// Seen in Claude Code 2.1.296 and its desktop app.
const PARENT_SESSION: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_PREVIEW_CLASSIFIER_FLOOR",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_HOST_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_DESKTOP_APP_VERSION",
    "CLAUDE_CODE_ORGANIZATION_UUID",
    "CLAUDE_CODE_ACCOUNT_UUID",
    "CLAUDE_CODE_USER_EMAIL",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDE_CODE_DISABLE_CRON",
    "CLAUDE_CODE_TERMINAL_MCP_TOOLS",
    "CLAUDE_CODE_EAGER_FLUSH",
    "CLAUDE_CODE_REPORT_FINDINGS",
    "CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES",
    "CLAUDE_CODE_ENABLE_ASK_USER_QUESTION_TOOL",
    "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING",
];

/// Prefixes of the same, for families of them.
const PARENT_SESSION_PREFIXES: &[&str] = &["CLAUDE_AGENT_SDK_", "CLAUDE_CODE_SDK_"];

/// The daemon's variables a session must not inherit: those of a Claude
/// Code session the daemon happened to be started from, and those naming
/// the terminal tab it was started in. The person's own settings stay,
/// `CLAUDE_CODE_USE_BEDROCK` or `CLAUDE_CODE_OAUTH_TOKEN` among them: a
/// session runs as if started in their terminal (`PRD-06`).
fn scrubbed(keys: impl Iterator<Item = String>) -> Vec<String> {
    keys.filter(|key| {
        PARENT_SESSION.contains(&key.as_str())
            || PARENT_SESSION_PREFIXES
                .iter()
                .any(|prefix| key.starts_with(prefix))
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
    fn parent_session_and_terminal_variables_are_scrubbed_and_settings_kept() {
        let keys = [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_PID",
            "CLAUDE_EFFORT",
            "CLAUDE_AGENT_SDK_VERSION",
            "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
            "TERM_SESSION_ID",
            // The person's own settings.
            "PATH",
            "HOME",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "ANTHROPIC_API_KEY",
        ]
        .map(str::to_owned);
        assert_eq!(
            scrubbed(keys.into_iter()),
            [
                "CLAUDECODE",
                "CLAUDE_CODE_ENTRYPOINT",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_PID",
                "CLAUDE_EFFORT",
                "CLAUDE_AGENT_SDK_VERSION",
                "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
                "TERM_SESSION_ID",
            ]
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
