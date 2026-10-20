//! End-to-end test of the Settings screen's Plugins section (`PRODUCT.md`
//! §6, `ARCHITECTURE.md` §7a, `PRD-15`): the real `muster` binary on a
//! real pseudo-terminal adds a plugin from a `file://` git repository the
//! test makes, after showing everything it would run; turns it off and on;
//! lists one installed from the command line as it comes, marked; and
//! removes it after asking. Nothing of a plugin is run.

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
use std::process::Stdio;

use support::{Sandbox, git};
use ui::{StopDaemon, Ui};

/// A plugin with a panel and a ticket source, whose programs would leave a
/// mark if anything ran them.
fn manifest(id: &str) -> String {
    format!(
        r#"id = "{id}"
name = "{id}"
version = "0.1.0"
description = "A plugin for the test."
muster = "0.3.0"
requires = ["sh"]

[[panels]]
id = "graph"
title = "graph"
command = ["./graph", "{{branch}}"]
key = "Z"

[tickets]
name = "Test tickets"
noun = "task"
command = ["./tickets"]
"#
    )
}

/// A git repository holding the plugin `id`.
fn plugin_repository(sandbox: &Sandbox, id: &str) -> PathBuf {
    let root = sandbox.repository(&format!("{id}-plugin"));
    std::fs::write(root.join("muster-plugin.toml"), manifest(id)).unwrap();
    let ran = sandbox.root().join("ran");
    for program in ["graph", "tickets"] {
        std::fs::write(
            root.join(program),
            format!("#!/bin/sh\ntouch '{}'\n", ran.display()),
        )
        .unwrap();
    }
    git(&root, &["add", "."]);
    git(&root, &["commit", "--quiet", "--message", "the plugin"]);
    root
}

fn settings(sandbox: &Sandbox) -> String {
    std::fs::read_to_string(sandbox.paths().config_dir.join("settings.toml")).unwrap_or_default()
}

#[test]
fn a_plugin_is_added_turned_off_and_on_and_removed_on_the_settings_screen() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let zoe = plugin_repository(&sandbox, "zoe");
    let other = plugin_repository(&sandbox, "other");
    let _stop = StopDaemon(sandbox.paths().socket());

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"4");
    ui.wait_for("+ add a plugin…");

    // a: an address, then everything it would run, then yes.
    ui.keys(b"a");
    ui.wait_for("Add a plugin");
    ui.keys(format!("file://{}", zoe.display()).as_bytes());
    ui.keys(b"\r");
    ui.wait_for(" Install zoe? ");
    ui.wait_for("./graph {branch}");
    ui.wait_for("./tickets");
    ui.wait_for("y install   n no");
    ui.keys(b"y");
    ui.wait_for("Installed zoe and turned it on.");
    ui.wait_for("● zoe");
    ui.wait_for("panel: graph (Z)");
    assert!(settings(&sandbox).contains("[plugins.zoe]"));

    // Space: off, and on again.
    ui.keys(b" ");
    ui.wait_for("Turned zoe off.");
    ui.wait_for("○ zoe");
    assert!(settings(&sandbox).contains("enabled = false"));
    ui.keys(b" ");
    ui.wait_for("Turned zoe on.");
    ui.wait_for("● zoe");

    // One installed from the command line is listed as it comes, marked.
    let output = sandbox
        .command()
        .args([
            "plugin",
            "install",
            &format!("file://{}", other.display()),
            "--yes",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    ui.wait_for("● other");
    ui.wait_for("from the command line");

    // x: removed after asking.
    ui.keys(b"x");
    ui.wait_for("Remove zoe?");
    ui.keys(b"y");
    ui.wait_for("Removed zoe.");
    ui.wait_without("● zoe");
    assert!(!settings(&sandbox).contains("[plugins.zoe]"));
    assert!(
        !sandbox.root().join("ran").exists(),
        "nothing of a plugin runs"
    );
}
