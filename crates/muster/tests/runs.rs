//! End-to-end tests for runs from the command line (M2): `muster run`
//! against the real daemon, with stand-ins for `gh` and `docker`. The
//! stand-in Docker is not running, so a run stops for the person at its
//! first piece of work, which is enough to follow, retry, survive a
//! restart and cancel it.

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

use std::path::PathBuf;
use std::process::Output;

use muster_core::protocol::{Command, Reply};
use muster_core::{Run, RunStage, RunTransition};
use serde_json::json;

use support::{Daemon, Sandbox};

/// A sandbox whose project `app` has checks and issue #42.
fn sandbox() -> (Sandbox, PathBuf) {
    // A named image, so the daemon builds none.
    let sandbox = Sandbox::new().with_env("MUSTER_RUN_IMAGE", "muster-e2e:absent");
    let root = sandbox.repository("app");
    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"true\"]\n",
    )
    .unwrap();
    std::fs::write(
        sandbox.fake_gh().join("issue-42.json"),
        json!({"number": 42, "title": "Fix the login redirect", "url": "https://github.com/o/app/issues/42",
               "labels": [{"name": "bug"}], "author": {"login": "someone"},
               "body": "After signing in, users land on /404."})
        .to_string(),
    )
    .unwrap();
    (sandbox, root)
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Runs `muster` and expects it to succeed; returns what it printed.
fn ok(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = sandbox.run(args);
    assert!(
        output.status.success(),
        "`muster {}` failed: {}",
        args.join(" "),
        stderr(&output)
    );
    stdout(&output)
}

/// Runs `muster` and expects it to fail; returns its complaint.
fn refused(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = sandbox.run(args);
    assert!(
        !output.status.success(),
        "`muster {}` succeeded: {}",
        args.join(" "),
        stdout(&output)
    );
    stderr(&output)
}

/// Waits until the only run, and its timeline, satisfy `done`.
async fn wait_for_run(
    daemon: &Daemon,
    mut done: impl FnMut(&Run, &[RunTransition]) -> bool,
) -> (Run, Vec<RunTransition>) {
    let mut connection = daemon.connect().await;
    wait::found_async("the run wanted", async || {
        let snapshot = connection.subscribe().await.unwrap();
        let [run] = snapshot.runs.as_slice() else {
            return None;
        };
        let Reply::RunHistory { transitions } = connection
            .command(Command::RunHistory { run: run.id })
            .await
            .unwrap()
        else {
            panic!("expected RunHistory");
        };
        done(run, &transitions).then(|| (run.clone(), transitions))
    })
    .await
}

#[tokio::test]
async fn a_run_is_followed_retried_kept_over_a_restart_and_cancelled() {
    let (sandbox, root) = sandbox();
    let daemon = sandbox.start();
    ok(&sandbox, &["project", "add", root.to_str().unwrap()]);

    let started = ok(&sandbox, &["run", "start", "app", "42"]);
    assert!(
        started.starts_with("Picked up issue #42 in app as run run_"),
        "{started}"
    );
    let (run, _) = wait_for_run(&daemon, |run, _| run.stage == RunStage::NeedsYou).await;
    let reason = run.reason.clone().unwrap();
    assert!(reason.starts_with("Docker is not running"), "{reason}");
    assert_eq!(run.branch, "muster/issue-42-fix-the-login-redirect");
    // The daemon asked Docker for the run's container, and never handed
    // it a value through its arguments.
    let calls = sandbox.docker_calls();
    assert!(
        calls.contains(&format!(
            "ps --all --no-trunc --filter label=muster.run={}",
            run.id
        )),
        "{calls}"
    );

    // Listed and shown, waiting on the person, with what they can do.
    let list = ok(&sandbox, &["run", "list"]);
    let first = list.lines().next().unwrap();
    for part in ["needs you", "#42", "app", "Fix the login redirect", "0/3"] {
        assert!(first.contains(part), "{part:?} in {list}");
    }
    assert!(first.ends_with(&run.id.to_string()), "{list}");
    assert!(list.contains("└ Docker is not running"), "{list}");
    let shown = ok(&sandbox, &["run", "show", "#42"]);
    for part in [
        "Issue #42 in app: Fix the login redirect",
        "stage         needs you: Docker is not running",
        "attempt       none yet, 3 allowed",
        "starting   there is room to run",
        "needs you  Docker is not running",
        "`muster run retry 42 [--notes …]`",
    ] {
        assert!(shown.contains(part), "{part:?} in\n{shown}");
    }
    let status = ok(&sandbox, &["status"]);
    assert!(status.contains("1 run going"), "{status}");
    assert!(status.contains("  run #42  needs you: Docker"), "{status}");

    // Only a run ready for the person takes changes.
    let early = refused(
        &sandbox,
        &["run", "changes", "42", "Handle the empty case."],
    );
    assert!(
        early.contains("cannot take a request for changes while it is waiting on you"),
        "{early}"
    );

    // A retry goes back to work, and stops again at the same place.
    let retried = ok(
        &sandbox,
        &[
            "run",
            "retry",
            &run.id.to_string(),
            "--notes",
            "Docker is up now",
        ],
    );
    assert_eq!(
        retried,
        "Sent the run on #42 back to work, with a fresh budget.\n"
    );
    let (_, timeline) = wait_for_run(&daemon, |run, timeline| {
        run.stage == RunStage::NeedsYou && timeline.len() == 4
    })
    .await;
    assert_eq!(timeline[2].reason, "you sent it back to work");

    // The run, its timeline and its reason outlive the daemon (`PRD-09`).
    drop(daemon);
    let daemon = sandbox.start();
    let (kept, timeline) = wait_for_run(&daemon, |_, _| true).await;
    assert_eq!(kept.stage, RunStage::NeedsYou);
    assert_eq!(kept.reason.as_deref(), Some(reason.as_str()));
    assert_eq!(timeline.len(), 4);

    let cancelled = ok(&sandbox, &["run", "cancel", "42"]);
    assert_eq!(
        cancelled,
        "Cancelled the run on #42; its container goes, its record stays.\n"
    );
    let (ended, _) = wait_for_run(&daemon, |run, _| run.stage == RunStage::Cancelled).await;
    assert!(ended.ended_at.is_some());
    assert_eq!(
        ok(&sandbox, &["run", "list"]),
        "No runs going. `muster run list --all` shows the ended ones too.\n"
    );
    let all = ok(&sandbox, &["run", "list", "--all"]);
    assert!(all.starts_with("cancelled  #42"), "{all}");
    let again = refused(&sandbox, &["run", "cancel", "42"]);
    assert!(again.contains("the run on #42 has ended"), "{again}");
    let shown = ok(&sandbox, &["run", "show", "42"]);
    assert!(shown.contains("time          ended "), "{shown}");
    assert!(!shown.contains("muster run retry"), "{shown}");
    drop(daemon);
}

#[tokio::test]
async fn runs_say_plainly_what_stops_them_from_starting() {
    let (sandbox, root) = sandbox();
    let _daemon = sandbox.start();
    ok(&sandbox, &["project", "add", root.to_str().unwrap()]);

    let missing = refused(&sandbox, &["run", "start", "app", "7"]);
    assert!(
        missing.contains("Could not resolve to an Issue with the number of 7"),
        "{missing}"
    );
    let unknown = refused(&sandbox, &["run", "start", "elsewhere", "42"]);
    assert!(
        unknown.contains("no project matches \"elsewhere\""),
        "{unknown}"
    );
    let none = refused(&sandbox, &["run", "show", "42"]);
    assert!(none.contains("no run on issue #42"), "{none}");
    let garbled = refused(&sandbox, &["run", "retry", "fix-it"]);
    assert!(
        garbled.contains("neither a run's id nor an issue's number"),
        "{garbled}"
    );
    assert_eq!(
        ok(&sandbox, &["run", "list"]),
        "No runs yet. `muster run start <project> <issue>` hands an issue to the autopilot.\n"
    );

    // Without checks there is nothing to stop unverified work (`PRD-02`).
    std::fs::write(root.join("muster.toml"), "").unwrap();
    let unchecked = refused(&sandbox, &["run", "start", "app", "42"]);
    assert!(unchecked.contains("runs need checks"), "{unchecked}");
    assert!(sandbox.docker_calls().is_empty());
}

#[tokio::test]
async fn a_worktree_s_committed_work_is_handed_to_the_autopilot() {
    let (sandbox, root) = sandbox();
    // The settings are the project's, committed: the checkout is clean.
    support::git(&root, &["add", "muster.toml"]);
    support::git(&root, &["commit", "--quiet", "--message", "Runs"]);
    let daemon = sandbox.start();
    ok(&sandbox, &["project", "add", root.to_str().unwrap()]);
    ok(
        &sandbox,
        &["workspace", "add", "-p", "app", "--branch", "by-hand"],
    );
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let tree = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.kind.branch() == "by-hand")
        .and_then(|workspace| workspace.kind.host_path())
        .unwrap()
        .to_owned();

    std::fs::write(tree.join("fix.txt"), "by hand\n").unwrap();
    let dirty = refused(
        &sandbox,
        &["run", "start", "app", "42", "--from", "by-hand"],
    );
    assert!(
        dirty.contains("1 uncommitted change(s); commit them first"),
        "{dirty}"
    );
    support::git(&tree, &["add", "fix.txt"]);
    support::git(&tree, &["commit", "--quiet", "--message", "By hand"]);

    let started = ok(
        &sandbox,
        &["run", "start", "app", "42", "--from", "by-hand"],
    );
    assert!(
        started.starts_with("Handed the work on by-hand to the autopilot as run run_"),
        "{started}"
    );
    // The branch reached Muster's copy before Docker was asked for a
    // container (which is not running here).
    let (run, _) = wait_for_run(&daemon, |run, _| run.stage == RunStage::NeedsYou).await;
    assert_eq!(run.branch, "by-hand");
    assert!(run.reason.unwrap().starts_with("Docker is not running"));
    let shown = ok(&sandbox, &["run", "show", "42"]);
    assert!(shown.contains("branch        by-hand"), "{shown}");
    let base = refused(
        &sandbox,
        &["run", "start", "app", "7", "--from", "elsewhere"],
    );
    assert!(base.contains("no workspace is \"elsewhere\""), "{base}");
}
