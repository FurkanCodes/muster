//! End-to-end tests for the terminal UI (M1 step 9): the real `muster`
//! binary on a real pseudo-terminal, driven by keystrokes, read back through
//! a terminal emulator — as a person would use it.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use support::{Sandbox, daemon_pid};

const COLS: u16 = 120;
const ROWS: u16 = 36;

/// The UI running on a pseudo-terminal, with what it draws parsed into a
/// screen.
struct Ui {
    screen: Arc<Mutex<vt100::Parser>>,
    /// Bytes for the UI's terminal input: keystrokes, and the answers the
    /// stand-in terminal gives.
    input: std::sync::mpsc::Sender<Vec<u8>>,
    child: Box<dyn Child + Send + Sync>,
    /// Held so the terminal stays open.
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Ui {
    fn start(sandbox: &Sandbox, cwd: &Path) -> Self {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut command = CommandBuilder::new(support::BINARY);
        command.cwd(cwd);
        let template = sandbox.command();
        for (key, value) in template.get_envs() {
            if let Some(value) = value {
                command.env(key, value);
            }
        }
        command.env("TERM", "xterm-256color");
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);

        let screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let parsed = screen.clone();
        // Plays the person's terminal: draws what arrives and answers the
        // device-attributes question crossterm asks, without claiming the
        // kitty keyboard protocol.
        let (input, queue) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer_tx = input.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; 16 * 1024];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let chunk = &buffer[..read];
                if chunk.windows(3).any(|window| window == b"\x1b[c") {
                    let _ = writer_tx.send(b"\x1b[?6c".to_vec());
                }
                parsed.lock().unwrap().process(chunk);
            }
        });
        std::thread::spawn(move || {
            while let Ok(bytes) = queue.recv() {
                let _ = writer.write_all(&bytes);
                let _ = writer.flush();
            }
        });
        Self {
            screen,
            input,
            child,
            _master: pair.master,
        }
    }

    fn text(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    /// Waits until the screen shows `wanted`.
    fn wait_for(&self, wanted: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self.text().contains(wanted) {
            assert!(
                Instant::now() < deadline,
                "never saw {wanted:?}; the screen:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    fn keys(&mut self, bytes: &[u8]) {
        self.input.send(bytes.to_vec()).unwrap();
        // Let the UI read one burst at a time, as typing would.
        std::thread::sleep(Duration::from_millis(60));
    }

    fn wait_for_exit(&mut self) -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            assert!(Instant::now() < deadline, "the UI did not quit");
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

impl Drop for Ui {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Stops the daemon the UI started, whatever happens in the test.
struct StopDaemon(PathBuf);

impl Drop for StopDaemon {
    fn drop(&mut self) {
        if let Some(pid) = daemon_pid(&self.0).and_then(|pid| i32::try_from(pid).ok()) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}

#[test]
fn the_ui_starts_the_daemon_and_drives_a_session_through_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());

    // No daemon is running: the UI starts one.
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.wait_for("daemon 0.1.0");

    // a: add the repository the UI was started in.
    ui.keys(b"a");
    ui.wait_for("Add a project");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");

    // w: a worktree on a new branch.
    ui.keys(b"w");
    ui.wait_for("New worktree");
    ui.keys(b"ui-work");
    ui.keys(b"\r");
    ui.wait_for("⎇ ui-work");

    // n, Tab, Enter: a shell in it, which opens full screen.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\t");
    ui.keys(b"\r");
    ui.wait_for("Ctrl-] back");

    // Typing reaches the shell, and its output comes back.
    ui.keys(b"echo ui-$((6*7))\r");
    ui.wait_for("ui-42");

    // Ctrl-] goes back to the list, where the session is listed.
    ui.keys(&[0x1d]);
    ui.wait_for("Sessions");
    ui.wait_for("Shell");

    // q quits the UI; the daemon and the session keep running.
    ui.keys(b"q");
    assert!(ui.wait_for_exit());
    assert!(daemon_pid(&sandbox.paths().socket()).is_some());
    let status = sandbox.run(&["status"]);
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("1 session"), "{stdout}");
    assert!(stdout.contains("Shell"), "{stdout}");
    assert!(!stdout.contains("exited"), "{stdout}");
}
