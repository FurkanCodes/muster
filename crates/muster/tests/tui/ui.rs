//! The UI under test: the real `muster` binary on a pseudo-terminal, what
//! it draws parsed into a screen, keystrokes and mouse reports typed into
//! it.

// Each UI test file compiles its own copy of this module and uses a different part.
#![allow(
    dead_code,
    reason = "every UI test crate includes it and uses only some of it; #[expect] would fail in the rest"
)]

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use portable_pty::{Child, CommandBuilder, PtySize, native_pty_system};

use crate::support::{self, Sandbox, daemon_pid};

pub(crate) const COLS: u16 = 120;
pub(crate) const ROWS: u16 = 36;
/// The stand-in terminal's background, as it says it when asked.
pub(crate) const BACKGROUND: &str = "rgb:2b2b/2b2b/3c3c";

/// The UI running on a pseudo-terminal, with what it draws parsed into a
/// screen.
pub(crate) struct Ui {
    screen: Arc<Mutex<vt100::Parser>>,
    /// Everything the UI wrote to its terminal, as written.
    written: Arc<Mutex<Vec<u8>>>,
    /// Bytes for the UI's terminal input: keystrokes, and the answers the
    /// stand-in terminal gives.
    input: std::sync::mpsc::Sender<Vec<u8>>,
    child: Box<dyn Child + Send + Sync>,
    /// Held so the terminal stays open.
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Ui {
    pub(crate) fn start(sandbox: &Sandbox, cwd: &Path) -> Self {
        Self::start_with(sandbox, cwd, &[])
    }

    /// Starts the UI with `env` set as well.
    pub(crate) fn start_with(sandbox: &Sandbox, cwd: &Path, env: &[(&str, &str)]) -> Self {
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
        // Agents' marks as text, whatever terminal runs the tests.
        command.env("MUSTER_LOGOS", "text");
        for (key, value) in env {
            command.env(key, value);
        }
        let child = pair.slave.spawn_command(command).unwrap();
        drop(pair.slave);

        let screen = Arc::new(Mutex::new(vt100::Parser::new(ROWS, COLS, 0)));
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let parsed = screen.clone();
        let written = Arc::new(Mutex::new(Vec::new()));
        let kept = written.clone();
        // Plays the person's terminal: draws what arrives and answers the
        // device-attributes question crossterm asks, without claiming the
        // kitty keyboard protocol, and the question about its background,
        // with `BACKGROUND`, as terminals answer them: in order.
        let (input, queue) = std::sync::mpsc::channel::<Vec<u8>>();
        let writer_tx = input.clone();
        std::thread::spawn(move || {
            let mut buffer = [0; 16 * 1024];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 {
                    break;
                }
                let chunk = &buffer[..read];
                if chunk.windows(6).any(|window| window == b"\x1b]11;?") {
                    let _ = writer_tx.send(format!("\x1b]11;{BACKGROUND}\x1b\\").into_bytes());
                }
                if chunk.windows(3).any(|window| window == b"\x1b[c") {
                    let _ = writer_tx.send(b"\x1b[?6c".to_vec());
                }
                parsed.lock().unwrap().process(chunk);
                kept.lock().unwrap().extend_from_slice(chunk);
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
            written,
            input,
            child,
            _master: pair.master,
        }
    }

    pub(crate) fn text(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    /// The UI's process id.
    pub(crate) fn pid(&self) -> u32 {
        self.child.process_id().expect("the UI's pid")
    }

    /// Waits until the screen shows `wanted`.
    pub(crate) fn wait_for(&self, wanted: &str) {
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

    /// Waits until the screen no longer shows `gone`.
    pub(crate) fn wait_without(&self, gone: &str) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while self.text().contains(gone) {
            assert!(
                Instant::now() < deadline,
                "still showing {gone:?}; the screen:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Waits until the UI has written `wanted` to its terminal, escape
    /// sequences and all.
    pub(crate) fn wait_for_written(&self, wanted: &[u8]) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !self
            .written
            .lock()
            .unwrap()
            .windows(wanted.len())
            .any(|window| window == wanted)
        {
            assert!(
                Instant::now() < deadline,
                "never wrote {:?}",
                String::from_utf8_lossy(wanted)
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// Where `text` first starts on screen, as (column, row).
    pub(crate) fn position_of(&self, text: &str) -> Option<(u16, u16)> {
        let parser = self.screen.lock().unwrap();
        let screen = parser.screen();
        (0..ROWS).find_map(|row| {
            let line = screen.contents_between(row, 0, row, COLS);
            let at = line.find(text)?;
            Some((u16::try_from(line[..at].chars().count()).unwrap(), row))
        })
    }

    /// The background of the cell where `text` first starts on screen.
    pub(crate) fn background_of(&self, text: &str) -> Option<vt100::Color> {
        let parser = self.screen.lock().unwrap();
        let screen = parser.screen();
        (0..ROWS).find_map(|row| {
            let line = screen.contents_between(row, 0, row, COLS);
            let at = line.find(text)?;
            let col = u16::try_from(line[..at].chars().count()).unwrap();
            screen.cell(row, col).map(vt100::Cell::bgcolor)
        })
    }

    pub(crate) fn keys(&mut self, bytes: &[u8]) {
        self.input.send(bytes.to_vec()).unwrap();
        // Let the UI read one burst at a time, as typing would.
        std::thread::sleep(Duration::from_millis(60));
    }

    /// A left click at `column`, `row` of the screen (from 0), as the
    /// person's terminal reports it (SGR, from 1).
    pub(crate) fn click(&mut self, column: u16, row: u16) {
        let (x, y) = (column + 1, row + 1);
        self.keys(format!("\x1b[<0;{x};{y}M").as_bytes());
        self.keys(format!("\x1b[<0;{x};{y}m").as_bytes());
    }

    /// A press at `from`, a drag to `to` and the release there.
    pub(crate) fn drag(&mut self, from: (u16, u16), to: (u16, u16)) {
        let (x, y) = (from.0 + 1, from.1 + 1);
        self.keys(format!("\x1b[<0;{x};{y}M").as_bytes());
        let (x, y) = (to.0 + 1, to.1 + 1);
        self.keys(format!("\x1b[<32;{x};{y}M").as_bytes());
        self.keys(format!("\x1b[<0;{x};{y}m").as_bytes());
    }

    /// A turn of the wheel back, at `column`, `row`.
    pub(crate) fn wheel_back(&mut self, column: u16, row: u16) {
        self.keys(format!("\x1b[<64;{};{}M", column + 1, row + 1).as_bytes());
    }

    /// The screen's `row`, from `column`, `width` cells of it.
    pub(crate) fn cells(&self, row: u16, column: u16, width: u16) -> String {
        let parser = self.screen.lock().unwrap();
        parser
            .screen()
            .contents_between(row, column, row, column + width)
    }

    pub(crate) fn wait_for_exit(&mut self) -> bool {
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
pub(crate) struct StopDaemon(pub(crate) PathBuf);

impl Drop for StopDaemon {
    fn drop(&mut self) {
        if let Some(pid) = daemon_pid(&self.0).and_then(|pid| i32::try_from(pid).ok()) {
            let _ = kill(Pid::from_raw(pid), Signal::SIGTERM);
        }
    }
}
