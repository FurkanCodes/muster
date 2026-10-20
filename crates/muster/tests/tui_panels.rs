//! End-to-end tests of a plugin's panel in the terminal UI (`PRODUCT.md`
//! §6, `PRD-15`): the real `muster` binary on a real pseudo-terminal, with
//! a panel plugin installed from a `file://` git repository. Its key opens
//! it beside the session selected; a click gives it the keys and Shift-Q
//! or a click elsewhere takes them back; its key again puts it away and
//! frees its place.

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

use std::os::unix::fs::PermissionsExt as _;

use support::{Sandbox, git};
use ui::{StopDaemon, Ui};

const MANIFEST: &str = r#"id = "zoe"
name = "Zoe"
version = "0.1.0"
muster = "0.3.0"

[[panels]]
id = "context"
title = "context"
on = "session"
command = ["./context", "{branch}"]
key = "Z"
"#;

/// Installs the panel plugin from a git repository the test makes: its
/// program says what it was told, then waits, ignoring what is typed.
fn install(sandbox: &Sandbox) {
    let root = sandbox.repository("zoe-plugin");
    std::fs::write(root.join("muster-plugin.toml"), MANIFEST).unwrap();
    let program = root.join("context");
    std::fs::write(
        &program,
        "#!/bin/sh\nstty -echo\necho \"panel-on-$1\"\nexec sleep 60\n",
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    git(&root, &["add", "muster-plugin.toml", "context"]);
    git(&root, &["commit", "--quiet", "--message", "the plugin"]);
    let address = format!("file://{}", root.display());
    let output = sandbox
        .command()
        .args(["plugin", "install", &address, "--yes"])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn its_key_opens_a_panel_beside_the_session_and_the_keys_move_between_them() {
    let sandbox = Sandbox::new();
    let _stop = StopDaemon(sandbox.paths().socket());
    let _daemon = sandbox.start();
    install(&sandbox);
    let root = sandbox.repository("app");
    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.keys(b"\r");
    ui.wait_for("⌂ main");
    // A shell in the checkout, then back on the list, its line selected.
    ui.keys(b"n");
    ui.wait_for("New session");
    ui.keys(b"\x1b[Z");
    ui.keys(b"\r");
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_without("Shift-Q back to the list");

    // Its key: the panel beside the session (in this window, over the
    // list), the keys still the list's.
    ui.keys(b"Z");
    ui.wait_for("panel-on-main");
    ui.wait_for("┌ context ·");
    ui.wait_for("Shift-Z context");
    let (column, row) = ui.position_of("panel-on-main").unwrap();
    let (prompt, _) = ui.position_of("$ ").or(ui.position_of("% ")).unwrap();
    assert!(column > prompt, "beside the session");
    // A click gives it the keys; Shift-Q takes them back.
    ui.click(column, row);
    ui.wait_for("Shift-Q back to the list");
    ui.keys(b"Q");
    ui.wait_without("Shift-Q back to the list");
    // A click on it, then one on the session: the session has them, and
    // what is typed reaches the shell.
    ui.click(column, row);
    ui.wait_for("Shift-Q back to the list");
    let (left, top) = ui.position_of("$ ").or(ui.position_of("% ")).unwrap();
    ui.click(left, top);
    ui.keys(b"echo typed-$((2*3))\r");
    ui.wait_for("typed-6");
    ui.keys(b"Q");
    ui.wait_without("Shift-Q back to the list");

    // Its key again: put away, its place freed and the list back.
    ui.keys(b"Z");
    ui.wait_without("panel-on-main");
    ui.wait_for("┌ projects");
    assert!(!ui.text().contains("┌ context"), "{}", ui.text());
}
