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
/// What the top bar says of the daemon, whatever the version.
const DAEMON: &str = concat!("daemon ", env!("CARGO_PKG_VERSION"));
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
        Self::start_with(sandbox, cwd, &[])
    }

    /// Starts the UI with `env` set as well.
    fn start_with(sandbox: &Sandbox, cwd: &Path, env: &[(&str, &str)]) -> Self {
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
        for (key, value) in env {
            command.env(key, value);
        }
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

    /// The UI's process id.
    fn pid(&self) -> u32 {
        self.child.process_id().expect("the UI's pid")
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

    /// The background of the cell where `text` first starts on screen.
    fn background_of(&self, text: &str) -> Option<vt100::Color> {
        let parser = self.screen.lock().unwrap();
        let screen = parser.screen();
        (0..ROWS).find_map(|row| {
            let line = screen.contents_between(row, 0, row, COLS);
            let at = line.find(text)?;
            let col = u16::try_from(line[..at].chars().count()).unwrap();
            screen.cell(row, col).map(vt100::Cell::bgcolor)
        })
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
    ui.wait_for(DAEMON);

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

    // n, Tab, Enter: a shell in it, which opens beside the list with the
    // keys.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");

    // Typing reaches the shell, and its output comes back.
    ui.keys(b"echo ui-$((6*7))\r");
    ui.wait_for("ui-42");
    assert!(
        ui.text().contains("⎇ ui-work"),
        "the list stays: {}",
        ui.text()
    );

    // Shift-Q gives the list the keys back; the session is listed.
    ui.keys(b"Q");
    ui.wait_for("f full screen");
    ui.wait_for("Shell");

    // f fills the window with it, and Shift-Q puts it back.
    ui.keys(b"f");
    ui.wait_for("Shift-Q back to the list");
    assert!(!ui.text().contains("╭ Sessions"), "{}", ui.text());
    ui.keys(b"Q");
    ui.wait_for("╭ Sessions");

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
fn tabs_enter_ctrl_q_and_x_get_about_and_close_a_session() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"echo first-$((6*7))\r");
    ui.wait_for("first-42");

    // Shift-Q out to the list; ⇥ round the tabs and back to Sessions.
    ui.keys(b"Q");
    ui.wait_for("x close");
    ui.keys(b"\t");
    ui.wait_for("Nothing is waiting on you.");
    // On round Tickets and Settings.
    ui.keys(b"\t\t");
    ui.wait_for("Only my tickets");
    ui.keys(b"\t");
    ui.wait_for("x close");

    // Ctrl-2, as a terminal with the kitty keyboard protocol sends it,
    // straight to Needs you; Ctrl-1 back.
    ui.keys(b"\x1b[50;5u");
    ui.wait_for("Nothing is waiting on you.");
    ui.keys(b"\x1b[49;5u");
    ui.wait_for("x close");

    // ⏎ back into the session.
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"echo second-$((6*7))\r");
    ui.wait_for("second-42");

    // x closes it: asked once, then stopped and gone from the list.
    ui.keys(b"Q");
    ui.wait_for("x close");
    ui.keys(b"x");
    ui.wait_for("Close Shell? It stops and leaves the list.");
    ui.keys(b"y");
    ui.wait_for("Closed the session.");
    let status = sandbox.run(&["status"]);
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("0 sessions"), "{stdout}");
}

#[test]
fn a_plain_folder_is_added_from_the_ui_and_a_session_runs_in_it() {
    let sandbox = Sandbox::new();
    let folder = sandbox.root().join("Desktop").join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    let folder = folder.canonicalize().unwrap();
    let _stop = StopDaemon(sandbox.paths().socket());

    // a: add the folder the UI was started in, though it is no repository.
    let mut ui = Ui::start(&sandbox, &folder);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.wait_for("or any folder");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    ui.wait_for("⌂ not a git repository");

    // w: no worktrees in a plain folder, and it says so.
    ui.keys(b"w");
    ui.wait_for("plain folder, not a git repository");

    // Enter on the project: a session in the folder.
    ui.keys(b"\r");
    ui.wait_for("In notes");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"pwd; echo folder-$((6*7))\r");
    ui.wait_for("folder-42");
    assert!(ui.text().contains("Desktop/notes"), "{}", ui.text());

    ui.keys(b"Q");
    ui.wait_for("f full screen");
    ui.keys(b"q");
    assert!(ui.wait_for_exit());
    assert!(!folder.join(".git").exists(), "Muster made no repository");
}

#[test]
fn the_folder_to_add_is_found_by_browsing_folders() {
    let sandbox = Sandbox::new();
    let repo = sandbox.repository("app");
    let repos = repo.parent().unwrap().to_owned();
    std::fs::create_dir_all(repos.join("notes").join("drafts")).unwrap();
    let _stop = StopDaemon(sandbox.paths().socket());

    // a: the folders where the UI was started, repositories marked.
    let mut ui = Ui::start(&sandbox, &repos);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.wait_for("app/  git");
    ui.wait_for("notes/");

    // Typing narrows them; ⇥ opens the chosen one, ⇧⇥ comes back up.
    ui.keys(b"no");
    ui.wait_for("› notes/");
    ui.keys(b"\t");
    ui.wait_for("drafts/");
    ui.keys(b"\x1b[Z");
    ui.wait_for("app/  git");

    // ⏎ adds the chosen folder.
    ui.keys(b"no");
    ui.wait_for("› notes/");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    ui.wait_for("⌂ not a git repository");
    let status = sandbox.run(&["status"]);
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(
        stdout.contains(&repos.join("notes").display().to_string()),
        "{stdout}"
    );
}

#[test]
fn a_daemon_the_ui_started_is_reaped_when_it_stops_and_another_takes_over() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let socket = sandbox.paths().socket();
    let _stop = StopDaemon(socket.clone());

    let ui = Ui::start(&sandbox, &root);
    ui.wait_for(DAEMON);
    let first = daemon_pid(&socket).expect("the UI started a daemon");
    let pid = Pid::from_raw(i32::try_from(first).unwrap());
    kill(pid, Signal::SIGTERM).unwrap();

    // The daemon is not the UI's child, and the system reaps it, so no
    // zombie is left: signalling the pid fails once it is gone, which a
    // zombie's would not.
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
    ui.wait_for(DAEMON);
}

/// The parent of process `pid`, as `ps` tells it.
fn parent_of(pid: u32) -> Option<u32> {
    let out = std::process::Command::new("ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// `root` and every process below it, as `ps` tells it now.
fn tree_of(root: u32) -> Vec<u32> {
    let out = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid="])
        .output()
        .unwrap();
    let pairs: Vec<(u32, u32)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
        })
        .collect();
    let mut tree = vec![root];
    let mut at = 0;
    while at < tree.len() {
        let parent = tree[at];
        tree.extend(
            pairs
                .iter()
                .filter(|(_, ppid)| *ppid == parent)
                .map(|(pid, _)| *pid),
        );
        at += 1;
    }
    tree
}

#[test]
fn the_daemon_the_ui_starts_outlives_the_ui_s_whole_process_tree_and_hang_ups() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let socket = sandbox.paths().socket();
    let _stop = StopDaemon(socket.clone());
    let ui = Ui::start(&sandbox, &root);
    ui.wait_for(DAEMON);
    let daemon = daemon_pid(&socket).expect("the UI started a daemon");

    // It is apart from the UI from the start: nothing of the UI's.
    let ui_tree = tree_of(ui.pid());
    assert!(!ui_tree.contains(&daemon), "{daemon} is in {ui_tree:?}");
    assert_ne!(parent_of(daemon), Some(ui.pid()));

    // What some terminals do as a pane closes: kill everything under it.
    for pid in tree_of(ui.pid()) {
        let _ = kill(Pid::from_raw(i32::try_from(pid).unwrap()), Signal::SIGKILL);
    }
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(daemon_pid(&socket), Some(daemon), "the daemon lives on");

    // A hang-up is someone else's: logged, and ignored.
    kill(
        Pid::from_raw(i32::try_from(daemon).unwrap()),
        Signal::SIGHUP,
    )
    .unwrap();
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(daemon_pid(&socket), Some(daemon), "a hang-up stops nothing");
    let log = sandbox.paths().runtime_dir.join("daemon.log");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("ignored a hang-up")
    {
        assert!(Instant::now() < deadline, "the hang-up is not in the log");
        std::thread::sleep(Duration::from_millis(50));
    }

    // Asked to stop, it says why as it goes.
    kill(
        Pid::from_raw(i32::try_from(daemon).unwrap()),
        Signal::SIGTERM,
    )
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !std::fs::read_to_string(&log)
        .unwrap_or_default()
        .contains("daemon stopped")
    {
        assert!(Instant::now() < deadline, "it never stopped");
        std::thread::sleep(Duration::from_millis(50));
    }
    let log = std::fs::read_to_string(&log).unwrap();
    assert!(log.contains("stopping: asked to stop"), "{log}");
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

    // n on the project: a shell in its own checkout, beside the list.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"seq 1 300\r");
    ui.wait_for("300");

    // Two turns of the wheel over the session, as the terminal reports
    // them (SGR).
    ui.keys(b"\x1b[<64;60;10M");
    ui.keys(b"\x1b[<64;60;10M");
    ui.wait_for("6 lines back");
    // Typing returns to the newest output.
    ui.keys(b"echo back-$((6*7))\r");
    ui.wait_for("back-42");
    assert!(!ui.text().contains("lines back"), "{}", ui.text());
}

#[test]
fn tickets_are_fetched_and_agents_assigned_to_them_from_the_ui() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let gh = sandbox.fake_gh();
    let issue = serde_json::json!({
        "number": 42, "title": "Fix the login redirect",
        "url": "https://github.com/o/app/issues/42", "labels": [{"name": "bug"}],
        "author": {"login": "someone"}
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

    // i: the Tickets screen, with the project's tickets; the selected
    // one's text comes as it is shown.
    ui.keys(b"i");
    ui.wait_for("app  1 open");
    ui.keys(b"j");
    ui.wait_for("users land on /404.");
    ui.wait_for("No agent on it yet");

    // ⏎: an agent on it, opened as it starts, with the ticket to work on.
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_for("⏎ open agent  a another agent");

    // a: another, in a worktree of its own.
    ui.keys(b"a");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_for("muster/issue-42-fix-the-login-redirect-2");

    let listed = sandbox.run(&["issue", "list", "app"]);
    let listed = String::from_utf8_lossy(&listed.stdout);
    assert!(
        listed.contains("#42") && listed.contains("2 agents"),
        "{listed}"
    );

    // x: closed in its tracker, GitHub, after asking, then its agents stopped and its
    // worktrees removed, if the person wants; the branches stay.
    ui.keys(b"x");
    ui.wait_for("Close #42 Fix the login redirect in GitHub issues?");
    ui.keys(b"y");
    ui.wait_for("Also close its 2 agents");
    ui.keys(b"y");
    let closed = std::fs::read_to_string(gh.join("closed")).unwrap();
    assert_eq!(closed.trim(), "42");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let status = String::from_utf8_lossy(&sandbox.run(&["status"]).stdout).into_owned();
        if status.contains("0 worktrees") && status.contains("0 sessions") {
            break;
        }
        assert!(Instant::now() < deadline, "{status}\n{}", ui.text());
        std::thread::sleep(Duration::from_millis(100));
    }
    let branches = support::git(&root, &["branch", "--list", "--format=%(refname:short)"]);
    assert!(
        branches.contains("muster/issue-42-fix-the-login-redirect-2"),
        "{branches}"
    );
}

#[test]
fn only_my_tickets_is_turned_on_and_off_on_the_settings_screen() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let gh = sandbox.fake_gh();
    std::fs::write(
        gh.join("issues.json"),
        serde_json::json!([{
            "number": 42, "title": "Fix the login redirect",
            "url": "https://github.com/o/app/issues/42", "labels": [],
            "author": {"login": "someone"}
        }])
        .to_string(),
    )
    .unwrap();
    let _stop = StopDaemon(sandbox.paths().socket());

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    ui.keys(b"i");
    ui.wait_for("app  1 open");

    // 4: the Settings screen, on the project's only setting.
    ui.keys(b"4");
    ui.wait_for("Only my tickets");
    ui.wait_for("The Tickets screen lists every open ticket.");
    ui.wait_for("Tickets from  GitHub issues, as its git remote says");
    ui.keys(b"\r");
    ui.wait_for("Tickets for app now lists only the tickets assigned to you.");
    // The person's own settings for the project, the one file there.
    let projects = sandbox.paths().config_dir.join("projects");
    let files: Vec<PathBuf> = std::fs::read_dir(&projects)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    let [file] = &files[..] else {
        panic!("{files:?}");
    };
    let written = std::fs::read_to_string(file).unwrap();
    assert!(written.ends_with("[tickets]\nmine = true\n"), "{written}");

    // The Tickets screen asks again, for only the person's.
    ui.keys(b"3");
    ui.wait_for("app  1 of yours open");
    let calls = std::fs::read_to_string(gh.join("calls")).unwrap();
    let last = calls
        .lines()
        .rfind(|call| call.starts_with("issue list"))
        .unwrap();
    assert!(last.ends_with("--assignee @me"), "{calls}");

    // And off again, with space.
    ui.keys(b"4");
    ui.wait_for("⏎ or space turns it off");
    ui.keys(b" ");
    ui.wait_for("Tickets for app now lists every open ticket.");
    assert!(
        std::fs::read_to_string(file)
            .unwrap()
            .ends_with("[tickets]\nmine = false\n")
    );
}

#[test]
fn a_running_agent_s_mark_spins_and_its_line_flashes_as_it_finishes_unless_motion_is_off() {
    // The ticket's line up to its agent's mark: its letter, C for Claude.
    const LINE: &str = "#42 Fix the login redirect C";
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let gh = sandbox.fake_gh();
    let issue = serde_json::json!({
        "number": 42, "title": "Fix the login redirect",
        "url": "https://github.com/o/app/issues/42", "labels": [],
        "author": {"login": "someone"}
    });
    std::fs::write(
        gh.join("issues.json"),
        serde_json::json!([issue]).to_string(),
    )
    .unwrap();
    std::fs::write(gh.join("issue-42.json"), issue.to_string()).unwrap();
    let _stop = StopDaemon(sandbox.paths().socket());

    // An agent on the ticket, started with it to work on: running.
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    ui.keys(b"i");
    ui.wait_for("app  1 open");
    ui.keys(b"j");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_for("⏎ open agent");

    // Its mark on the ticket's line goes round the spinner's frames.
    let mark = |ui: &Ui| {
        let text = ui.text();
        let at = text.find(LINE).expect("the ticket's line") + LINE.len();
        text[at..].chars().next().unwrap()
    };
    let first = mark(&ui);
    assert!("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏".contains(first), "{}", ui.text());
    let deadline = Instant::now() + Duration::from_secs(5);
    while mark(&ui) == first {
        assert!(Instant::now() < deadline, "it never moved: {}", ui.text());
        std::thread::sleep(Duration::from_millis(20));
    }

    // It finishes its turn: on the sessions list its line lights up in
    // green, then settles.
    ui.keys(b"1");
    ui.wait_for("running");
    let status = String::from_utf8_lossy(&sandbox.run(&["status"]).stdout).into_owned();
    let session: muster_core::SessionId = status
        .split_whitespace()
        .find(|word| word.starts_with("ses_"))
        .expect("the agent in the status")
        .parse()
        .unwrap();
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let agent = launch.value("--session-id").unwrap().to_owned();
    launch.fire(
        "Stop",
        &serde_json::json!({"session_id": agent, "hook_event_name": "Stop", "cwd": "/work"}),
    );
    ui.wait_for("✓ #42");
    assert_eq!(
        ui.background_of("✓ #42"),
        Some(vt100::Color::Idx(2)),
        "{}",
        ui.text()
    );
    // Settled, it is the selected line: tinted, as the selection is.
    let deadline = Instant::now() + Duration::from_secs(5);
    while ui.background_of("✓ #42") != Some(vt100::Color::Idx(8)) {
        assert!(Instant::now() < deadline, "it never settled: {}", ui.text());
        std::thread::sleep(Duration::from_millis(20));
    }
    ui.keys(b"q");
    assert!(ui.wait_for_exit());

    // MUSTER_MOTION=off: the agent's next turn spins nothing, and its
    // end lights up nothing.
    launch.fire(
        "UserPromptSubmit",
        &serde_json::json!({"session_id": agent, "hook_event_name": "UserPromptSubmit", "cwd": "/work"}),
    );
    let mut ui = Ui::start_with(&sandbox, &root, &[("MUSTER_MOTION", "off")]);
    ui.keys(b"i");
    ui.wait_for(&format!("{LINE}●"));
    std::thread::sleep(Duration::from_millis(500));
    assert_eq!(mark(&ui), '●', "{}", ui.text());
    ui.keys(b"1");
    // Its prompt went in: it is thinking, as its hooks said.
    ui.wait_for("thinking");
    launch.fire(
        "Stop",
        &serde_json::json!({"session_id": agent, "hook_event_name": "Stop", "cwd": "/work"}),
    );
    ui.wait_for("✓ #42");
    assert_eq!(
        ui.background_of("✓ #42"),
        Some(vt100::Color::Default),
        "never lit (and not the selected line): {}",
        ui.text()
    );
    ui.keys(b"q");
    assert!(ui.wait_for_exit());
}
