//! Runs the real `muster` binary as a daemon in its own directories.

// Each test file compiles its own copy of this module and uses a different part.
#![allow(dead_code)]

use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::Paths;
use muster_core::frame::{self, FrameDecoder};
use muster_core::protocol::{ClientFrame, Hello, ServerFrame};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use tempfile::TempDir;

pub(crate) mod agents;

pub(crate) const BINARY: &str = env!("CARGO_BIN_EXE_muster");

/// A scratch area: the daemon's runtime and data directories, and room for
/// test repositories.
#[derive(Debug)]
pub(crate) struct Sandbox {
    dir: TempDir,
    /// More environment for every process this sandbox starts.
    env: Vec<(String, String)>,
}

/// Stands in for Claude Code: records how it was started (its arguments,
/// then the hook socket and token it was given), then behaves like a shell
/// so tests can type into it. While the FAKE_CLAUDE_RESUME_FAILS flag file
/// exists, a resume exits at once, as a real one does when the conversation
/// is gone; with FAKE_CLAUDE_RESUME_FAILS_LATE, it says so and exits after
/// a second, long enough for a client to be watching. It announces its
/// start through the SessionStart hook, unless FAKE_CLAUDE_SILENT exists.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
dir="$FAKE_CLAUDE_DIR/$MUSTER_SESSION"
mkdir -p "$dir"
n=$(( $(ls "$dir" | grep -c '\.args$') + 1 ))
printf '%s\n%s\n' "$MUSTER_HOOK_SOCKET" "$MUSTER_HOOK_TOKEN" > "$dir/launch-$n.env"
for arg in "$@"; do printf '%s\0' "$arg"; done > "$dir/launch-$n.part"
mv "$dir/launch-$n.part" "$dir/launch-$n.args"
if [ "$1" = "--resume" ] && [ -f "$FAKE_CLAUDE_DIR/FAKE_CLAUDE_RESUME_FAILS" ]; then exit 1; fi
if [ "$1" = "--resume" ] && [ -f "$FAKE_CLAUDE_DIR/FAKE_CLAUDE_RESUME_FAILS_LATE" ]; then
  echo "resume-failing-$((40+2))"; sleep 1; exit 1
fi
# Announce the start as Claude Code does, unless told to stay silent, as
# Claude Code is while it asks whether to trust the folder.
id=""; previous=""
for arg in "$@"; do
  case "$previous" in --session-id|--resume) id="$arg" ;; esac
  previous="$arg"
done
if [ -n "$MUSTER_HOOK_TOKEN" ] && [ ! -f "$FAKE_CLAUDE_DIR/FAKE_CLAUDE_SILENT" ]; then
  printf '{"hook_event_name":"SessionStart","session_id":"%s","source":"startup"}' "$id" |
    curl -sS -m 3 --unix-socket "$MUSTER_HOOK_SOCKET" -H "Authorization: Bearer $MUSTER_HOOK_TOKEN" \
      -H "Content-Type: application/json" --data-binary @- http://muster/hooks/claude >/dev/null 2>&1
fi
exec /bin/sh
"#;

/// Stands in for the GitHub CLI: answers `issue list` and `issue view N`
/// from fixture files, records `issue close N` for an issue it has, keeps one pull request (#17) once one is created,
/// with what it is told about it, and records every call. While the GH_SLOW
/// flag file exists it takes two seconds to answer, like a slow network.
const FAKE_GH: &str = r#"#!/bin/sh
dir="$FAKE_GH_DIR"
printf '%s\n' "$*" >> "$dir/calls"
[ -f "$dir/GH_SLOW" ] && sleep 2
case "$1 $2" in
  "issue list") cat "$dir/issues.json" ;;
  "issue view")
    if [ -f "$dir/issue-$3.json" ]; then cat "$dir/issue-$3.json"
    else echo "GraphQL: Could not resolve to an Issue with the number of $3." >&2; exit 1; fi ;;
  "issue close")
    if [ ! -f "$dir/issue-$3.json" ]; then
      echo "GraphQL: Could not resolve to an Issue with the number of $3." >&2; exit 1; fi
    echo "$3" >> "$dir/closed" ;;
  "pr list") if [ -f "$dir/pr.json" ]; then cat "$dir/pr.json"; else echo "[]"; fi ;;
  "pr create")
    cat > "$dir/pr-body"
    echo '[{"number":17,"url":"https://github.com/o/app/pull/17"}]' > "$dir/pr.json"
    echo "https://github.com/o/app/pull/17" ;;
  "pr ready") echo "$3 $4" >> "$dir/ready" ;;
  "pr review") { cat; echo; echo "-----"; } >> "$dir/reviews" ;;
  "pr view")
    if [ -f "$dir/pr-state" ]; then printf '{"state":"%s"}' "$(cat "$dir/pr-state")"
    else echo '{"state":"OPEN"}'; fi ;;
  *) echo "unexpected gh call: $*" >&2; exit 1 ;;
esac
"#;

/// Stands in for the `docker` command: records every call and answers as
/// Docker does when it is not running, so runs stop for the person with
/// that reason. Tests against the real Docker do without it.
const FAKE_DOCKER: &str = r#"#!/bin/sh
printf '%s\n' "$*" >> "$FAKE_DOCKER_DIR/calls"
echo "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?" >&2
exit 1
"#;

impl Sandbox {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let agents = agents::AGENTS.map(|agent| (agent, agents::FAKE_AGENT));
        for (name, script) in [
            ("claude", FAKE_CLAUDE),
            ("gh", FAKE_GH),
            ("docker", FAKE_DOCKER),
        ]
        .into_iter()
        .chain(agents)
        {
            std::fs::write(bin.join(name), script).unwrap();
            std::fs::set_permissions(
                bin.join(name),
                std::os::unix::fs::PermissionsExt::from_mode(0o755),
            )
            .unwrap();
        }
        std::fs::create_dir_all(dir.path().join("fake-gh")).unwrap();
        std::fs::create_dir_all(dir.path().join("fake-docker")).unwrap();
        std::fs::create_dir_all(dir.path().join("fake-claude")).unwrap();
        std::fs::create_dir_all(dir.path().join("fake-agents")).unwrap();
        std::fs::create_dir_all(dir.path().join("claude-config/projects")).unwrap();
        Self {
            dir,
            env: Vec::new(),
        }
    }

    /// Uses the real `docker` instead of the stand-in, for tests against
    /// the real Docker.
    pub(crate) fn with_real_docker(self) -> Self {
        std::fs::remove_file(self.dir.path().join("bin/docker")).unwrap();
        self
    }

    /// The sandbox's own directory, for repositories and fixtures.
    pub(crate) fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Sets `key` for every process this sandbox starts.
    pub(crate) fn with_env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_owned(), value.to_owned()));
        self
    }

    /// A flag file the stand-in `claude` checks, such as
    /// `FAKE_CLAUDE_RESUME_FAILS`.
    pub(crate) fn fake_claude_flag(&self, name: &str) -> PathBuf {
        self.dir.path().join("fake-claude").join(name)
    }

    /// The stand-in `gh`'s directory: `issues.json`, `issue-<n>.json`, the
    /// `GH_SLOW` flag and the `calls` it recorded.
    pub(crate) fn fake_gh(&self) -> PathBuf {
        self.dir.path().join("fake-gh")
    }

    /// The calls the stand-in `docker` was given, one per line.
    pub(crate) fn docker_calls(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("fake-docker/calls")).unwrap_or_default()
    }

    /// Where Claude Code would keep its transcripts in this sandbox.
    pub(crate) fn claude_projects(&self) -> PathBuf {
        self.dir.path().join("claude-config/projects")
    }

    /// How the stand-in `claude` was started for `session`, oldest first.
    pub(crate) fn claude_launches(&self, session: muster_core::SessionId) -> Vec<ClaudeLaunch> {
        let dir = self
            .dir
            .path()
            .join("fake-claude")
            .join(session.to_string());
        (1..)
            .map_while(|n| {
                let args = std::fs::read(dir.join(format!("launch-{n}.args"))).ok()?;
                let env = std::fs::read_to_string(dir.join(format!("launch-{n}.env"))).ok()?;
                let mut env = env.lines().map(str::to_owned);
                Some(ClaudeLaunch {
                    args: args
                        .split(|&byte| byte == 0)
                        .filter(|arg| !arg.is_empty())
                        .map(|arg| String::from_utf8_lossy(arg).into_owned())
                        .collect(),
                    socket: env.next().unwrap_or_default(),
                    token: env.next().unwrap_or_default(),
                })
            })
            .collect()
    }

    /// Waits until the stand-in `claude` has been started `count` times for
    /// `session`, and returns those launches.
    pub(crate) fn wait_for_launches(
        &self,
        session: muster_core::SessionId,
        count: usize,
    ) -> Vec<ClaudeLaunch> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let launches = self.claude_launches(session);
            if launches.len() >= count {
                return launches;
            }
            assert!(
                Instant::now() < deadline,
                "only {} launches",
                launches.len()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub(crate) fn paths(&self) -> Paths {
        Paths {
            runtime_dir: self.dir.path().join("run"),
            data_dir: self.dir.path().join("data"),
            worktrees_dir: self.dir.path().join("worktrees"),
            config_dir: self.dir.path().join("config"),
        }
    }

    /// A git repository with one commit on `main`.
    pub(crate) fn repository(&self, name: &str) -> PathBuf {
        let root = self.dir.path().join("repos").join(name);
        std::fs::create_dir_all(&root).unwrap();
        git(&root, &["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(root.join("README.md"), "test\n").unwrap();
        git(&root, &["add", "README.md"]);
        git(&root, &["commit", "--quiet", "--message", "first"]);
        root.canonicalize().unwrap()
    }

    /// `muster` with its environment pointed at this sandbox.
    pub(crate) fn command(&self) -> Command {
        let paths = self.paths();
        let mut command = Command::new(BINARY);
        command
            .env("MUSTER_RUNTIME_DIR", &paths.runtime_dir)
            .env("MUSTER_DATA_DIR", &paths.data_dir)
            .env("MUSTER_WORKTREES_DIR", &paths.worktrees_dir)
            .env("MUSTER_CONFIG_DIR", &paths.config_dir)
            .env("MUSTER_LOG", "debug")
            // Sessions run a plain, predictable shell.
            .env("SHELL", "/bin/sh")
            // `claude` is the stand-in, recording into the sandbox.
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    self.dir.path().join("bin").display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            )
            .env("FAKE_CLAUDE_DIR", self.dir.path().join("fake-claude"))
            .env("FAKE_AGENT_DIR", self.dir.path().join("fake-agents"))
            // The person's own settings for the agents, which Muster adds to
            // rather than replaces, are none in the tests.
            .env_remove("GEMINI_CLI_SYSTEM_DEFAULTS_PATH")
            .env_remove("OPENCODE_CONFIG_CONTENT")
            .env("FAKE_GH_DIR", self.dir.path().join("fake-gh"))
            .env("FAKE_DOCKER_DIR", self.dir.path().join("fake-docker"))
            .env("CLAUDE_CONFIG_DIR", self.dir.path().join("claude-config"))
            // Runs never reach the person's own credential: not the
            // Keychain, and not one in the environment the tests run in.
            .env("MUSTER_KEYCHAIN", "off")
            .env("CLAUDE_CODE_OAUTH_TOKEN", "")
            .env("ANTHROPIC_API_KEY", "")
            // As if the daemon were started from inside an agent session,
            // which sessions must not inherit.
            .env("CLAUDECODE", "1");
        for (key, value) in GIT_ENV {
            command.env(key, value);
        }
        for (key, value) in &self.env {
            command.env(key, value);
        }
        command
    }

    /// Runs `muster` with `args` and waits for it. A command still running
    /// after ten seconds is killed and fails the test, so a regression shows
    /// up as a failure rather than a hung test run.
    pub(crate) fn run(&self, args: &[&str]) -> Output {
        let mut child = self
            .command()
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let output = child.wait_with_output().unwrap();
                panic!(
                    "`muster {}` was still running after 10 s; stderr:\n{}",
                    args.join(" "),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        child.wait_with_output().unwrap()
    }

    pub(crate) fn start(&self) -> Daemon {
        Daemon::start(self)
    }

    fn log(&self) -> PathBuf {
        self.dir.path().join("daemon.log")
    }
}

/// A running daemon, stopped when dropped so a failing test never leaks one.
#[derive(Debug)]
pub(crate) struct Daemon {
    child: Option<Child>,
    socket: PathBuf,
    log: PathBuf,
}

impl Daemon {
    fn start(sandbox: &Sandbox) -> Self {
        let log = sandbox.log();
        let child = sandbox
            .command()
            .arg("daemon")
            .stdout(Stdio::null())
            .stderr(File::create(&log).unwrap())
            .spawn()
            .unwrap();
        let daemon = Self {
            child: Some(child),
            socket: sandbox.paths().socket(),
            log,
        };
        daemon.wait_until_ready();
        daemon
    }

    fn wait_until_ready(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if answers_hello(&self.socket) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the daemon did not start; its log:\n{}", self.log());
    }

    pub(crate) fn pid(&self) -> u32 {
        self.child.as_ref().map_or(0, Child::id)
    }

    pub(crate) fn socket(&self) -> &Path {
        &self.socket
    }

    pub(crate) async fn connect(&self) -> Connection {
        Connection::connect(&self.socket, "muster e2e")
            .await
            .unwrap()
    }

    pub(crate) fn log(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// Sends SIGTERM and waits for the daemon to exit.
    pub(crate) fn stop(mut self) -> ExitStatus {
        self.terminate().expect("the daemon was running")
    }

    fn terminate(&mut self) -> Option<ExitStatus> {
        let mut child = self.child.take()?;
        let pid = Pid::from_raw(i32::try_from(child.id()).ok()?);
        let _ = kill(pid, Signal::SIGTERM);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        child.wait().ok()
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// The pid of the daemon answering on `socket`, if one does.
pub(crate) fn daemon_pid(socket: &Path) -> Option<u32> {
    let mut stream = UnixStream::connect(socket).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    let hello = frame::encode(&ClientFrame::Hello(Hello::current("e2e pid"))).ok()?;
    stream.write_all(&hello).ok()?;
    let (mut decoder, mut chunk) = (FrameDecoder::new(), [0; 4096]);
    loop {
        let read = stream.read(&mut chunk).ok().filter(|read| *read > 0)?;
        decoder.push(&chunk[..read]);
        if let Some(frame) = decoder.next_frame::<ServerFrame>().ok()? {
            return match frame {
                ServerFrame::Welcome(welcome) => Some(welcome.daemon_pid),
                _ => None,
            };
        }
    }
}

/// Whether a daemon answers a hello on `socket`.
///
/// Being able to connect is not enough: on macOS a socket cannot be created
/// close-on-exec in one step, so a process another test spawns at that moment
/// can inherit a listening socket and keep accepting connections nobody will
/// answer.
pub(crate) fn answers_hello(socket: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(socket) else {
        return false;
    };
    let timeout = Some(Duration::from_millis(500));
    let hello = frame::encode(&ClientFrame::Hello(Hello::current("e2e readiness"))).unwrap();
    if stream.set_read_timeout(timeout).is_err() || stream.write_all(&hello).is_err() {
        return false;
    }
    let (mut decoder, mut chunk) = (FrameDecoder::new(), [0; 4096]);
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return false,
            Ok(read) => decoder.push(&chunk[..read]),
        }
        match decoder.next_frame::<ServerFrame>() {
            Ok(Some(frame)) => return matches!(frame, ServerFrame::Welcome(_)),
            Ok(None) => continue,
            Err(_) => return false,
        }
    }
}

/// Keeps git away from the user's own configuration, hooks and signing.
const GIT_ENV: [(&str, &str); 6] = [
    ("GIT_CONFIG_GLOBAL", "/dev/null"),
    ("GIT_CONFIG_NOSYSTEM", "1"),
    ("GIT_AUTHOR_NAME", "Test"),
    ("GIT_AUTHOR_EMAIL", "test@example.com"),
    ("GIT_COMMITTER_NAME", "Test"),
    ("GIT_COMMITTER_EMAIL", "test@example.com"),
];

/// Runs git in `dir` and returns what it printed; panics if it fails.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .envs(GIT_ENV)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Waits for a pushed frame that `wanted` accepts, skipping the rest.
pub(crate) async fn expect_event<T>(
    connection: &mut Connection,
    mut wanted: impl FnMut(&ServerFrame) -> Option<T>,
) -> T {
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            match connection.next_event().await.unwrap() {
                Some(frame) => {
                    if let Some(found) = wanted(&frame) {
                        return found;
                    }
                }
                None => panic!("the daemon closed the connection"),
            }
        }
    })
    .await;
    found.expect("the expected frame arrived in time")
}

/// Collects a session's terminal output until it contains `marker`, and
/// returns it with the sequence number of the next byte.
pub(crate) async fn read_terminal(
    connection: &mut Connection,
    session: muster_core::SessionId,
    marker: &str,
) -> (String, u64) {
    let mut text = String::new();
    let mut next = 0;
    expect_event(connection, |frame| {
        match frame {
            ServerFrame::Replay {
                session: s,
                seq,
                data,
                ..
            }
            | ServerFrame::Output {
                session: s,
                seq,
                data,
            } if *s == session => {
                text.push_str(&String::from_utf8_lossy(data));
                next = seq + data.len() as u64;
            }
            ServerFrame::Detached { session: s, reason } if *s == session => {
                panic!("detached ({reason:?}) before {marker:?} appeared in {text:?}")
            }
            _ => {}
        }
        text.contains(marker).then_some(())
    })
    .await;
    (text, next)
}

/// One start of the stand-in `claude`.
#[derive(Debug)]
pub(crate) struct ClaudeLaunch {
    pub(crate) args: Vec<String>,
    pub(crate) socket: String,
    pub(crate) token: String,
}

impl ClaudeLaunch {
    /// The value after `flag`, if given.
    pub(crate) fn value(&self, flag: &str) -> Option<&str> {
        let at = self.args.iter().position(|arg| arg == flag)?;
        self.args.get(at + 1).map(String::as_str)
    }

    /// The command the `event` hook runs, from the `--settings` given.
    pub(crate) fn hook_command(&self, event: &str) -> String {
        let settings: serde_json::Value =
            serde_json::from_str(self.value("--settings").expect("--settings")).unwrap();
        settings["hooks"][event][0]["hooks"][0]["command"]
            .as_str()
            .unwrap_or_else(|| panic!("no {event} hook in {settings}"))
            .to_owned()
    }

    /// Runs the `event` hook exactly as Claude Code would: its command, in a
    /// shell, with `payload` on stdin and this launch's environment.
    pub(crate) fn fire(&self, event: &str, payload: &serde_json::Value) {
        self.fire_with_token(event, payload, &self.token);
    }

    /// As `fire`, but carrying `token` instead of this launch's own.
    pub(crate) fn fire_with_token(&self, event: &str, payload: &serde_json::Value, token: &str) {
        use std::io::Write as _;
        let mut child = Command::new("sh")
            .args(["-c", &self.hook_command(event)])
            .env("MUSTER_HOOK_SOCKET", &self.socket)
            .env("MUSTER_HOOK_TOKEN", token)
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(payload.to_string().as_bytes())
            .unwrap();
        assert!(
            child.wait().unwrap().success(),
            "a hook must never fail the agent"
        );
    }
}
