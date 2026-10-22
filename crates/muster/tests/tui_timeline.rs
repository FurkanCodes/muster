//! End-to-end test of a workspace's timeline in the terminal UI
//! (`PRODUCT.md` §6, `PRD-14`): the real `muster` binary on a real
//! pseudo-terminal, a stand-in Claude Code's turn kept by its daemon,
//! listed under `d`, and its files put back with `z` once the person says
//! yes.

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

use std::path::Path;
use std::time::Duration;

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use serde_json::json;

use support::{Sandbox, expect_event};
use ui::{StopDaemon, Ui};

/// A turn of a stand-in Claude Code in the project's checkout at `root`,
/// told to the daemon the UI started: it adds a line to the README.
async fn a_turn(sandbox: &Sandbox, root: &Path) {
    let mut connection = Connection::connect(&sandbox.paths().socket(), "muster e2e")
        .await
        .unwrap();
    let snapshot = connection.subscribe().await.unwrap();
    let workspace = snapshot.workspaces[0].id;
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str| {
        launch.fire(
            event,
            &json!({ "session_id": id, "hook_event_name": event, "cwd": "/work" }),
        );
    };
    fire("UserPromptSubmit");
    let open = sandbox
        .paths()
        .data_dir
        .join("timelines")
        .join(workspace.to_string())
        .join("refs/open");
    let started = (0..200).any(|_| {
        let found = std::fs::read_dir(&open).is_ok_and(|mut entries| entries.next().is_some());
        if !found {
            std::thread::sleep(Duration::from_millis(25));
        }
        found
    });
    assert!(started, "the turn's start was never kept");
    std::fs::write(root.join("README.md"), "test\nby the agent\n").unwrap();
    fire("Stop");
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown)) if shown.id == workspace => {
            shown.timeline
        }
        _ => None,
    })
    .await;
}

#[test]
fn z_puts_a_turn_s_files_back_once_the_person_says_yes() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _stop = StopDaemon(sandbox.paths().socket());

    let mut ui = Ui::start(&sandbox, &root);
    ui.wait_for("No projects yet");
    ui.keys(b"a");
    ui.wait_for("Add a project");
    ui.keys(b"\r");
    ui.wait_for("⌂ main");
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(a_turn(&sandbox, &root));

    // The checkout's changes, with its turn under them.
    ui.keys(b"\x1b[B");
    ui.wait_for("d changes");
    ui.keys(b"d");
    ui.wait_for("Not committed · 1 file");
    ui.wait_for("Turns · 1");
    ui.wait_for("#1  Claude Code · turn 1");

    ui.keys(b"\x1b[B");
    ui.wait_for("z put back as before it");
    ui.keys(b"z");
    ui.wait_for("Put the files back?");
    ui.wait_for("Claude Code still remembers that turn");
    ui.keys(b"y");
    ui.wait_for("The files are as they were before #1");
    assert_eq!(
        std::fs::read_to_string(root.join("README.md")).unwrap(),
        "test\n"
    );
    // What was there is an entry of its own, to undo in turn.
    ui.wait_for("#2  Put back as before #1");
    ui.wait_without("Not committed · 1 file");
}
