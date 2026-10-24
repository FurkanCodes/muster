//! End-to-end test of a name too long for its line, in the terminal UI
//! (`PRODUCT.md` §6): the real `muster` binary on a real pseudo-terminal,
//! an agent on a ticket whose title the sessions list cuts short, sliding
//! along on the selected line until its end shows, then back to its
//! start.

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

use support::wait;
#[path = "tui/ui.rs"]
mod ui;

use support::Sandbox;
use ui::{StopDaemon, Ui};

/// The list's columns in the UI's window of 120, the cells either side of
/// its lines with them.
const LIST: usize = 36;

/// What the list shows, without the pane beside it, whose title says the
/// session's in full.
fn list(ui: &Ui) -> String {
    ui.text()
        .lines()
        .map(|line| line.chars().take(LIST).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Waits for the list to show `wanted`.
fn wait_in_list(ui: &Ui, wanted: &str) {
    assert!(
        wait::within(wait::WAIT, || list(ui).contains(wanted).then_some(())).is_some(),
        "never {wanted:?} in the list: {}",
        ui.text()
    );
}

#[test]
fn a_title_too_long_for_its_line_slides_along_on_the_selected_line() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let gh = sandbox.fake_gh();
    let issue = serde_json::json!({
        "number": 42, "title": "Fix the login redirect loop on Safari",
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

    // An agent on the ticket, its session titled after it.
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

    // On the sessions list its line is the selected one: cut short at
    // first, it slides along until its end shows, then starts again.
    ui.keys(b"1");
    wait_in_list(&ui, "#42 Fix the");
    assert!(!list(&ui).contains("Safari"), "{}", ui.text());
    wait_in_list(&ui, "Safari");
    assert!(!list(&ui).contains("#42 Fix the"), "{}", ui.text());
    wait_in_list(&ui, "#42 Fix the");
    ui.keys(b"q");
    assert!(ui.wait_for_exit());
}
