//! End-to-end tests for resource use (M1 step 10): what each session's
//! process tree uses, measured by the daemon and sent to clients.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use muster_client::Connection;
use muster_core::SessionId;
use muster_core::protocol::{Command, Delta, Reply, ResourceReport, ServerFrame, TermSize};

use support::{Daemon, Sandbox, expect_event};

/// A daemon that measures every 100 ms, with one shell session.
async fn shell_session() -> (Sandbox, Daemon, Connection, SessionId) {
    let sandbox = Sandbox::new().with_env("MUSTER_SAMPLE_MS", "100");
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
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    connection.subscribe().await.unwrap();
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    (sandbox, daemon, connection, session)
}

/// Waits for a report that `wanted` accepts.
async fn expect_report(
    connection: &mut Connection,
    wanted: impl Fn(&ResourceReport) -> bool,
) -> ResourceReport {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::Resources(report)) if wanted(report) => Some(report.clone()),
        _ => None,
    })
    .await
}

fn memory_of(report: &ResourceReport, session: SessionId) -> Option<u64> {
    report
        .sessions
        .iter()
        .find(|usage| usage.session == session)
        .map(|usage| usage.memory_bytes)
}

#[tokio::test]
async fn a_running_session_and_the_host_are_measured() {
    let (_sandbox, daemon, mut connection, session) = shell_session().await;
    let report = expect_report(&mut connection, |report| {
        memory_of(report, session).is_some()
    })
    .await;
    assert!(memory_of(&report, session).unwrap() > 0);
    assert!(report.host_memory_total >= report.host_memory_used);
    assert!(report.host_memory_used > 0);

    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert!(
        snapshot.resources.is_some(),
        "a new client gets the latest report"
    );
}

#[tokio::test]
async fn what_a_session_starts_counts_toward_it() {
    let (_sandbox, _daemon, mut connection, session) = shell_session().await;
    // A child holding about 200 MB.
    connection
        .input(session, b"perl -e '$x = \"a\" x 200_000_000; sleep 60' &\n")
        .await
        .unwrap();
    let report = expect_report(&mut connection, |report| {
        memory_of(report, session).is_some_and(|memory| memory > 150_000_000)
    })
    .await;
    assert!(memory_of(&report, session).unwrap() > 150_000_000);
}

#[tokio::test]
async fn an_ended_session_is_no_longer_measured() {
    let (_sandbox, _daemon, mut connection, session) = shell_session().await;
    expect_report(&mut connection, |report| {
        memory_of(report, session).is_some()
    })
    .await;
    connection
        .command(Command::StopSession { session })
        .await
        .unwrap();
    expect_report(&mut connection, |report| {
        memory_of(report, session).is_none()
    })
    .await;
}
