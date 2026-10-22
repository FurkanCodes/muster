//! End-to-end test of tidying a branch's commits (`PRODUCT.md` §6,
//! `PRD-14`): the agent the person picked, a stand-in Claude Code, is
//! started with what to tidy; Muster refuses another tidy while it works
//! and one over changes not committed, and once it stops says the files
//! are as they began, from the real daemon on a real repository.

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

use std::path::Path;

use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::tidy::TidyCheck;
use serde_json::json;

use support::{Sandbox, expect_event, git};

fn commit(dir: &Path, text: &str, message: &str) {
    std::fs::write(dir.join("notes.txt"), text).unwrap();
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "--quiet", "-m", message]);
}

#[tokio::test]
async fn an_agent_tidies_the_commits_not_pushed_and_muster_checks_the_files() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id: workspace } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let path = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .and_then(|found| found.kind.host_path())
        .unwrap()
        .to_owned();
    commit(&path, "one\n", "wip");
    commit(&path, "one\ntwo\n", "more wip");
    commit(&path, "one\ntwo\nthree\n", "fixup");
    let tidy = |workspace| Command::TidyCommits {
        workspace,
        harness: "claude".to_owned(),
        size: TermSize::default(),
    };

    // Changes not committed are not tidied over.
    std::fs::write(path.join("README.md"), "not committed\n").unwrap();
    let refused = connection.command(tidy(workspace)).await.unwrap_err();
    assert!(refused.to_string().contains("not committed"), "{refused}");
    git(&path, &["checkout", "--", "README.md"]);

    let Reply::SessionStarted { id: session } = connection.command(tidy(workspace)).await.unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    assert!(
        launch
            .args
            .iter()
            .any(|arg| arg.contains("Tidy the 3 newest commits on the branch `work`")),
        "{:?}",
        launch.args
    );
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str| {
        launch.fire(
            event,
            &json!({ "session_id": id, "hook_event_name": event, "cwd": "/work" }),
        );
    };
    fire("UserPromptSubmit");

    // Not twice at once.
    let busy = connection.command(tidy(workspace)).await.unwrap_err();
    assert!(
        matches!(&busy, muster_client::ClientError::Daemon(body) if body.code == ErrorCode::Conflict),
        "{busy}"
    );

    // The agent squashes the three into one.
    git(&path, &["reset", "--quiet", "--soft", "HEAD~3"]);
    git(&path, &["commit", "--quiet", "-m", "Notes in three lines"]);
    fire("Stop");
    let checked = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionUpserted(shown)) if shown.id == session => {
            shown.tidy.as_ref().and_then(|tidy| tidy.checked.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(checked, TidyCheck::Kept { commits: 1 });
}
