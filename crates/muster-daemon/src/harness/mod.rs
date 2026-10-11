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
use crate::runs::credentials::Secret;

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
    /// Whether it is a coding agent, rather than a shell.
    pub(crate) fn is_agent(self) -> bool {
        self.kind == Kind::Claude
    }

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

/// Where a session's program runs.
#[derive(Clone, Debug)]
pub(crate) enum Place {
    /// On the host, in this directory, as the person, with their sign-ins.
    Host(PathBuf),
    /// In a run's container, through `docker exec` with a terminal: the
    /// daemon's terminal drives docker's, which drives the program's
    /// (`ARCHITECTURE.md` §5). Claude Code there gets `secret`, through
    /// docker's environment, never its arguments (`PRD-06`).
    Container {
        docker: String,
        container: String,
        secret: Option<Secret>,
    },
}

impl Place {
    /// Whether hooks can reach the daemon from there: its socket is on the
    /// host, out of a container's reach.
    pub(crate) fn hears_hooks(&self) -> bool {
        matches!(self, Self::Host(_))
    }
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
    pub(crate) place: Place,
    pub(crate) size: TermSize,
}

/// Runs a session's program in a container: `$1` the session, `$2` the
/// harness, `$3` its agent session. It records its process, which the
/// program then becomes, so Muster can stop it there: stopping docker's
/// client stops nothing in the container. A program the session ran
/// before, still there after a daemon restart, is stopped first; a pid is
/// trusted only if that process carries the session in its environment,
/// as a pid can be reused. Stopping is what a closed terminal does, a
/// hangup (an interactive shell ignores a plain terminate), and a kill if
/// the program is still there after two seconds. Claude Code resumes its
/// conversation if the container has the transcript, and starts it under
/// that id otherwise; the container is the sandbox, so it asks no
/// permissions, as the building agent does.
pub(crate) const IN_CONTAINER: &str = r#"stop_session() {
  f="/tmp/muster/sessions/$1.pid"
  [ -s "$f" ] || return 0
  pid="$(cat "$f")"
  if tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep -qx "MUSTER_SESSION=$1"; then
    kill -HUP -- "-$pid" 2>/dev/null || kill -HUP "$pid" 2>/dev/null
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.2 2>/dev/null || sleep 1
    done
    if kill -0 "$pid" 2>/dev/null; then
      kill -KILL -- "-$pid" 2>/dev/null || kill -KILL "$pid" 2>/dev/null
    fi
  fi
  rm -f "$f"
}
"#;

/// Starts a session's program: [`IN_CONTAINER`], then this.
const START_IN_CONTAINER: &str = r#"stop_session "$1"
mkdir -p /tmp/muster/sessions && echo $$ > "/tmp/muster/sessions/$1.pid"
cd /work 2>/dev/null || cd
case "$2" in
  claude)
    if ls "$HOME"/.claude/projects/*/"$3".jsonl >/dev/null 2>&1; then
      exec claude --dangerously-skip-permissions --resume "$3"
    fi
    exec claude --dangerously-skip-permissions --session-id "$3" ;;
  *)
    command -v bash >/dev/null 2>&1 && exec bash -l
    exec sh -l ;;
esac
"#;

/// Stops a session's program in a container: `$1` the session.
pub(crate) const STOP_IN_CONTAINER: &str = r#"stop_session "$1""#;

/// How to start `harness` for `launch`.
pub(crate) fn spawn_spec(harness: Harness, launch: Launch) -> SpawnSpec {
    let env = vec![
        ("TERM".to_owned(), "xterm-256color".to_owned()),
        ("COLORTERM".to_owned(), "truecolor".to_owned()),
        ("MUSTER_SESSION".to_owned(), launch.session.to_string()),
    ];
    match &launch.place {
        Place::Host(cwd) => on_host(harness, &launch, cwd.clone(), env),
        Place::Container {
            docker,
            container,
            secret,
        } => in_container(harness, &launch, (docker, container), secret.as_ref(), env),
    }
}

fn on_host(
    harness: Harness,
    launch: &Launch,
    cwd: PathBuf,
    mut env: Vec<(String, String)>,
) -> SpawnSpec {
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
        Kind::Claude => ("claude".to_owned(), claude::arguments(launch)),
    };
    SpawnSpec {
        program,
        args,
        cwd,
        env,
        env_remove: scrubbed(std::env::vars().map(|(key, _)| key)),
        size: launch.size,
    }
}

/// `docker exec` with a terminal, running [`START_IN_CONTAINER`].
fn in_container(
    harness: Harness,
    launch: &Launch,
    (docker, container): (&str, &str),
    secret: Option<&Secret>,
    mut env: Vec<(String, String)>,
) -> SpawnSpec {
    if let Some(secret) = secret.filter(|_| harness.kind == Kind::Claude) {
        env.push((secret.variable.to_owned(), secret.value().to_owned()));
    }
    let mut args: Vec<String> = ["exec", "--interactive", "--tty"]
        .map(str::to_owned)
        .to_vec();
    // Values reach the program through docker's own environment.
    for (name, _) in &env {
        args.extend(["--env".to_owned(), name.clone()]);
    }
    let kind = match harness.kind {
        Kind::Claude => "claude",
        Kind::Shell => "shell",
    };
    args.extend([
        container.to_owned(),
        "sh".to_owned(),
        "-c".to_owned(),
        format!("{IN_CONTAINER}{START_IN_CONTAINER}"),
        "sh".to_owned(),
        launch.session.to_string(),
        kind.to_owned(),
        launch.agent_session.clone().unwrap_or_default(),
    ]);
    SpawnSpec {
        program: docker.to_owned(),
        args,
        // Docker's client needs no particular directory.
        cwd: PathBuf::from("/"),
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
mod in_container_tests;

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
                place: Place::Host(PathBuf::from("/work")),
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
