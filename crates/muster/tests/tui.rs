//! End-to-end tests for the terminal UI (M1 step 9): the real `muster`
//! binary on a real pseudo-terminal, driven by keystrokes, read back through
//! a terminal emulator — as a person would use it.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

mod support;
#[path = "tui/ui.rs"]
mod ui;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use support::{Sandbox, daemon_pid};
use ui::{StopDaemon, Ui};

/// What the top bar says of the daemon, whatever the version.
const DAEMON: &str = concat!("daemon ", env!("CARGO_PKG_VERSION"));

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
    // Its checkout too, which can come after the reply.
    ui.wait_for("⌂ main");

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
    // Its checkout too, which can come after the reply.
    ui.wait_for("⌂ main");
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
fn kep_welcomes_the_person_and_a_key_typed_ahead_goes_through() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());

    // The welcome fills the window as the UI starts: Kep, Muster's name,
    // and nothing of the screens behind.
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("┏┳┓╻ ╻┏━┓╺┳╸┏━╸┏━┓");
    let screen = ui.text();
    assert!(
        screen.contains('▀') && !screen.contains("Sessions"),
        "{screen}"
    );

    // `a`, typed ahead, lets the welcome go and opens what it says.
    ui.keys(b"a");
    ui.wait_for("Add a project");
    ui.keys(b"\x1b");
    ui.wait_without("Add a project");
    // Kep sits beside the name in the pane, with nothing to show yet: once
    // the daemon the UI started answers, which the key typed ahead may
    // well have come before.
    ui.wait_for("No projects yet");
    let screen = ui.text();
    assert!(screen.contains('▀'), "{screen}");
    ui.keys(b"q");
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
    // Its checkout too, which can come after the reply.
    ui.wait_for("⌂ main");

    // n on the project: a shell in its own checkout, beside the list.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    // Waited for by what only the output says: the command as typed
    // says "300" too, before there is anything to scroll back to.
    ui.keys(b"seq 1 300; echo seq-$((2*3))\r");
    ui.wait_for("seq-6");

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

/// The UI started with `env`, a shell open in the project's checkout that
/// printed two lines, and a drag over them, which the UI said it copied.
fn drag_over_two_lines(sandbox: &Sandbox, env: &[(&str, &str)]) -> Ui {
    let root = sandbox.repository("app");
    let mut ui = Ui::start_with(sandbox, &root, env);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    // Its checkout too, which can come after the reply.
    ui.wait_for("⌂ main");
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"echo copy-$((6*7)); echo next-$((2*4))\r");
    ui.wait_for("next-8");

    // Pressed on the first line's first letter, dragged to the second's
    // last and let go (SGR, 1-based).
    let (column, row) = ui.position_of("copy-42").unwrap();
    let (end, end_row) = ui.position_of("next-8").unwrap();
    let (end, end_row) = (end + 6, end_row + 1);
    ui.keys(format!("\x1b[<0;{};{}M", column + 1, row + 1).as_bytes());
    ui.keys(format!("\x1b[<32;{end};{end_row}M").as_bytes());
    ui.keys(format!("\x1b[<0;{end};{end_row}m").as_bytes());
    ui.wait_for("Copied 2 lines.");
    ui
}

#[test]
fn a_drag_over_a_session_copies_its_text_with_the_system_s_own_program() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    // On the person's machine: `pbcopy` on macOS, `xclip` under X on
    // Linux, the sandbox's stand-ins both.
    let _ui = drag_over_two_lines(
        &sandbox,
        &[
            ("SSH_CONNECTION", ""),
            ("SSH_TTY", ""),
            ("WAYLAND_DISPLAY", ""),
            ("DISPLAY", ":0"),
        ],
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    // The session's text, none of the list beside it.
    while sandbox.clipboard().as_deref() != Some("copy-42\nnext-8") {
        assert!(Instant::now() < deadline, "{:?}", sandbox.clipboard());
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[test]
fn over_ssh_a_drag_copies_through_the_person_s_terminal() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    // The machine's clipboard is not the person's: their terminal is asked
    // (OSC 52), which is this test's.
    let ui = drag_over_two_lines(
        &sandbox,
        &[("SSH_CONNECTION", "192.0.2.1 50000 192.0.2.2 22")],
    );
    // "copy-42\nnext-8".
    ui.wait_for_written(b"\x1b]52;c;Y29weS00MgpuZXh0LTg=\x07");
    assert_eq!(sandbox.clipboard(), None);
}

/// A shell open in the project's checkout, beside the list, that printed
/// 200 lines.
fn shell_with_output(sandbox: &Sandbox, env: &[(&str, &str)]) -> Ui {
    let root = sandbox.repository("app");
    let mut ui = Ui::start_with(sandbox, &root, env);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    // Its checkout too, which can come after the reply.
    ui.wait_for("⌂ main");
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"seq 101 300; echo seq-$((2*3))\r");
    ui.wait_for("seq-6");
    ui
}

/// The pane beside the list starts here: after the 36-column list of a
/// 120-column window and the pane's border, below the top bar and it.
const PANE: (u16, u16) = (37, 2);

#[test]
fn the_mouse_reaches_what_it_points_at_around_a_copy() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let mut ui = shell_with_output(
        &sandbox,
        &[
            ("SSH_CONNECTION", ""),
            ("SSH_TTY", ""),
            ("WAYLAND_DISPLAY", ""),
            ("DISPLAY", ":0"),
        ],
    );
    // Back to the list, the session still selected: the wheel over its
    // preview looks back.
    ui.keys(b"Q");
    ui.wheel_back(80, 10);
    ui.wait_for("3 lines back");

    // A drag over the preview opens the session and copies what it covers.
    let first = ui.cells(PANE.1, PANE.0, 4).trim().to_owned();
    let second = ui.cells(PANE.1 + 1, PANE.0, 4).trim().to_owned();
    ui.drag(PANE, (PANE.0 + 3, PANE.1 + 1));
    ui.wait_for("Copied 2 lines.");
    let wanted = format!("{first}\n{second}");
    let deadline = Instant::now() + Duration::from_secs(15);
    while sandbox.clipboard().as_deref() != Some(wanted.as_str()) {
        assert!(Instant::now() < deadline, "{:?}", sandbox.clipboard());
        std::thread::sleep(Duration::from_millis(25));
    }

    // A click on the project's line (on its name: its triangle, before
    // it, folds it) gives the list the keys and selects it; one on its
    // details opens nothing, so no dialog takes the clicks.
    ui.click(6, 2);
    ui.wait_for("╭ Details");
    ui.click(80, 10);
    std::thread::sleep(Duration::from_millis(300));
    assert!(!ui.text().contains("New worktree"), "{}", ui.text());

    // The session's line, then its pane: the session has the keys.
    let (column, row) = ui.position_of("Shell").unwrap();
    ui.click(column, row);
    ui.wait_for("╭ Shell");
    ui.click(80, 10);
    ui.wait_for("keys go to the session");
    // The wheel, over the list, looks back in the session with the keys.
    ui.wheel_back(5, 10);
    ui.wait_for("3 lines back");
    ui.keys(b"echo typed-$((3*3))\r");
    ui.wait_for("typed-9");
}

#[test]
fn a_program_takes_the_mouse_and_lets_it_go_and_full_screen_is_left_by_a_click() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let mut ui = shell_with_output(&sandbox, &[]);
    // As Claude Code does around its full-screen views: the alternate
    // screen and the mouse, every way of tracking it, until a line is read.
    ui.keys(
        b"printf '\\033[?1049h\\033[?1000h\\033[?1002h\\033[?1003h\\033[?1006h'; read -r line; \
          printf '\\033[?1006l\\033[?1003l\\033[?1002l\\033[?1000l\\033[?1049l'; \
          echo back-$((6*7))\r",
    );
    // The alternate screen shown: the modes, sent after the output that
    // set them, have come too.
    ui.wait_without("seq-6");
    std::thread::sleep(Duration::from_millis(100));
    ui.wheel_back(80, 10);
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !ui.text().contains("lines back"),
        "the program's: {}",
        ui.text()
    );
    ui.keys(b"\r");
    ui.wait_for("back-42");
    // Let go of: the wheel is Muster's again.
    ui.wheel_back(80, 10);
    ui.wait_for("3 lines back");

    // Full screen, and back to the list by a click on the top bar.
    ui.keys(b"Q");
    ui.keys(b"f");
    ui.wait_for("(or click here)");
    ui.click(2, 0);
    ui.wait_for("╭ Sessions");
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

    // ⏎: which agent, Claude Code first; ⏎ again starts it, opened as it
    // starts, with the ticket to work on.
    ui.keys(b"\r");
    ui.wait_for("Assign an agent");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_for("⏎ open agent  a another agent");

    // a: another, in a worktree of its own.
    ui.keys(b"a");
    ui.wait_for("In a worktree of its own");
    ui.keys(b"\r");
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
    ui.wait_for("Assign an agent");
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
