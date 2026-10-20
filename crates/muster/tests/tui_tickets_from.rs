//! End-to-end test of choosing where a project's tickets come from in the
//! terminal UI (`PRD-15`): anyone's ticket plugin, installed from a git
//! repository, offered on the Settings screen's *Tickets from* by its
//! name, asked for the setting it declares, and its tickets then listed.

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
fn anyone_s_ticket_plugin_is_chosen_with_its_setting_and_lists_its_tickets() {
    let sandbox = Sandbox::new().with_env("FIXTURE_TOKEN", "fixture-s3cret-0451");
    let plugin = sandbox.ticket_plugin_repository();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("Added the project.");
    let address = format!("file://{}", plugin.display());
    let installed = sandbox.run(&["plugin", "install", &address, "--yes"]);
    assert!(
        installed.status.success(),
        "{}",
        String::from_utf8_lossy(&installed.stderr)
    );

    // 4: the Settings screen; ↓ to Tickets from, ⏎ for the dialog, which
    // offers Muster's own and the plugin by their names.
    ui.keys(b"4");
    ui.wait_for("Tickets from  GitHub issues, as its git remote says");
    ui.keys(b"\x1b[B");
    ui.keys(b"\r");
    ui.wait_for("Where app's tickets come from");
    ui.wait_for(" Fixture ");
    for name in [" GitHub ", " GitLab ", " Jira ", " Linear "] {
        assert!(ui.text().contains(name), "{name}: {}", ui.text());
    }
    // ⇧⇥ from its git remote: the last, anyone's, with what it declares.
    ui.keys(b"\x1b[Z");
    ui.wait_for("Board, such as WEB:");
    ui.keys(b"\r");
    ui.wait_for("Fixture needs its Board, such as WEB.");
    ui.keys(b"WEB");
    ui.keys(b"\r");
    ui.wait_for("app's tickets now come from Fixture tickets, board WEB.");
    ui.wait_for("Fixture WEB");

    // The Tickets screen lists them, from the plugin.
    ui.keys(b"3");
    ui.wait_for("FIX-1 Make the button blue");
    ui.wait_for("FIX-2 Say hello");
    ui.wait_for("Its open tickets from Fixture board WEB");
}
