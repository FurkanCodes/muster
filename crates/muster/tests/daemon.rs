//! End-to-end tests: the real `muster` binary, speaking the real protocol over
//! its socket, in throwaway directories (`ARCHITECTURE.md` §13).

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use muster_client::{ClientError, FramedRead, FramedWrite};
use muster_core::protocol::{
    ClientFrame, Command, Delta, ErrorCode, Hello, PROTOCOL_VERSION, Reply, ServerFrame,
};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::time::timeout;

use support::{Daemon, Sandbox};

/// A connection that speaks frames directly, to test what the client library
/// never sends.
async fn raw(daemon: &Daemon) -> (FramedRead<OwnedReadHalf>, FramedWrite<OwnedWriteHalf>) {
    let (read, write) = UnixStream::connect(daemon.socket())
        .await
        .unwrap()
        .into_split();
    (FramedRead::new(read), FramedWrite::new(write))
}

async fn next(reader: &mut FramedRead<OwnedReadHalf>) -> Option<ServerFrame> {
    timeout(Duration::from_secs(5), reader.next())
        .await
        .expect("the daemon answered in time")
        .unwrap()
}

fn add(root: &std::path::Path) -> Command {
    Command::AddProject {
        root: root.to_owned(),
        name: None,
    }
}

#[tokio::test]
async fn the_handshake_names_the_daemon() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let welcome = connection.welcome().clone();
    assert_eq!(welcome.version, PROTOCOL_VERSION);
    assert_eq!(welcome.daemon_pid, daemon.pid());
    assert_eq!(
        connection.command(Command::Ping).await.unwrap(),
        Reply::Pong
    );
}

#[tokio::test]
async fn an_incompatible_client_is_told_the_daemons_range() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let (mut reader, mut writer) = raw(&daemon).await;
    let hello = Hello {
        min_version: PROTOCOL_VERSION + 1,
        max_version: PROTOCOL_VERSION + 9,
        client: "from the future".to_owned(),
        capabilities: Vec::new(),
    };
    writer.send(&ClientFrame::Hello(hello)).await.unwrap();
    let Some(ServerFrame::Incompatible(incompatible)) = next(&mut reader).await else {
        panic!("expected Incompatible");
    };
    assert_eq!(incompatible.max_version, PROTOCOL_VERSION);
    assert_eq!(
        next(&mut reader).await,
        None,
        "the daemon closes the connection"
    );
}

#[tokio::test]
async fn anything_before_hello_is_refused() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let (mut reader, mut writer) = raw(&daemon).await;
    writer.send(&ClientFrame::Subscribe).await.unwrap();
    let Some(ServerFrame::Error(error)) = next(&mut reader).await else {
        panic!("expected Error");
    };
    assert_eq!(error.code, ErrorCode::OutOfOrder);
    assert_eq!(next(&mut reader).await, None);
}

#[tokio::test]
async fn an_oversized_frame_closes_only_its_own_connection() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let (mut reader, mut writer) = raw(&daemon).await;
    writer
        .send(&ClientFrame::Hello(Hello::current("e2e")))
        .await
        .unwrap();
    assert!(matches!(
        next(&mut reader).await,
        Some(ServerFrame::Welcome(_))
    ));
    writer.send_bytes(&u32::MAX.to_le_bytes()).await.unwrap();
    let Some(ServerFrame::Error(error)) = next(&mut reader).await else {
        panic!("expected Error");
    };
    assert_eq!(error.code, ErrorCode::BadRequest);
    assert_eq!(next(&mut reader).await, None);

    let mut other = daemon.connect().await;
    assert_eq!(other.command(Command::Ping).await.unwrap(), Reply::Pong);
}

#[tokio::test]
async fn subscribers_get_a_snapshot_then_live_changes() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();

    let mut watcher = daemon.connect().await;
    assert!(watcher.subscribe().await.unwrap().projects.is_empty());

    let mut actor = daemon.connect().await;
    let Reply::ProjectAdded { id } = actor.command(add(&root)).await.unwrap() else {
        panic!("expected ProjectAdded");
    };

    let event = timeout(Duration::from_secs(5), watcher.next_event())
        .await
        .unwrap()
        .unwrap();
    let Some(ServerFrame::Delta(Delta::ProjectUpserted(project))) = event else {
        panic!("expected ProjectUpserted, got {event:?}");
    };
    assert_eq!((project.id, project.root.as_path()), (id, root.as_path()));

    let mut late = daemon.connect().await;
    assert_eq!(late.subscribe().await.unwrap().projects, [project]);
}

#[tokio::test]
async fn refused_commands_carry_an_error_code() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    connection.command(add(&root)).await.unwrap();

    let code = |result: Result<Reply, ClientError>| match result {
        Err(ClientError::Daemon(error)) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    };
    assert_eq!(
        code(connection.command(add(&root)).await),
        ErrorCode::Conflict
    );
    let missing = root.with_file_name("missing");
    assert_eq!(
        code(connection.command(add(&missing)).await),
        ErrorCode::BadRequest
    );
    // The connection stays usable after a refusal.
    assert_eq!(
        connection.command(Command::Ping).await.unwrap(),
        Reply::Pong
    );
}

#[tokio::test]
async fn state_survives_a_daemon_restart() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    daemon.connect().await.command(add(&root)).await.unwrap();
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(snapshot.projects.len(), 1);
    assert_eq!(snapshot.projects[0].root, root);
}

#[test]
fn a_second_daemon_refuses_to_start() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let second = sandbox.run(&["daemon"]);
    assert!(!second.status.success());
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already running"), "stderr: {stderr}");
    // The first daemon keeps its socket and still answers.
    assert!(support::answers_hello(daemon.socket()));
}

#[test]
fn the_socket_is_private_and_removed_on_stop() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let paths = sandbox.paths();
    let mode =
        |path: &std::path::Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&paths.runtime_dir), 0o700);
    assert_eq!(mode(&paths.socket()), 0o600);

    assert!(daemon.stop().success());
    assert!(!paths.socket().exists());
}

#[test]
fn a_socket_left_by_a_crashed_daemon_is_replaced() {
    let sandbox = Sandbox::new();
    let paths = sandbox.paths();
    std::fs::create_dir_all(&paths.runtime_dir).unwrap();
    // A socket file nobody listens on, as a killed daemon leaves behind.
    drop(std::os::unix::net::UnixListener::bind(paths.socket()).unwrap());

    let daemon = sandbox.start();
    let log = daemon.log();
    assert!(log.contains("removed a stale socket"), "daemon log:\n{log}");
}

#[test]
fn the_command_line_reports_status_and_adds_projects() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");

    let stopped = sandbox.run(&["status"]);
    assert!(!stopped.status.success());
    assert!(String::from_utf8_lossy(&stopped.stdout).contains("not running"));

    let _daemon = sandbox.start();
    let added = sandbox.run(&["project", "add", root.to_str().unwrap(), "--name", "App"]);
    assert!(
        added.status.success(),
        "{}",
        String::from_utf8_lossy(&added.stderr)
    );

    let status = sandbox.run(&["status"]);
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status.status.success());
    assert!(stdout.contains("The daemon is running"), "{stdout}");
    assert!(stdout.contains("1 projects"), "{stdout}");
    assert!(stdout.contains("App"), "{stdout}");

    let again = sandbox.run(&["project", "add", root.to_str().unwrap()]);
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("already a project"));
}
