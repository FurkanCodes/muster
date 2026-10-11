//! End-to-end tests for the checks by hand (M2): `muster check` runs the
//! project's checks in a workspace as a session, shows their output, and
//! exits as they did; starting the session again runs them again.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use muster_core::SessionStatus;
use muster_core::protocol::{Command, Reply, TermSize};

use support::Sandbox;

#[tokio::test]
async fn the_checks_run_in_a_workspace_show_their_output_and_say_how_they_went() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let added = sandbox.run(&["project", "add", root.to_str().unwrap()]);
    assert!(added.status.success());

    // None named yet: said plainly.
    let none = sandbox.run(&["check", "app"]);
    assert!(!none.status.success());
    let said = String::from_utf8_lossy(&none.stderr);
    assert!(said.contains("the project names no checks"), "{said}");

    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"echo checking-$((6*7))\", \"exit 3\", \"echo never\"]\n",
    )
    .unwrap();
    let failed = sandbox.run(&["check", "app"]);
    let shown = String::from_utf8_lossy(&failed.stdout);
    assert!(!failed.status.success(), "{shown}");
    for part in [
        "$ echo checking-$((6*7))",
        "checking-42",
        "$ exit 3",
        "(exit 3)",
        "The checks failed.",
    ] {
        assert!(shown.contains(part), "{part:?} in {shown}");
    }
    assert!(
        !shown.contains("never"),
        "it stops at the first failure: {shown}"
    );

    // The run is a session, and starting it again runs the checks as they
    // are now.
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    let checks = snapshot
        .sessions
        .iter()
        .find(|session| session.harness == "checks")
        .unwrap();
    assert_eq!(checks.title, "Checks");
    assert_eq!(checks.status, SessionStatus::Exited { success: false });
    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"true\"]\n",
    )
    .unwrap();
    let restarted = connection
        .command(Command::RestartSession {
            session: checks.id,
            size: TermSize::default(),
        })
        .await
        .unwrap();
    assert_eq!(restarted, Reply::SessionStarted { id: checks.id });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        let status = snapshot
            .sessions
            .iter()
            .find(|session| session.id == checks.id)
            .unwrap()
            .status;
        if status == (SessionStatus::Exited { success: true }) {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "still {status:?}");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let passed = sandbox.run(&["check", "app"]);
    assert!(passed.status.success());
    assert!(String::from_utf8_lossy(&passed.stdout).contains("All checks passed."));
}
