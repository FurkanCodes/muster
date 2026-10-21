//! The real daemon in folders of its own, and a pretend webview the link
//! sends to: what the link's end-to-end tests need (`ARCHITECTURE.md` §13).
//!
//! The `muster` binary is the core workspace's, built once per test run
//! (`MUSTER_TEST_BIN` names one already built). Every daemon gets a runtime
//! folder of its own under /tmp, short enough for its socket, and the rest
//! in a temporary folder; none ever reaches the person's own daemon.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

use muster_core::protocol::{ClientFrame, Hello, ServerFrame, Snapshot};
use muster_core::{Paths, Project, ProjectId, Session, SessionId, Workspace, WorkspaceId};
use muster_desktop::contract::{Derived, LinkState, StateMessage, frame};
use muster_desktop::link::{FrameSink, StateSink};
use tempfile::TempDir;
use tokio::sync::mpsc;

/// How long a test waits for what it expects before failing.
pub(crate) const WAIT: Duration = Duration::from_secs(15);

/// The `muster` binary under test.
pub(crate) fn muster() -> &'static Path {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT.get_or_init(|| {
        if let Some(named) = std::env::var_os("MUSTER_TEST_BIN") {
            return PathBuf::from(named);
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let status = Command::new(env!("CARGO"))
            .args(["build", "--locked", "--quiet", "-p", "muster"])
            .current_dir(&root)
            .status()
            .expect("cargo runs");
        assert!(status.success(), "building muster failed");
        root.join("target/debug/muster")
            .canonicalize()
            .expect("muster was built")
    })
}

/// Stands in for Codex: a shell, whatever it is started with, so a test
/// types into it what Codex would write (its title asking the person).
const FAKE_CODEX: &str = "#!/bin/sh\nexec /bin/sh\n";

/// A daemon's folders and the programs it finds.
#[derive(Debug)]
pub(crate) struct Sandbox {
    /// The runtime folder, under /tmp so its socket's path stays short.
    runtime: TempDir,
    /// Data, settings, worktrees, repositories and stand-in programs.
    dir: TempDir,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        let runtime = tempfile::Builder::new()
            .prefix("mdl")
            .tempdir_in("/tmp")
            .expect("a runtime folder under /tmp");
        let dir = tempfile::tempdir().expect("a temporary folder");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).expect("bin");
        std::fs::write(bin.join("codex"), FAKE_CODEX).expect("the stand-in codex");
        std::fs::set_permissions(
            bin.join("codex"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .expect("runnable");
        Self { runtime, dir }
    }

    pub(crate) fn paths(&self) -> Paths {
        Paths {
            runtime_dir: self.runtime.path().to_path_buf(),
            data_dir: self.dir.path().join("data"),
            worktrees_dir: self.dir.path().join("worktrees"),
            config_dir: self.dir.path().join("config"),
        }
    }

    /// `muster` with its environment pointed at this sandbox: its own
    /// folders, a plain shell, the stand-in `codex`, no notch, no Keychain,
    /// git away from the person's settings, and not inside a session.
    pub(crate) fn command(&self) -> Command {
        let paths = self.paths();
        let mut command = Command::new(muster());
        command
            .env("MUSTER_RUNTIME_DIR", &paths.runtime_dir)
            .env("MUSTER_DATA_DIR", &paths.data_dir)
            .env("MUSTER_WORKTREES_DIR", &paths.worktrees_dir)
            .env("MUSTER_CONFIG_DIR", &paths.config_dir)
            .env("SHELL", "/bin/sh")
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin:/usr/sbin:/sbin",
                    self.dir.path().join("bin").display()
                ),
            )
            .env("MUSTER_NOTCH", "off")
            .env("MUSTER_KEYCHAIN", "off")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("MUSTER_SESSION");
        command
    }

    /// The daemon, run here as a child of the test.
    pub(crate) fn start(&self) -> Daemon {
        let log = std::fs::File::create(self.dir.path().join("daemon.log")).expect("a log");
        let child = self
            .command()
            .arg("daemon")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log)
            .spawn()
            .expect("the daemon starts");
        let daemon = Daemon {
            child: Some(child),
            sandbox_log: self.dir.path().join("daemon.log"),
        };
        daemon.wait_until_ready(&self.paths().socket());
        daemon
    }

    /// Stops a daemon started apart from the test (`--detach`).
    pub(crate) fn stop_detached(&self) {
        let _ = self
            .command()
            .args(["daemon", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

    /// A git repository with one commit on `main`.
    pub(crate) fn repository(&self, name: &str) -> PathBuf {
        let root = self.dir.path().join("repos").join(name);
        std::fs::create_dir_all(&root).expect("the repository's folder");
        let git = |args: &[&str]| {
            let status = Command::new("git")
                .args(args)
                .current_dir(&root)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .stdout(Stdio::null())
                .status()
                .expect("git runs");
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "--quiet", "--initial-branch", "main"]);
        std::fs::write(root.join("README.md"), "test\n").expect("a file");
        git(&["add", "README.md"]);
        git(&["commit", "--quiet", "--message", "first"]);
        root.canonicalize().expect("the repository")
    }
}

/// A daemon the test started, stopped when dropped.
#[derive(Debug)]
pub(crate) struct Daemon {
    child: Option<Child>,
    sandbox_log: PathBuf,
}

impl Daemon {
    fn wait_until_ready(&self, socket: &Path) {
        let started = std::time::Instant::now();
        while started.elapsed() < WAIT {
            if answers_hello(socket) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!(
            "the daemon did not start; its log:\n{}",
            std::fs::read_to_string(&self.sandbox_log).unwrap_or_default()
        );
    }

    /// Stops it, as the person's `muster daemon stop` would (SIGTERM).
    pub(crate) fn stop(mut self) {
        self.terminate();
    }

    fn terminate(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        if let Ok(pid) = i32::try_from(child.id()) {
            let _ = nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(pid),
                nix::sys::signal::Signal::SIGTERM,
            );
        }
        let started = std::time::Instant::now();
        while started.elapsed() < WAIT {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// Whether a daemon answers a hello on `socket`. Being able to connect is
/// not enough: on macOS a socket cannot be made close-on-exec in one step,
/// so a program another test starts at that moment can inherit a listening
/// socket and take connections nobody answers.
fn answers_hello(socket: &Path) -> bool {
    use std::io::{Read, Write};
    let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) else {
        return false;
    };
    let hello = muster_core::frame::encode(&ClientFrame::Hello(Hello::current("link tests")))
        .expect("a hello");
    if stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .is_err()
        || stream.write_all(&hello).is_err()
    {
        return false;
    }
    let (mut decoder, mut chunk) = (muster_core::frame::FrameDecoder::new(), [0; 4096]);
    loop {
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return false,
            Ok(read) => decoder.push(&chunk[..read]),
        }
        match decoder.next_frame::<ServerFrame>() {
            Ok(Some(frame)) => return matches!(frame, ServerFrame::Welcome(_)),
            Ok(None) => {}
            Err(_) => return false,
        }
    }
}

/// The webview's state channel, as a test reads it.
#[derive(Debug)]
pub(crate) struct States(pub(crate) mpsc::UnboundedSender<StateMessage>);

impl StateSink for States {
    fn send(&self, message: StateMessage) {
        let _ = self.0.send(message);
    }
}

/// A terminal's channel, as a test reads it.
#[derive(Debug)]
pub(crate) struct Frames(pub(crate) mpsc::UnboundedSender<Vec<u8>>);

impl FrameSink for Frames {
    fn send(&self, frame: Vec<u8>) -> bool {
        self.0.send(frame).is_ok()
    }
}

/// The state as the webview holds it: patches applied as `state/daemon.ts`
/// applies them.
#[derive(Debug)]
pub(crate) struct Webview {
    messages: mpsc::UnboundedReceiver<StateMessage>,
    pub(crate) link: Option<LinkState>,
    pub(crate) projects: HashMap<ProjectId, Project>,
    pub(crate) workspaces: HashMap<WorkspaceId, Workspace>,
    pub(crate) sessions: HashMap<SessionId, Session>,
    pub(crate) derived: Derived,
    /// Patches so far, and how many of them were resets.
    pub(crate) patches: usize,
    pub(crate) resets: usize,
    /// Every link state told, in order.
    pub(crate) links: Vec<LinkState>,
}

impl Webview {
    /// A webview and the sink the link sends it to.
    pub(crate) fn new() -> (Self, States) {
        let (sender, messages) = mpsc::unbounded_channel();
        let webview = Self {
            messages,
            link: None,
            projects: HashMap::new(),
            workspaces: HashMap::new(),
            sessions: HashMap::new(),
            derived: Derived::default(),
            patches: 0,
            resets: 0,
            links: Vec::new(),
        };
        (webview, States(sender))
    }

    fn apply(&mut self, message: StateMessage) {
        match message {
            StateMessage::Link { link } => {
                self.links.push(link.clone());
                self.link = Some(link);
            }
            StateMessage::Patch { patch } => {
                self.patches += 1;
                if patch.reset {
                    self.resets += 1;
                    self.projects.clear();
                    self.workspaces.clear();
                    self.sessions.clear();
                }
                for project in patch.projects {
                    self.projects.insert(project.id, project);
                }
                for workspace in patch.workspaces {
                    self.workspaces.insert(workspace.id, workspace);
                }
                for session in patch.sessions {
                    self.sessions.insert(session.id, session);
                }
                for id in patch.gone.projects {
                    self.projects.remove(&id);
                }
                for id in patch.gone.workspaces {
                    self.workspaces.remove(&id);
                }
                for id in patch.gone.sessions {
                    self.sessions.remove(&id);
                }
                self.derived = patch.derived;
            }
            StateMessage::Reveal { .. } => {}
        }
    }

    /// Takes what the link sent until `wanted` says the state is as
    /// expected; fails after [`WAIT`], saying what it waited for.
    pub(crate) async fn until(&mut self, what: &str, wanted: impl Fn(&Self) -> bool) {
        let outcome = tokio::time::timeout(WAIT, async {
            while !wanted(self) {
                match self.messages.recv().await {
                    Some(message) => self.apply(message),
                    None => panic!("the link ended while waiting for {what}"),
                }
            }
        })
        .await;
        assert!(
            outcome.is_ok(),
            "waited for {what}; the webview holds {:#?}",
            (&self.link, &self.sessions, &self.derived)
        );
    }

    /// Whether the link is up.
    pub(crate) fn up(&self) -> bool {
        matches!(self.link, Some(LinkState::Up { .. }))
    }

    /// Takes whatever came, without waiting.
    pub(crate) fn take_waiting(&mut self) -> usize {
        let mut taken = 0;
        while let Ok(message) = self.messages.try_recv() {
            self.apply(message);
            taken += 1;
        }
        taken
    }
}

/// A terminal as the webview's xterm would show its text.
#[derive(Debug)]
pub(crate) struct Screen {
    frames: mpsc::UnboundedReceiver<Vec<u8>>,
    /// Every byte written since the last reset, as text.
    pub(crate) text: String,
    /// The kinds of the frames so far, in order.
    pub(crate) kinds: Vec<u8>,
    /// The place after the last byte.
    pub(crate) next: u64,
}

impl Screen {
    /// A screen and the sink the link streams it to.
    pub(crate) fn new() -> (Self, Frames) {
        let (sender, frames) = mpsc::unbounded_channel();
        let screen = Self {
            frames,
            text: String::new(),
            kinds: Vec::new(),
            next: 0,
        };
        (screen, Frames(sender))
    }

    fn apply(&mut self, bytes: &[u8]) {
        let kind = bytes[0];
        let seq = u64::from_le_bytes(bytes[1..frame::HEADER].try_into().expect("a header"));
        let payload = &bytes[frame::HEADER..];
        self.kinds.push(kind);
        match kind {
            frame::REPLAY_RESET => {
                self.text.clear();
                self.text.push_str(&String::from_utf8_lossy(payload));
                self.next = seq + payload.len() as u64;
            }
            frame::REPLAY | frame::OUTPUT => {
                assert_eq!(
                    seq, self.next,
                    "each byte once, in order, after the first replay"
                );
                self.text.push_str(&String::from_utf8_lossy(payload));
                self.next = seq + payload.len() as u64;
            }
            _ => {}
        }
    }

    /// Takes frames until the text holds `marker`.
    pub(crate) async fn until_shows(&mut self, marker: &str) {
        let waited = tokio::time::timeout(WAIT, async {
            while !self.text.contains(marker) {
                match self.frames.recv().await {
                    Some(bytes) => {
                        let bytes_so_far = self.kinds.iter().any(|&kind| kind != frame::EVENT);
                        if !bytes_so_far && bytes[0] != frame::EVENT {
                            // The first bytes start wherever the daemon's
                            // ring does, or where the screen said it was.
                            self.next = u64::from_le_bytes(
                                bytes[1..frame::HEADER].try_into().expect("a header"),
                            );
                        }
                        self.apply(&bytes);
                    }
                    None => panic!("the stream ended before {marker:?}"),
                }
            }
        })
        .await;
        assert!(
            waited.is_ok(),
            "waited for {marker:?}; the terminal shows {:?}",
            self.text
        );
    }
}

/// The daemon's state as a client of its own sees it, to check against.
pub(crate) async fn snapshot(paths: &Paths) -> Snapshot {
    let mut connection = muster_client::Connection::connect(&paths.socket(), "link tests")
        .await
        .expect("connects");
    connection.subscribe().await.expect("a snapshot")
}
