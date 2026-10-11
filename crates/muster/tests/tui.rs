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

#[test]
fn a_daemon_the_ui_started_is_reaped_when_it_stops_and_another_takes_over() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let socket = sandbox.paths().socket();
    let _stop = StopDaemon(socket.clone());

    let ui = Ui::start(&sandbox, &root);
    ui.wait_for("daemon 0.1.0");
    let first = daemon_pid(&socket).expect("the UI started a daemon");
    let pid = Pid::from_raw(i32::try_from(first).unwrap());
    kill(pid, Signal::SIGTERM).unwrap();

    // The UI waits for its child, so no zombie is left: signalling the pid
    // fails once it is gone, which a zombie's would not.
    let deadline = Instant::now() + Duration::from_secs(10);
    while kill(pid, None).is_ok() {
        assert!(
            Instant::now() < deadline,
            "daemon {first} lingers as a zombie"
        );
        std::thread::sleep(Duration::from_millis(25));
    }

    // The UI notices and starts a new daemon.
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match daemon_pid(&socket) {
            Some(second) if second != first => break,
            _ => assert!(Instant::now() < deadline, "no new daemon"),
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    ui.wait_for("daemon 0.1.0");
}

#[test]
fn the_wheel_scrolls_a_session_back_and_typing_returns_to_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");

    // n on the project: a shell in its own checkout, full screen.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\t");
    ui.keys(b"\r");
    ui.wait_for("Ctrl-] back");
    ui.keys(b"seq 1 300\r");
    ui.wait_for("300");

    // Two turns of the wheel, as the terminal reports them (SGR).
    ui.keys(b"\x1b[<64;20;10M");
    ui.keys(b"\x1b[<64;20;10M");
    ui.wait_for("6 lines back");
    // Typing returns to the newest output.
    ui.keys(b"echo back-$((6*7))\r");
    ui.wait_for("back-42");
    assert!(!ui.text().contains("lines back"), "{}", ui.text());
}

#[test]
fn a_run_is_handed_over_followed_and_sent_back_to_work_from_the_ui() {
    // Docker is a stand-in that is not running, so the run stops for the
    // person at once: enough to follow it and act on it.
    let sandbox = Sandbox::new().with_env("MUSTER_RUN_IMAGE", "muster-e2e:absent");
    let root = sandbox.repository("app");
    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"true\"]\n",
    )
    .unwrap();
    let gh = sandbox.fake_gh();
    let issue = serde_json::json!({
        "number": 42, "title": "Fix the login redirect",
        "url": "https://github.com/o/app/issues/42", "labels": [], "author": {"login": "someone"}
    });
    std::fs::write(
        gh.join("issues.json"),
        serde_json::json!([issue]).to_string(),
    )
    .unwrap();
    let mut detail = issue.clone();
    detail["body"] = serde_json::Value::from("After signing in, users land on /404.");
    std::fs::write(gh.join("issue-42.json"), detail.to_string()).unwrap();
    let _stop = StopDaemon(sandbox.paths().socket());

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");

    // ⇥ ⇥: the runs view, empty; n: the project's issues, for a run.
    ui.keys(b"\t\t");
    ui.wait_for("No runs yet");
    ui.keys(b"n");
    ui.wait_for("Fix the login redirect");
    ui.wait_for("⏎ hand to the autopilot");
    ui.keys(b"\r");

    // It stops for the person, with why and its timeline.
    ui.wait_for("▲ #42 needs you");
    ui.wait_for("Docker is not running");
    ui.wait_for("there is room to run");
    ui.wait_for("1 need you");

    // r: back to work with notes; it stops again at the same place.
    ui.keys(b"r");
    ui.wait_for("Back to work");
    ui.keys(b"Docker is up now");
    ui.keys(b"\r");
    ui.wait_for("Sent the run back to work.");
    ui.wait_for("you sent it back to work");

    // x, y: cancelled, and listed as ended.
    ui.keys(b"x");
    ui.wait_for("Cancel the run on #42?");
    ui.keys(b"y");
    ui.wait_for("✕ #42 cancelled");
    ui.wait_for("0 runs going");
    let shown = sandbox.run(&["run", "show", "42"]);
    let shown = String::from_utf8_lossy(&shown.stdout);
    assert!(shown.contains("stage         cancelled"), "{shown}");
    assert!(shown.contains("you sent it back to work"), "{shown}");
}
