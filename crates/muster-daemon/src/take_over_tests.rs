//! Taking runs over and handing them back (`PRD-08`): the person's agent
//! in the run's container, through a stand-in `docker`.

use std::path::Path;
use std::time::Duration;

use muster_core::protocol::{Command, ErrorCode, Reply, Snapshot, TermSize};
use muster_core::{RunId, RunStage, SessionId, WorkspaceId, WorkspaceKind};
use muster_state::run::{RunEffect, RunEvent};
use tokio::time::timeout;

use super::*;

/// One start of the stand-in docker: its arguments, and the credential its
/// environment carried.
#[derive(Debug)]
struct Launch {
    args: Vec<String>,
    secret: String,
}

impl Launch {
    /// The arguments after the container: the session, the harness and the
    /// agent's conversation the script in it is given.
    fn script_args(&self) -> &[String] {
        &self.args[self.args.len() - 3..]
    }

    fn session(&self) -> &str {
        &self.script_args()[0]
    }
}

fn launches(dir: &Path) -> Vec<Launch> {
    let Ok(entries) = std::fs::read_dir(dir.join("launches")) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "args"))
        .map(|entry| {
            let args = std::fs::read(entry.path()).unwrap();
            let secret =
                std::fs::read_to_string(entry.path().with_extension("secret")).unwrap_or_default();
            // Each argument ends in a NUL; an empty one is an argument too.
            let mut args: Vec<String> = args
                .split(|byte| *byte == 0)
                .map(|arg| String::from_utf8_lossy(arg).into_owned())
                .collect();
            args.pop();
            Launch { args, secret }
        })
        .collect()
}

/// Waits for `count` starts of the stand-in docker for `session`.
async fn wait_for_launches(dir: &Path, session: SessionId, count: usize) -> Vec<Launch> {
    timeout(Duration::from_secs(5), async {
        loop {
            let found: Vec<Launch> = launches(dir)
                .into_iter()
                .filter(|launch| launch.session() == session.to_string())
                .collect();
            if found.len() >= count {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("docker was not started {count} time(s) for {session}"))
}

/// Waits until the snapshot satisfies `done`.
async fn wait_for(setup: &Setup, mut done: impl FnMut(&Snapshot) -> bool) -> Snapshot {
    timeout(Duration::from_secs(5), async {
        loop {
            let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
            if done(&snapshot) {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the state never got there")
}

fn ended(snapshot: &Snapshot, session: SessionId) -> bool {
    snapshot
        .sessions
        .iter()
        .any(|found| found.id == session && found.ended_at.is_some())
}

/// A run building in container `c-1`, its agent's conversation `agent-1`.
async fn building(setup: &Setup, deltas: &mut broadcast::Receiver<Delta>) -> RunId {
    let run = start(setup).await;
    let (_, effect) = setup.work.asked(1).await;
    setup.work.report(run, Report::Container("c-1".to_owned()));
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&effect),
        }),
    );
    setup.work.asked(2).await;
    setup
        .work
        .report(run, Report::AgentSession("agent-1".to_owned()));
    wait_for_stage(deltas, run, RunStage::Building).await;
    // The report on the agent's conversation is in before anything else.
    setup.supervisor.command(Command::Ping).await.unwrap();
    run
}

async fn take_over(setup: &Setup, run: RunId) -> (WorkspaceId, SessionId) {
    match setup
        .supervisor
        .command(Command::TakeOverRun { run })
        .await
        .unwrap()
    {
        Reply::RunTakenOver { workspace, session } => (workspace, session),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn the_person_drives_the_run_s_agent_in_its_container_and_hands_it_back() {
    let setup = setup(CHECKS).await;
    let dir = setup._dir.path().to_owned();
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = building(&setup, &mut deltas).await;

    let (workspace, agent) = take_over(&setup, run).await;
    wait_for_stage(&mut deltas, run, RunStage::TakenOver).await;
    // Muster's own agent stops; the person's is not Muster's work to start.
    let (_, effect) = setup.work.asked(3).await;
    assert!(matches!(effect, RunEffect::StopWork { .. }), "{effect:?}");
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    let container = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .unwrap();
    assert_eq!(container.project, setup.project);
    assert_eq!(
        container.kind,
        WorkspaceKind::Container {
            run,
            branch: "muster/issue-42-fix-the-login-redirect".to_owned()
        }
    );
    let session = snapshot.sessions.iter().find(|s| s.id == agent).unwrap();
    assert_eq!(
        (session.workspace, session.harness.as_str()),
        (workspace, "claude")
    );

    // Started through docker with a terminal, in the run's container,
    // carrying on the run agent's conversation; the credential in docker's
    // environment, never its arguments (`PRD-06`).
    let launch = wait_for_launches(&dir, agent, 1).await.remove(0);
    assert_eq!(launch.args[..3], ["exec", "--interactive", "--tty"]);
    let container_at = launch.args.iter().position(|arg| arg == "c-1").unwrap();
    assert!(
        launch.args[..container_at]
            .windows(2)
            .any(|pair| pair == ["--env", "CLAUDE_CODE_OAUTH_TOKEN"])
    );
    assert_eq!(
        launch.args[container_at + 1..container_at + 3],
        ["sh", "-c"]
    );
    assert_eq!(launch.script_args()[1..], ["claude", "agent-1"]);
    assert!(launch.args.iter().all(|arg| !arg.contains(TOKEN)));
    assert_eq!(launch.secret, TOKEN);

    // Once is enough; and in the container, Claude Code is the run's.
    let twice = setup
        .supervisor
        .command(Command::TakeOverRun { run })
        .await
        .unwrap_err();
    assert_eq!(twice.code, ErrorCode::Conflict);
    assert!(
        twice.message.contains("while it is taken over"),
        "{}",
        twice.message
    );
    let start_in = |harness: &str| Command::StartSession {
        workspace,
        harness: harness.to_owned(),
        title: None,
        size: TermSize::default(),
        prompt: None,
    };
    let refused = setup
        .supervisor
        .command(start_in("claude"))
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    let Reply::SessionStarted { id: shell } =
        setup.supervisor.command(start_in("shell")).await.unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let shell_launch = wait_for_launches(&dir, shell, 1).await.remove(0);
    assert_eq!(shell_launch.script_args()[1], "shell");
    assert_eq!(shell_launch.secret, "", "a shell gets no credential");

    // Handing back stops what runs in the container and checks the work.
    setup
        .supervisor
        .command(Command::HandBackRun { run })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Checking).await;
    let (_, stop) = setup.work.asked(4).await;
    let (_, checks) = setup.work.asked(5).await;
    let RunEffect::RunChecks { generation } = checks else {
        panic!("expected RunChecks, got {checks:?}");
    };
    assert_eq!(stop, RunEffect::StopWork { before: generation });
    let snapshot = wait_for(&setup, |snapshot| {
        ended(snapshot, agent) && ended(snapshot, shell)
    })
    .await;
    assert_eq!(snapshot.sessions.len(), 2);
    let stopped = setup.work.stopped.lock().unwrap().clone();
    for session in [agent, shell] {
        assert!(
            stopped.contains(&("c-1".to_owned(), session)),
            "{stopped:?}"
        );
    }

    // The agent is the run's again: the person cannot start it now.
    let restart = Command::RestartSession {
        session: agent,
        size: TermSize::default(),
    };
    let refused = setup.supervisor.command(restart.clone()).await.unwrap_err();
    assert!(
        refused.message.contains("take the run over"),
        "{}",
        refused.message
    );

    // Taking over again starts the same session again.
    let (again, same) = take_over(&setup, run).await;
    assert_eq!((again, same), (workspace, agent));
    wait_for_launches(&dir, agent, 2).await;
    wait_for(&setup, |snapshot| !ended(snapshot, agent)).await;

    // Cancelling ends the run and its container: the workspace goes.
    setup
        .supervisor
        .command(Command::CancelRun { run })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Cancelled).await;
    let snapshot = wait_for(&setup, |snapshot| {
        snapshot
            .workspaces
            .iter()
            .all(|found| found.id != workspace)
    })
    .await;
    assert!(snapshot.sessions.iter().all(|s| s.workspace != workspace));
}

#[tokio::test]
async fn taking_over_needs_the_credential_and_a_container() {
    let setup = setup_with(CHECKS, None).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    setup.work.asked(1).await;
    let early = setup
        .supervisor
        .command(Command::TakeOverRun { run })
        .await
        .unwrap_err();
    assert_eq!(early.code, ErrorCode::Conflict);
    assert!(
        early.message.contains("while it is starting its container"),
        "{}",
        early.message
    );

    let (_, effect) = setup.work.asked(1).await;
    setup.work.report(run, Report::Container("c-1".to_owned()));
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&effect),
        }),
    );
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    let missing = setup
        .supervisor
        .command(Command::TakeOverRun { run })
        .await
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::BadRequest);
    assert!(
        missing.message.contains("claude setup-token"),
        "{}",
        missing.message
    );
    // Nothing moved: the run builds on, with no workspace for the person.
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.runs[0].stage, RunStage::Building);
    assert!(snapshot.workspaces.iter().all(|w| w.kind.run().is_none()));
    assert_eq!(setup.work.count(), 2);
}

#[tokio::test]
async fn a_daemon_restart_starts_the_person_s_agent_again_and_ends_their_shells() {
    let setup = setup(CHECKS).await;
    let dir = setup._dir.path().to_owned();
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = building(&setup, &mut deltas).await;
    let (workspace, agent) = take_over(&setup, run).await;
    let Reply::SessionStarted { id: shell } = setup
        .supervisor
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
    wait_for_launches(&dir, agent, 1).await;
    wait_for_launches(&dir, shell, 1).await;

    let Setup {
        _dir: kept,
        database,
        supervisor,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let work = Arc::new(FakeWork::default());
    let supervisor = spawn(&database, work.clone(), kept.path()).await;

    // The agent carries on its conversation, in a program started again
    // (whatever is left of the old one is stopped by the script first).
    let relaunched = wait_for_launches(&dir, agent, 2).await;
    assert!(
        relaunched
            .iter()
            .all(|launch| launch.script_args()[1..] == ["claude", "agent-1"])
    );
    // The shell is recorded as ended, and stopped where it may still run.
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.runs[0].stage, RunStage::TakenOver);
    assert!(ended(&snapshot, shell));
    assert!(
        work.stopped
            .lock()
            .unwrap()
            .contains(&("c-1".to_owned(), shell))
    );
    assert_eq!(
        launches(&dir)
            .iter()
            .filter(|l| l.session() == shell.to_string())
            .count(),
        1
    );
}

#[tokio::test]
async fn a_container_left_by_a_run_that_ended_goes_when_the_daemon_starts() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    setup.work.asked(1).await;
    setup
        .supervisor
        .command(Command::CancelRun { run })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Cancelled).await;

    // As if the daemon had stopped before the run's container left.
    let Setup {
        _dir: kept,
        database,
        supervisor,
        project,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let left = muster_core::Workspace {
        id: WorkspaceId::new(),
        project,
        kind: WorkspaceKind::Container {
            run,
            branch: "muster/issue-42-fix-the-login-redirect".to_owned(),
        },
        created_at: muster_core::Timestamp::from_millis(43),
    };
    let mut store = Store::open(&database).unwrap();
    store.insert_workspace(&left).unwrap();
    drop(store);

    let supervisor = spawn(&database, Arc::new(FakeWork::default()), kept.path()).await;
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert!(snapshot.workspaces.iter().all(|found| found.id != left.id));
    drop(supervisor);
    let store = Store::open(&database).unwrap();
    assert!(
        store
            .snapshot()
            .unwrap()
            .workspaces
            .iter()
            .all(|found| found.id != left.id)
    );
}
