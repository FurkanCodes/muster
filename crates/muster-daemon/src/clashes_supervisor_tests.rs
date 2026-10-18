//! Where the work in a project's workspaces meets, kept by the Supervisor
//! on each workspace (`PRODUCT.md` §6): found once both have work, and
//! gone once one has none.

use std::path::PathBuf;
use std::time::Duration;

use muster_core::Workspace;
use muster_core::protocol::Delta;
use tokio::sync::broadcast;
use tokio::time::timeout;

use crate::pulls_tests::{Setup, setup};

/// The next time the clashes shown on a workspace meet `wanted`.
async fn shown(
    deltas: &mut broadcast::Receiver<Delta>,
    wanted: impl Fn(&Workspace) -> bool,
) -> Workspace {
    timeout(Duration::from_secs(20), async {
        loop {
            if let Delta::WorkspaceUpserted(shown) = deltas.recv().await.unwrap()
                && wanted(&shown)
            {
                return shown;
            }
        }
    })
    .await
    .expect("the clashes were never shown")
}

#[tokio::test]
async fn a_worktree_and_the_checkout_changing_one_line_are_shown_to_conflict_until_one_stops() {
    // The repository lives as long as the test.
    let Setup {
        _dir,
        supervisor,
        worktree,
        ..
    } = setup().await;
    let (snapshot, mut deltas) = supervisor.subscribe().await.unwrap();
    let path = |id| {
        snapshot
            .workspaces
            .iter()
            .find(|found: &&Workspace| found.id == id)
            .and_then(|found| found.kind.host_path().map(PathBuf::from))
            .unwrap()
    };
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|found| found.id != worktree)
        .unwrap()
        .id;
    std::fs::write(path(worktree).join("README.md"), "test\nby the agent\n").unwrap();
    std::fs::write(path(checkout).join("README.md"), "test\nby the person\n").unwrap();

    assert!(supervisor.check_pull_requests().await);
    let found = shown(&mut deltas, |shown| {
        shown.id == worktree && !shown.clashes.is_empty()
    })
    .await;
    assert_eq!(found.clashes.len(), 1);
    let clash = &found.clashes[0];
    assert_eq!(clash.with, checkout);
    assert_eq!((clash.files, clash.conflicts), (1, 1));
    assert_eq!(clash.shown[0].path, "README.md");
    // The checkout's, told in the same breath.
    let (now, _) = supervisor.subscribe().await.unwrap();
    let theirs = now
        .workspaces
        .iter()
        .find(|found| found.id == checkout)
        .unwrap();
    assert_eq!(theirs.clashes.len(), 1);
    assert_eq!(theirs.clashes[0].with, worktree);

    // The agent's change goes: nothing meets any more.
    std::fs::write(path(worktree).join("README.md"), "test\n").unwrap();
    assert!(supervisor.check_pull_requests().await);
    shown(&mut deltas, |shown| {
        shown.id == checkout && shown.clashes.is_empty()
    })
    .await;
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert!(
        snapshot
            .workspaces
            .iter()
            .all(|workspace| workspace.clashes.is_empty())
    );
}
