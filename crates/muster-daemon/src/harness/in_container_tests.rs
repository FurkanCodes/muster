//! The script that runs sessions' programs in a run's container, against
//! the real Docker (`MUSTER_DOCKER_TESTS=1`).

use std::process::Command;
use std::time::Duration;

use super::{IN_CONTAINER, START_IN_CONTAINER, STOP_IN_CONTAINER};
use crate::containers::Docker;
use crate::runs::job::tests::container;
use crate::waiting;

const QUICK: Duration = Duration::from_secs(60);

/// Starts session `session`'s shell as `docker exec --tty` does for a
/// person, detached, with the session in its environment.
fn start_shell(container: &str, session: &str) {
    let script = format!("{IN_CONTAINER}{START_IN_CONTAINER}");
    let status = Command::new("docker")
        .args(["exec", "--detach", "--tty", "--env"])
        .arg(format!("MUSTER_SESSION={session}"))
        .args([container, "sh", "-c", &script, "sh", session, "shell", ""])
        .status()
        .unwrap();
    assert!(status.success());
}

async fn sh(docker: &Docker, container: &str, script: &str) -> String {
    let output = docker
        .exec(container, &["sh", "-c", script], &[], QUICK)
        .await
        .unwrap();
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

async fn alive(docker: &Docker, container: &str, pid: &str) -> bool {
    sh(
        docker,
        container,
        &format!("kill -0 {pid} 2>/dev/null && echo yes"),
    )
    .await
        == "yes"
}

/// The pid recorded for `session`, once it is there.
async fn recorded(docker: &Docker, container: &str, session: &str) -> String {
    waiting::found(&format!("a pid recorded for {session}"), async || {
        let pid = sh(
            docker,
            container,
            &format!("cat /tmp/muster/sessions/{session}.pid 2>/dev/null"),
        )
        .await;
        (!pid.is_empty()).then_some(pid)
    })
    .await
}

async fn stop(docker: &Docker, container: &str, session: &str) {
    let script = format!("{IN_CONTAINER}{STOP_IN_CONTAINER}");
    docker
        .exec(container, &["sh", "-c", &script, "sh", session], &[], QUICK)
        .await
        .unwrap();
}

#[tokio::test]
async fn real_docker_stops_a_session_s_program_and_nothing_that_reused_its_pid() {
    if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
        return;
    }
    let docker = Docker::default();
    let guard = container(&docker).await;
    let name = guard.0.as_str();

    // The program is recorded, and found again to be stopped.
    start_shell(name, "ses_one");
    let first = recorded(&docker, name, "ses_one").await;
    assert!(alive(&docker, name, &first).await);

    // Started again (a daemon restart, a person starting it again), the
    // program from before is stopped first.
    start_shell(name, "ses_one");
    let second = waiting::found("the new program's pid", async || {
        Some(recorded(&docker, name, "ses_one").await).filter(|second| *second != first)
    })
    .await;
    assert!(!alive(&docker, name, &first).await, "the old program went");
    assert!(alive(&docker, name, &second).await);

    // A pid file naming a process of someone else's (a reused pid) stops
    // nothing, and is cleared.
    let stranger = sh(&docker, name, "sleep 600 >/dev/null 2>&1 & echo $!").await;
    sh(
        &docker,
        name,
        &format!("echo {stranger} > /tmp/muster/sessions/ses_two.pid"),
    )
    .await;
    stop(&docker, name, "ses_two").await;
    assert!(alive(&docker, name, &stranger).await, "not the session's");
    assert_eq!(
        sh(
            &docker,
            name,
            "ls /tmp/muster/sessions/ses_two.pid 2>/dev/null"
        )
        .await,
        ""
    );

    stop(&docker, name, "ses_one").await;
    waiting::until("it to stop", async || !alive(&docker, name, &second).await).await;
    // Stopping again, with nothing left, is fine.
    stop(&docker, name, "ses_one").await;
}
