//! End-to-end test of a workspace's changes in the terminal UI (`PRODUCT.md`
//! §6): the real `muster` binary on a real pseudo-terminal, a change made
//! in the project's checkout, shown with `d` and its diff with ⏎.

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

#[test]
fn d_shows_what_changed_in_a_checkout_and_enter_a_file_s_diff() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());
    std::fs::write(root.join("README.md"), "test\nchanged by the agent\n").unwrap();

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.wait_for("Add a project");
    ui.keys(b"\r");
    ui.wait_for("⌂ main");

    // The checkout's line, then its changes beside the list.
    ui.keys(b"\x1b[B");
    ui.wait_for("d changes");
    ui.keys(b"d");
    ui.wait_for("Changes · main");
    ui.wait_for("Not committed · 1 file");
    ui.wait_for("M  README.md");
    ui.wait_for("+1");

    ui.keys(b"\r");
    ui.wait_for("README.md · not committed");
    ui.wait_for("+changed by the agent");

    // Esc goes back to the files, and again to the list.
    ui.keys(b"\x1b");
    ui.wait_for("Not committed · 1 file");
    ui.keys(b"\x1b");
    ui.wait_without("Not committed · 1 file");
    // The line says what the work comes to, as the pane last saw.
    ui.wait_for("⌂ main");
    let line = ui
        .text()
        .lines()
        .find(|line| line.contains("⌂ main"))
        .unwrap()
        .to_owned();
    assert!(line.contains("+1"), "{line}");
}
