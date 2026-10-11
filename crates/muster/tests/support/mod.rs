//! Runs the real `muster` binary as a daemon in its own directories.

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

pub(crate) const BINARY: &str = env!("CARGO_BIN_EXE_muster");

/// A scratch area: the daemon's runtime and data directories, and room for
/// test repositories.
#[derive(Debug)]
pub(crate) struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    pub(crate) fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    pub(crate) fn paths(&self) -> Paths {
        Paths {
            runtime_dir: self.dir.path().join("run"),
            data_dir: self.dir.path().join("data"),
        }
    }

    /// A directory that looks like a git repository.
    pub(crate) fn repository(&self, name: &str) -> PathBuf {
        let root = self.dir.path().join("repos").join(name);
        std::fs::create_dir_all(root.join(".git")).unwrap();
        root.canonicalize().unwrap()
    }

    /// `muster` with its environment pointed at this sandbox.
    pub(crate) fn command(&self) -> Command {
        let paths = self.paths();
        let mut command = Command::new(BINARY);
        command
            .env("MUSTER_RUNTIME_DIR", &paths.runtime_dir)
            .env("MUSTER_DATA_DIR", &paths.data_dir)
            .env("MUSTER_LOG", "debug");
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
