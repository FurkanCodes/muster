//! End-to-end tests of a session's terminal behaving as a real one, in the
//! terminal UI: the real `muster` binary on a real pseudo-terminal, whose
//! stand-in terminal says its colours and keeps what is copied.

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

use support::Sandbox;
use ui::{BACKGROUND, StopDaemon, Ui};

/// On the person's machine, so copies go through the system's own program
/// (the sandbox's stand-ins).
const LOCAL: [(&str, &str); 4] = [
    ("SSH_CONNECTION", ""),
    ("SSH_TTY", ""),
    ("WAYLAND_DISPLAY", ""),
    ("DISPLAY", ":0"),
];

/// The UI, with the project added and a shell open beside the list, and
/// the project's folder.
fn shell(sandbox: &Sandbox) -> (Ui, PathBuf) {
    let root = sandbox.repository("app");
    let mut ui = Ui::start_with(sandbox, &root, &LOCAL);
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
    (ui, root)
}

#[test]
fn a_program_in_a_session_is_told_the_person_s_terminal_s_background() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let (mut ui, _) = shell(&sandbox);
    // OSC 11, ended with ST; what comes back, ESC as E and `\` as B.
    ui.keys(
        b"stty -echo -icanon min 1; printf '\\033]11;?\\033\\\\'; \
          r=$(dd bs=1 count=25 2>/dev/null | tr '\\033\\\\' 'EB'); stty echo icanon; \
          echo \"answer=$r=$((2*21))\"\r",
    );
    ui.wait_for(&format!("answer=E]11;{BACKGROUND}EB=42"));
}

#[test]
fn a_program_s_copy_reaches_the_clipboard_and_is_not_made_again_on_attaching() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let (mut ui, root) = shell(&sandbox);
    // "copy-42", as a program copies it.
    ui.keys(b"printf '\\033]52;c;Y29weS00Mg==\\007'; echo sent-$((2*4))\r");
    ui.wait_for("sent-8");
    ui.wait_for("The session copied a line.");
    let deadline = Instant::now() + Duration::from_secs(15);
    while sandbox.clipboard().as_deref() != Some("copy-42") {
        assert!(Instant::now() < deadline, "{:?}", sandbox.clipboard());
        std::thread::sleep(Duration::from_millis(25));
    }

    // Another UI, which shows the session from the start of its history,
    // the copy in it: not copied again.
    ui.keys(b"Q");
    ui.keys(b"q");
    assert!(ui.wait_for_exit());
    std::fs::remove_file(sandbox.root().join("fake-clipboard/copied")).unwrap();
    let mut again = Ui::start_with(&sandbox, &root, &LOCAL);
    again.wait_for("⌂ main");
    // Down from the project, past its checkout, to the session.
    again.keys(b"\x1b[B");
    again.keys(b"\x1b[B");
    again.wait_for("sent-8");
    // And on into its live output, which copies nothing either.
    again.keys(b"\r");
    again.wait_for("Shift-Q back to the list");
    again.keys(b"echo after-$((3*3))\r");
    again.wait_for("after-9");
    std::thread::sleep(Duration::from_millis(1000));
    assert_eq!(sandbox.clipboard(), None);
    assert!(!again.text().contains("copied"), "{}", again.text());
}

#[test]
fn the_ui_draws_on_a_screen_of_its_own_and_gives_the_shell_s_back_arc_13() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let root = sandbox.repository("app");
    // The person's terminal as their shell left it.
    let earlier: Vec<u8> = (0..ui::ROWS - 1)
        .flat_map(|row| format!("$ old shell line {row:02}\r\n").into_bytes())
        .collect();
    let mut ui = Ui::start_over(&sandbox, &root, &LOCAL, &earlier);
    ui.wait_for("No projects yet");
    assert!(!ui.text().contains("old shell line"), "{}", ui.text());

    ui.keys(b"q");
    assert!(ui.wait_for_exit());
    ui.wait_for("$ old shell line 00");
    ui.wait_without("No projects yet");
}

#[test]
fn resizing_the_window_updates_the_fixed_viewport_and_the_program_arc_13() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let (mut ui, _) = shell(&sandbox);
    ui.resize(90, 36);
    // A narrow window opens the session over the list. Its top bar takes
    // one row, and its PTY must receive the new columns too.
    ui.keys(&when_sized(35, 90));
    ui.wait_for("size=35 90");
    ui.resize(120, 36);
    // Back beside the list: the session has its border and bottom bar.
    ui.keys(&when_sized(32, 88));
    ui.wait_for("size=32 88");
}

/// A command line that waits for the shell's terminal to be `rows` by
/// `cols`, then prints `size=<rows> <cols>`, which the line itself, echoed,
/// never reads.
fn when_sized(rows: u16, cols: u16) -> Vec<u8> {
    format!(
        "until [ \"$(stty size)\" = '{rows} {cols}' ]; do sleep .05; done; \
         printf 'si%s=%s\\n' ze \"$(stty size)\"\r"
    )
    .into_bytes()
}
