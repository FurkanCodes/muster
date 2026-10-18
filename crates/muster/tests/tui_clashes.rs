//! End-to-end test of where the work in a project's workspaces meets, in
//! the terminal UI (`PRODUCT.md` §6): the real `muster` binary on a real
//! pseudo-terminal, a worktree and the checkout changing the same line,
//! the worktree's line saying they would conflict, its details naming the
//! checkout and the file, and its changes pane showing the other work
//! first.

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

use muster_client::Connection;
use muster_core::WorkspaceKind;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame};

use support::{Sandbox, expect_event};
use ui::{StopDaemon, Ui};

/// A worktree on `work` in the project the UI added, changing the line
/// the person changes in the checkout at `root`, both looked at.
async fn both_change_one_line(sandbox: &Sandbox, root: &Path) {
    let mut connection = Connection::connect(&sandbox.paths().socket(), "muster e2e")
        .await
        .unwrap();
    let snapshot = connection.subscribe().await.unwrap();
    let project = snapshot.projects[0].id;
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|found| matches!(found.kind, WorkspaceKind::Checkout { .. }))
        .unwrap()
        .id;
    let Reply::WorktreeCreated { id: worktree } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let path = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown)) if shown.id == worktree => {
            shown.kind.host_path().map(Path::to_path_buf)
        }
        _ => None,
    })
    .await;
    std::fs::write(root.join("README.md"), "test\nby the person\n").unwrap();
    std::fs::write(path.join("README.md"), "test\nby the agent\n").unwrap();
    for workspace in [checkout, worktree] {
        connection
            .command(Command::WorkspaceChanges { workspace })
            .await
            .unwrap();
    }
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown))
            if shown.id == worktree && !shown.clashes.is_empty() =>
        {
            Some(())
        }
        _ => None,
    })
    .await;
}

#[test]
fn a_worktree_whose_work_would_conflict_with_the_checkout_says_so() {
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
        .block_on(both_change_one_line(&sandbox, &root));

    // Both lines say so; the worktree's details name the checkout.
    ui.wait_for("⎇ work");
    ui.wait_for("conflict +1");
    ui.keys(b"\x1b[B\x1b[B");
    ui.wait_for("Clashes ⌂ main would conflict in README.md");

    // Its changes, the other work first.
    ui.keys(b"d");
    ui.wait_for("Other work on these files · 1");
    ui.wait_for("⌂ main  would conflict in README.md");
    ui.wait_for("Not committed · 1 file");
}
