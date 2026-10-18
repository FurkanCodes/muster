//! Stand-ins for the coding agents other than Claude Code: Codex, Gemini
//! CLI, `OpenCode` and Copilot CLI.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use super::Sandbox;

/// Stands in for each of them, by the name it was started as: records its
/// arguments and the environment Muster gave it (the hook socket and
/// token, Gemini CLI's defaults file, `OpenCode`'s config), then behaves like
/// a shell so tests can type into it.
pub(super) const FAKE_AGENT: &str = r#"#!/bin/sh
dir="$FAKE_AGENT_DIR/$(basename "$0")/$MUSTER_SESSION"
mkdir -p "$dir"
n=$(( $(ls "$dir" | grep -c '\.args$') + 1 ))
printf '%s\n%s\n%s\n%s\n' "$MUSTER_HOOK_SOCKET" "$MUSTER_HOOK_TOKEN" \
  "$GEMINI_CLI_SYSTEM_DEFAULTS_PATH" "$OPENCODE_CONFIG_CONTENT" > "$dir/launch-$n.env"
for arg in "$@"; do printf '%s\0' "$arg"; done > "$dir/launch-$n.part"
mv "$dir/launch-$n.part" "$dir/launch-$n.args"
exec /bin/sh
"#;

/// The agents the stand-in plays.
pub(super) const AGENTS: [&str; 4] = ["codex", "gemini", "opencode", "copilot"];

/// One start of a stand-in agent.
#[derive(Debug)]
pub(crate) struct AgentLaunch {
    pub(crate) args: Vec<String>,
    pub(crate) socket: String,
    pub(crate) token: String,
    /// `GEMINI_CLI_SYSTEM_DEFAULTS_PATH`, if set.
    pub(crate) gemini_defaults: Option<PathBuf>,
    /// `OPENCODE_CONFIG_CONTENT`, if set.
    pub(crate) opencode_config: Option<String>,
}

impl AgentLaunch {
    /// The value after `flag`, if given.
    pub(crate) fn value(&self, flag: &str) -> Option<&str> {
        let at = self.args.iter().position(|arg| arg == flag)?;
        self.args.get(at + 1).map(String::as_str)
    }

    /// Runs `command` as the agent would run a hook: in a shell, with
    /// `stdin` and this launch's environment, and `args` after it.
    pub(crate) fn run(&self, command: &str, args: &[&str], stdin: &str) {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .args(args)
            .env("MUSTER_HOOK_SOCKET", &self.socket)
            .env("MUSTER_HOOK_TOKEN", &self.token)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        assert!(
            child.wait().unwrap().success(),
            "a hook must never fail the agent"
        );
    }

    /// Posts `body` to the hook socket at `path` with `token`, as a plugin
    /// would.
    pub(crate) fn post(&self, path: &str, token: &str, body: &serde_json::Value) {
        let status = Command::new("curl")
            .args(["-sS", "-m", "3", "--unix-socket", &self.socket])
            .args(["-H", &format!("Authorization: Bearer {token}")])
            .args(["-H", "Content-Type: application/json"])
            .args(["--data-binary", &body.to_string()])
            .arg(format!("http://muster{path}"))
            .stdout(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }
}

impl Sandbox {
    /// Puts the stand-in on `PATH` as `name` too: a coding agent Muster does
    /// not know, which the person adds in their own settings.
    pub(crate) fn add_agent_program(&self, name: &str) {
        let path = self.root().join("bin").join(name);
        std::fs::write(&path, FAKE_AGENT).unwrap();
        std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
    }

    /// Writes Muster's own settings, the person's `settings.toml`.
    pub(crate) fn own_settings(&self, text: &str) {
        let dir = self.paths().config_dir;
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("settings.toml"), text).unwrap();
    }

    fn agent_dir(&self, agent: &str, session: muster_core::SessionId) -> PathBuf {
        self.root()
            .join("fake-agents")
            .join(agent)
            .join(session.to_string())
    }

    /// How the stand-in `agent` was started for `session`, oldest first.
    pub(crate) fn agent_launches(
        &self,
        agent: &str,
        session: muster_core::SessionId,
    ) -> Vec<AgentLaunch> {
        let dir = self.agent_dir(agent, session);
        (1..)
            .map_while(|n| {
                let args = std::fs::read(dir.join(format!("launch-{n}.args"))).ok()?;
                let env = std::fs::read_to_string(dir.join(format!("launch-{n}.env"))).ok()?;
                let mut env = env.lines().map(str::to_owned);
                let mut next = || env.next().unwrap_or_default();
                let (socket, token, defaults, config) = (next(), next(), next(), next());
                Some(AgentLaunch {
                    args: args
                        .split(|&byte| byte == 0)
                        .filter(|arg| !arg.is_empty())
                        .map(|arg| String::from_utf8_lossy(arg).into_owned())
                        .collect(),
                    socket,
                    token,
                    gemini_defaults: (!defaults.is_empty()).then(|| PathBuf::from(defaults)),
                    opencode_config: (!config.is_empty()).then_some(config),
                })
            })
            .collect()
    }

    /// Waits until the stand-in `agent` has been started `count` times for
    /// `session`, and returns those launches.
    pub(crate) fn wait_for_agent(
        &self,
        agent: &str,
        session: muster_core::SessionId,
        count: usize,
    ) -> Vec<AgentLaunch> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let launches = self.agent_launches(agent, session);
            if launches.len() >= count {
                return launches;
            }
            assert!(
                Instant::now() < deadline,
                "{agent} started only {} times",
                launches.len()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

/// A file's text, which must be there.
pub(crate) fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}
