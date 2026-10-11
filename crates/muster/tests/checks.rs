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

#[tokio::test]
async fn a_review_by_hand_is_claude_code_in_plan_mode_on_the_change() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    let added = sandbox.run(&["worktree", "add", "app", "--branch", "feature"]);
    assert!(added.status.success());

    let reviewed = sandbox.run(&["review", "feature"]);
    let said = String::from_utf8_lossy(&reviewed.stdout);
    assert!(
        reviewed.status.success(),
        "{}",
        String::from_utf8_lossy(&reviewed.stderr)
    );
    assert!(
        said.starts_with("Claude Code reviews the change on feature in session ses_"),
        "{said}"
    );
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let session = snapshot
        .sessions
        .iter()
        .find(|session| session.harness == "review")
        .unwrap();
    assert_eq!(session.title, "Review");
    let launch = sandbox.wait_for_launches(session.id, 1).remove(0);
    assert_eq!(launch.args[..2], ["--permission-mode", "plan"]);
    assert!(launch.value("--session-id").is_some());
    let prompt = launch.args.last().unwrap();
    assert!(prompt.contains("`git diff main...HEAD`"), "{prompt}");
    assert!(prompt.contains("do not change anything"), "{prompt}");
}

#[tokio::test]
async fn a_draft_pull_request_by_hand_pushes_the_branch_and_opens_or_finds_it() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let remote = root.parent().unwrap().join("remote.git");
    support::git(
        root.parent().unwrap(),
        &[
            "clone",
            "--bare",
            "--quiet",
            root.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    support::git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        sandbox
            .run(&["worktree", "add", "app", "--branch", "feature"])
            .status
            .success()
    );
    let tree = sandbox.root().join("worktrees/app/feature");
    std::fs::write(tree.join("fix.txt"), "fixed\n").unwrap();
    support::git(&tree, &["add", "fix.txt"]);
    support::git(
        &tree,
        &["commit", "--quiet", "--message", "Fix the login redirect"],
    );

    let opened = sandbox.run(&["pr", "feature"]);
    let said = String::from_utf8_lossy(&opened.stdout);
    assert!(
        opened.status.success(),
        "{}",
        String::from_utf8_lossy(&opened.stderr)
    );
    assert_eq!(
        said,
        "Draft pull request #17: https://github.com/o/app/pull/17\n"
    );
    assert_eq!(
        support::git(&remote, &["log", "-1", "--format=%s", "feature"]),
        "Fix the login redirect"
    );
    let calls = std::fs::read_to_string(sandbox.fake_gh().join("calls")).unwrap();
    assert!(
        calls.contains(
            "pr create --draft --base main --head feature --title Fix the login redirect"
        ),
        "{calls}"
    );
    // Asked again, it finds the one open rather than opening another.
    let again = sandbox.run(&["pr", "feature"]);
    assert!(again.status.success());
    let calls = std::fs::read_to_string(sandbox.fake_gh().join("calls")).unwrap();
    assert_eq!(calls.matches("pr create").count(), 1, "{calls}");

    // The branch a pull request would merge into is refused.
    let base = sandbox.run(&["pr", "app"]);
    assert!(!base.status.success());
    let said = String::from_utf8_lossy(&base.stderr);
    assert!(
        said.contains("main is the branch a pull request would merge into"),
        "{said}"
    );
}
