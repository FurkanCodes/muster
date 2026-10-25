//! End-to-end tests of spaces and terminals in no project in the terminal
//! UI (`PRODUCT.md` §6, `ARC-33`): the real `muster` binary on a real
//! pseudo-terminal over the real daemon. `+` starts a terminal in no
//! project, listed under Terminals; `W` makes a space and shows it; `]`
//! goes back to everything;
//! `F` files a terminal whose shell went into a project's folder under it,
//! the same shell running on.

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

use support::Sandbox;
use ui::{StopDaemon, Ui};

/// A sandbox whose home folder is its own, where free terminals start.
fn sandbox() -> Sandbox {
    let sandbox = Sandbox::new();
    let home = sandbox.root().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let home = home.canonicalize().unwrap();
    sandbox.with_env("HOME", home.to_str().unwrap())
}

#[test]
fn a_space_is_made_shown_and_left_and_a_free_terminal_is_filed_where_its_shell_is() {
    let sandbox = sandbox();
    let _stop = StopDaemon(sandbox.paths().socket());
    let _daemon = sandbox.start();
    let root = sandbox.repository("app");
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("⌂ main");

    // A terminal in no project, under Terminals; back on the list.
    ui.keys(b"+");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_without("Shift-Q back to the list");
    ui.wait_for("Terminals");

    // A space, made from the menu and shown at once: empty.
    ui.keys(b"W");
    ui.wait_for("new space…");
    ui.keys(b"\x1b[B\r");
    ui.wait_for("it doesn't change where they run");
    ui.keys(b"Work\r");
    ui.wait_for("Sessions · Work");
    ui.wait_for("Showing Work.");
    assert!(!ui.text().contains("⌂ main"), "{}", ui.text());

    // Everything again.
    ui.keys(b"]");
    ui.wait_for("Showing everything.");
    ui.wait_for("⌂ main");

    // The free terminal's shell into the project's folder, by the person:
    // then F files it there, as it is.
    let typed = format!("cd {}\r", root.display());
    ui.keys(b"+");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(typed.as_bytes());
    ui.keys(b"Q");
    ui.wait_without("Shift-Q back to the list");
    ui.keys(b"F");
    ui.wait_for("file under app, as it is");
    ui.keys(b"\r");
    ui.wait_for("Filed under the project, still running.");
}
