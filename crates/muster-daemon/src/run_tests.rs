//! In-process tests of runs: the Supervisor stepping run machines, storing
//! transitions, handing work to a fake [`RunWork`] and taking its reports.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::protocol::{Command, Delta, ErrorCode, Reply};
use muster_core::{
    Issue, ProjectId, PullRequest, RunId, RunStage, SessionId, TicketKey, Timestamp,
};
use muster_state::run::{AgentOutcome, RunEffect, RunEvent, Verdict};
use tokio::sync::{broadcast, mpsc};
use tokio::time::timeout;

use crate::clock::Clock;
use crate::github::BoxFuture;
use crate::runs::credentials::{FixedCredentials, Secret};
use crate::runs::{Report, Reports, RunContext, RunWork};
use crate::store::{Store, StoreHandle};
use crate::supervisor::{self, SupervisorHandle};
use crate::trackers::{Fixed, IssueDetail, ServiceError, TicketSource};

#[derive(Debug)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp::from_millis(42)
    }
}

/// Issue 42, and no other.
#[derive(Debug)]
struct OneIssue;

impl TicketSource for OneIssue {
    fn describe(&self) -> String {
        "GitHub issues".to_owned()
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async { Ok(vec![issue()]) })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            if key.as_number() == Some(42) {
                Ok(IssueDetail {
                    issue: issue(),
                    body: "Users land on /404.".to_owned(),
                    noun: crate::trackers::Noun::own("github"),
                })
            } else {
                Err(ServiceError::NotFound(format!(
                    "Could not resolve to an Issue with the number of {key}."
                )))
            }
        })
    }

    fn close<'a>(&'a self, _key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async { Ok(()) })
    }
}

fn issue() -> Issue {
    Issue {
        key: TicketKey::number(42),
        title: "Fix the login redirect".to_owned(),
        url: "https://github.com/o/app/issues/42".to_owned(),
        labels: vec!["bug".to_owned()],
        author: Some("someone".to_owned()),
    }
}

/// Records the work asked for and keeps the reports channel, so a test can
/// answer as the work would.
#[derive(Debug, Default)]
struct FakeWork {
    asked: Mutex<Vec<(RunContext, RunEffect)>>,
    pr_checks: Mutex<Vec<RunId>>,
    reports: Mutex<Option<Reports>>,
    /// Sessions asked to stop in their container.
    stopped: Mutex<Vec<(String, SessionId)>>,
    /// Runs whose containers were asked to go, outside their own work.
    removed: Mutex<Vec<RunId>>,
}

impl RunWork for FakeWork {
    fn start(&self, context: RunContext, effect: RunEffect, reports: Reports) {
        self.asked.lock().unwrap().push((context, effect));
        *self.reports.lock().unwrap() = Some(reports);
    }

    fn check_pull_request(&self, context: RunContext, reports: Reports) {
        self.pr_checks.lock().unwrap().push(context.run);
        *self.reports.lock().unwrap() = Some(reports);
    }

    fn stop_session(&self, container: String, session: SessionId) {
        self.stopped.lock().unwrap().push((container, session));
    }

    fn remove_containers_of(&self, runs: Vec<RunId>) {
        self.removed.lock().unwrap().extend(runs);
    }
}

impl FakeWork {
    /// Waits for the `n`-th piece of work asked for (from 1).
    async fn asked(&self, n: usize) -> (RunContext, RunEffect) {
        timeout(Duration::from_secs(5), async {
            loop {
                if let Some(found) = self.asked.lock().unwrap().get(n - 1).cloned() {
                    return found;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "work {n} was never asked for: {:?}",
                self.asked.lock().unwrap()
            )
        })
    }

    fn count(&self) -> usize {
        self.asked.lock().unwrap().len()
    }

    fn report(&self, run: RunId, report: Report) {
        let reports = self
            .reports
            .lock()
            .unwrap()
            .clone()
            .expect("work was asked for");
        reports.send((run, report)).unwrap();
    }
}

struct Setup {
    dir: tempfile::TempDir,
    root: PathBuf,
    database: PathBuf,
    work: Arc<FakeWork>,
    supervisor: SupervisorHandle,
    project: ProjectId,
}

const CHECKS: &str = "[checks]\ncommands = [\"make test\"]\n";

/// The runs' credential in these tests.
const TOKEN: &str = "sk-ant-oat01-test-only";

/// Stands in for `docker exec` with a terminal: records its arguments and
/// the credential it was given in `launches/`, then acts as a shell.
const FAKE_DOCKER: &str = r#"#!/bin/sh
dir="$(dirname "$0")/launches"
mkdir -p "$dir"
for arg in "$@"; do printf '%s\0' "$arg"; done > "$dir/$$.part"
printf '%s' "${CLAUDE_CODE_OAUTH_TOKEN:-}" > "$dir/$$.secret"
mv "$dir/$$.part" "$dir/$$.args"
exec /bin/sh
"#;

async fn spawn(database: &Path, work: Arc<FakeWork>, config_dir: &Path) -> SupervisorHandle {
    spawn_with(database, work, config_dir, Some(TOKEN)).await
}

async fn spawn_with(
    database: &Path,
    work: Arc<FakeWork>,
    config_dir: &Path,
    token: Option<&str>,
) -> SupervisorHandle {
    let docker = config_dir.join("docker");
    std::fs::write(&docker, FAKE_DOCKER).unwrap();
    std::fs::set_permissions(&docker, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let store = StoreHandle::spawn(Store::open(database, FixedClock.now()).unwrap()).unwrap();
    let secret = token.map(|token| Secret::new("CLAUDE_CODE_OAUTH_TOKEN", token.to_owned()));
    supervisor::spawn(supervisor::Settings {
        store,
        clock: Arc::new(FixedClock),
        worktrees_dir: config_dir.join("worktrees"),
        data_dir: config_dir.join("data"),
        hook_socket: config_dir.join("hooks.sock"),
        hook_queue: mpsc::channel(1).1,
        delta_capacity: 256,

        timing: muster_state::session::Timing::default(),
        work,
        trackers: Arc::new(Fixed(Arc::new(OneIssue))),
        config_dir: config_dir.to_owned(),
        credentials: Arc::new(FixedCredentials(secret)),
        docker: docker.display().to_string(),
        // Never reached here: pull requests by hand are tested end to end.
        hosts: Arc::new(crate::githost::choose::ServiceHosts {
            secrets: Arc::new(crate::secrets::Kept::default()),
            http: crate::service::http_client(),
        }),
        adopted: Vec::new(),
        tasks: crate::tasks::Tasks::default(),
        ticket_plugins: crate::trackers::plugin::Plugins::new("muster".into()),
        places: crate::spend::Places::default(),
    })
    .await
    .unwrap()
}

async fn setup(muster_toml: &str) -> Setup {
    setup_with(muster_toml, Some(TOKEN)).await
}

async fn setup_with(muster_toml: &str, token: Option<&str>) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    std::fs::create_dir_all(root.join(".git")).unwrap();
    std::fs::write(root.join("muster.toml"), muster_toml).unwrap();
    let root = root.canonicalize().unwrap();
    let database = dir.path().join("muster.db");
    let work = Arc::new(FakeWork::default());
    let supervisor = spawn_with(&database, work.clone(), dir.path(), token).await;
    let Reply::ProjectAdded { id: project } = supervisor
        .command(Command::AddProject {
            root: root.clone(),
            name: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    Setup {
        dir,
        root,
        database,
        work,
        supervisor,
        project,
    }
}

async fn start(setup: &Setup) -> RunId {
    match setup
        .supervisor
        .command(Command::StartRun {
            project: setup.project,
            number: 42,
            from: None,
        })
        .await
        .unwrap()
    {
        Reply::RunStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

/// Waits for the run's stage, as clients are told it.
async fn wait_for_stage(deltas: &mut broadcast::Receiver<Delta>, run: RunId, stage: RunStage) {
    timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Delta::RunUpserted(shown)) = deltas.recv().await
                && shown.id == run
                && shown.stage == stage
            {
                return;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the run never reached {stage:?}"));
}

fn generation(effect: &RunEffect) -> u64 {
    match effect {
        RunEffect::StartContainer { generation }
        | RunEffect::StartBuilder { generation, .. }
        | RunEffect::RunChecks { generation }
        | RunEffect::PublishBranch { generation }
        | RunEffect::StartReviewer { generation } => *generation,
        other => panic!("{other:?} names no generation"),
    }
}

#[tokio::test]
async fn a_run_goes_through_every_stage_and_its_timeline_is_stored() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    wait_for_stage(&mut deltas, run, RunStage::Provisioning).await;

    let (context, effect) = setup.work.asked(1).await;
    assert!(matches!(effect, RunEffect::StartContainer { .. }));
    assert_eq!(context.ticket.number, 42);
    assert_eq!(context.ticket.body, "Users land on /404.");
    assert_eq!(context.branch, "muster/issue-42-fix-the-login-redirect");
    assert_eq!(context.root, setup.root);
    assert_eq!(context.config.checks.commands, ["make test"]);

    let report = |event| setup.work.report(run, Report::Event(event));
    setup.work.report(run, Report::Container("c-1".to_owned()));
    report(RunEvent::ContainerReady {
        generation: generation(&effect),
    });
    let (_, effect) = setup.work.asked(2).await;
    assert!(matches!(effect, RunEffect::StartBuilder { attempt: 1, .. }));
    setup
        .work
        .report(run, Report::AgentSession("agent-1".to_owned()));
    report(RunEvent::AgentFinished {
        generation: generation(&effect),
        outcome: AgentOutcome::Done,
    });
    let (context, effect) = setup.work.asked(3).await;
    assert!(matches!(effect, RunEffect::RunChecks { .. }));
    assert_eq!(context.container.as_deref(), Some("c-1"));
    assert_eq!(context.agent_session.as_deref(), Some("agent-1"));

    report(RunEvent::ChecksFinished {
        generation: generation(&effect),
        passed: true,
        report: String::new(),
    });
    let (_, effect) = setup.work.asked(4).await;
    assert!(matches!(effect, RunEffect::PublishBranch { .. }));
    let pr = PullRequest {
        number: 17,
        url: "https://github.com/o/app/pull/17".to_owned(),
    };
    setup.work.report(run, Report::PullRequest(pr.clone()));
    report(RunEvent::PrOpened {
        generation: generation(&effect),
        number: 17,
    });
    let (_, effect) = setup.work.asked(5).await;
    assert!(matches!(effect, RunEffect::StartReviewer { .. }));
    report(RunEvent::ReviewSubmitted {
        generation: generation(&effect),
        verdict: Verdict::Approve,
    });
    wait_for_stage(&mut deltas, run, RunStage::ReadyForYou).await;
    let (_, effect) = setup.work.asked(6).await;
    assert_eq!(effect, RunEffect::MarkReady);

    // Its pull request is looked at, and its merge ends the run.
    setup.supervisor.check_pull_requests().await;
    timeout(Duration::from_secs(5), async {
        while setup.work.pr_checks.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    report(RunEvent::PrMerged);
    wait_for_stage(&mut deltas, run, RunStage::Merged).await;
    let (_, effect) = setup.work.asked(7).await;
    assert_eq!(effect, RunEffect::RemoveContainer);

    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    let shown = &snapshot.runs[0];
    assert_eq!(shown.pr, Some(pr));
    assert_eq!(shown.attempt, 1);
    assert_eq!(shown.ended_at, Some(Timestamp::from_millis(42)));
    let Reply::RunHistory { transitions } = setup
        .supervisor
        .command(Command::RunHistory { run })
        .await
        .unwrap()
    else {
        panic!("expected RunHistory");
    };
    let stages: Vec<_> = transitions.iter().map(|t| t.to).collect();
    assert_eq!(
        stages,
        [
            RunStage::Provisioning,
            RunStage::Building,
            RunStage::Checking,
            RunStage::OpeningPr,
            RunStage::AgentReview,
            RunStage::ReadyForYou,
            RunStage::Merged,
        ]
    );
    assert_eq!(transitions[2].reason, "the agent is done");

    // Over is over: the person's commands are refused, plainly.
    let refused = setup
        .supervisor
        .command(Command::CancelRun { run })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::Conflict);
    assert!(refused.message.contains("has ended"), "{}", refused.message);
}

#[tokio::test]
async fn runs_need_checks_one_at_a_time_per_issue_and_an_issue_that_exists() {
    let setup = setup("").await;
    let refused = setup
        .supervisor
        .command(Command::StartRun {
            project: setup.project,
            number: 42,
            from: None,
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(
        refused.message.contains("runs need checks"),
        "{}",
        refused.message
    );

    let setup = self::setup(CHECKS).await;
    start(&setup).await;
    let twice = setup
        .supervisor
        .command(Command::StartRun {
            project: setup.project,
            number: 42,
            from: None,
        })
        .await
        .unwrap_err();
    assert_eq!(twice.code, ErrorCode::Conflict);
    let missing = setup
        .supervisor
        .command(Command::StartRun {
            project: setup.project,
            number: 7,
            from: None,
        })
        .await
        .unwrap_err();
    assert_eq!(missing.code, ErrorCode::BadRequest);
    let unknown = setup
        .supervisor
        .command(Command::CancelRun { run: RunId::new() })
        .await
        .unwrap_err();
    assert_eq!(unknown.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn the_persons_commands_apply_only_when_they_make_sense() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    let (_, effect) = setup.work.asked(1).await;

    // Nothing to retry or hand back yet.
    for command in [
        Command::RetryRun { run, notes: None },
        Command::HandBackRun { run },
        Command::TakeOverRun { run },
    ] {
        let refused = setup.supervisor.command(command).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::Conflict);
        assert!(
            refused.message.contains("starting its container"),
            "{}",
            refused.message
        );
    }

    // A failure stops for the person; a retry sends it back with notes.
    setup.work.report(
        run,
        Report::Event(RunEvent::InfraFailed {
            generation: generation(&effect),
            reason: "docker went away".to_owned(),
            container_lost: false,
        }),
    );
    wait_for_stage(&mut deltas, run, RunStage::NeedsYou).await;
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.runs[0].reason.as_deref(), Some("docker went away"));
    setup
        .supervisor
        .command(Command::RetryRun {
            run,
            notes: Some("try again".to_owned()),
        })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Provisioning).await;
    let (_, effect) = setup.work.asked(3).await;
    assert!(matches!(effect, RunEffect::StartContainer { .. }));

    setup
        .supervisor
        .command(Command::CancelRun { run })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Cancelled).await;
}

#[tokio::test]
async fn reports_on_overtaken_work_change_nothing() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    let (_, first) = setup.work.asked(1).await;
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&first),
        }),
    );
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    // The same report again, now overtaken.
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&first),
        }),
    );
    setup.supervisor.command(Command::Ping).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(setup.work.count(), 2);
}

#[tokio::test]
async fn a_run_over_its_time_stops_its_work_for_the_person() {
    let setup = setup(&format!("{CHECKS}[limits]\nrun_timeout = \"1s\"\n")).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    setup.work.asked(1).await;
    wait_for_stage(&mut deltas, run, RunStage::NeedsYou).await;
    let (_, effect) = setup.work.asked(2).await;
    assert!(
        matches!(effect, RunEffect::StopWork { before: 2 }),
        "{effect:?}"
    );
    let (snapshot, _) = setup.supervisor.subscribe().await.unwrap();
    assert_eq!(
        snapshot.runs[0].reason.as_deref(),
        Some("it reached its time limit (1 s)")
    );
}

#[tokio::test]
async fn a_restarted_daemon_carries_on_with_the_work_under_way() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    let (_, effect) = setup.work.asked(1).await;
    setup.work.report(run, Report::Container("c-1".to_owned()));
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&effect),
        }),
    );
    let (_, building) = setup.work.asked(2).await;
    setup
        .work
        .report(run, Report::AgentSession("agent-1".to_owned()));
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    setup.supervisor.command(Command::Ping).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    // A new daemon on the same store.
    let Setup {
        dir,
        database,
        supervisor,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let work = Arc::new(FakeWork::default());
    let supervisor = spawn(&database, work.clone(), dir.path()).await;

    let (context, effect) = work.asked(1).await;
    assert_eq!(effect, building, "the same work, under the same generation");
    assert_eq!(context.container.as_deref(), Some("c-1"));
    assert_eq!(context.agent_session.as_deref(), Some("agent-1"));
    work.report(
        run,
        Report::Event(RunEvent::AgentFinished {
            generation: generation(&effect),
            outcome: AgentOutcome::Done,
        }),
    );
    let (_, effect) = work.asked(2).await;
    assert!(matches!(effect, RunEffect::RunChecks { .. }));
    let (snapshot, _) = supervisor.subscribe().await.unwrap();
    assert_eq!(snapshot.runs[0].stage, RunStage::Checking);
}

/// Answers the run's work, as it is asked for, from pickup to ready; the
/// work asked for so far is `asked`.
async fn drive_to_ready(setup: &Setup, run: RunId, deltas: &mut broadcast::Receiver<Delta>) {
    let report = |event| setup.work.report(run, Report::Event(event));
    let (_, effect) = setup.work.asked(1).await;
    setup.work.report(run, Report::Container("c-1".to_owned()));
    report(RunEvent::ContainerReady {
        generation: generation(&effect),
    });
    let (_, effect) = setup.work.asked(2).await;
    report(RunEvent::AgentFinished {
        generation: generation(&effect),
        outcome: AgentOutcome::Done,
    });
    let (_, effect) = setup.work.asked(3).await;
    report(RunEvent::ChecksFinished {
        generation: generation(&effect),
        passed: true,
        report: String::new(),
    });
    let (_, effect) = setup.work.asked(4).await;
    setup.work.report(
        run,
        Report::PullRequest(PullRequest {
            number: 17,
            url: "https://github.com/o/app/pull/17".to_owned(),
        }),
    );
    report(RunEvent::PrOpened {
        generation: generation(&effect),
        number: 17,
    });
    let (_, effect) = setup.work.asked(5).await;
    report(RunEvent::ReviewSubmitted {
        generation: generation(&effect),
        verdict: Verdict::Approve,
    });
    wait_for_stage(deltas, run, RunStage::ReadyForYou).await;
    assert_eq!(setup.work.asked(6).await.1, RunEffect::MarkReady);
}

#[tokio::test]
async fn the_person_sends_a_ready_run_back_with_what_to_change() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    let changes = |comments: &str| Command::RequestChanges {
        run,
        comments: comments.to_owned(),
    };

    // Only a run that is ready for the person takes changes.
    setup.work.asked(1).await;
    let early = setup
        .supervisor
        .command(changes("Handle the empty case."))
        .await
        .unwrap_err();
    assert_eq!(early.code, ErrorCode::Conflict);
    assert_eq!(
        early.message,
        "a run cannot take a request for changes while it is starting its container"
    );

    drive_to_ready(&setup, run, &mut deltas).await;
    // Saying nothing is no request.
    let empty = setup.supervisor.command(changes(" \n ")).await.unwrap_err();
    assert_eq!(empty.code, ErrorCode::BadRequest);

    setup
        .supervisor
        .command(changes("Handle the empty case."))
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    assert_eq!(setup.work.asked(7).await.1, RunEffect::MarkDraft);
    let (_, effect) = setup.work.asked(8).await;
    assert!(
        matches!(
            &effect,
            RunEffect::StartBuilder {
                attempt: 2,
                feedback: Some(muster_state::run::Feedback::PersonRequestedChanges { comments }),
                ..
            } if comments == "Handle the empty case."
        ),
        "{effect:?}"
    );
    let Reply::RunHistory { transitions } = setup
        .supervisor
        .command(Command::RunHistory { run })
        .await
        .unwrap()
    else {
        panic!("expected RunHistory");
    };
    assert_eq!(
        transitions.last().unwrap().reason,
        "you asked for changes; attempt 2"
    );
}

#[path = "take_over_tests.rs"]
mod take_over_tests;

#[path = "hand_off_tests.rs"]
mod hand_off_tests;

#[tokio::test]
async fn a_run_resumed_with_unreadable_settings_stops_for_the_person_until_they_are_mended() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    let (_, effect) = setup.work.asked(1).await;
    setup.work.report(run, Report::Container("c-1".to_owned()));
    setup.work.report(
        run,
        Report::Event(RunEvent::ContainerReady {
            generation: generation(&effect),
        }),
    );
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    setup.supervisor.command(Command::Ping).await.unwrap();

    // The settings break while the daemon is down.
    std::fs::write(
        setup.root.join("muster.toml"),
        "[checks]\ncommand = [\"typo\"]\n",
    )
    .unwrap();
    let Setup {
        dir,
        database,
        supervisor,
        root,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let work = Arc::new(FakeWork::default());
    let supervisor = spawn(&database, work.clone(), dir.path()).await;
    let (snapshot, mut deltas) = supervisor.subscribe().await.unwrap();
    let stopped = &snapshot.runs[0];
    assert_eq!(stopped.stage, RunStage::NeedsYou, "no work with no checks");
    let reason = stopped.reason.clone().unwrap();
    assert!(
        reason.starts_with("its settings cannot be read"),
        "{reason}"
    );
    assert!(
        work.asked
            .lock()
            .unwrap()
            .iter()
            .all(|(_, effect)| !matches!(effect, RunEffect::StartBuilder { .. })),
        "the agent was not started again"
    );

    // Sent back while they are still broken: refused, with why.
    let refused = supervisor
        .command(Command::RetryRun { run, notes: None })
        .await
        .unwrap_err();
    assert_eq!(refused.code, ErrorCode::BadRequest);
    assert!(refused.message.contains("command"), "{}", refused.message);

    // Mended, it goes back to work with them.
    std::fs::write(
        root.join("muster.toml"),
        "[checks]\ncommands = [\"make check\"]\n",
    )
    .unwrap();
    supervisor
        .command(Command::RetryRun { run, notes: None })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Building).await;
    let (context, _) = work
        .asked
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|(_, effect)| matches!(effect, RunEffect::StartBuilder { .. }))
        .cloned()
        .unwrap();
    assert_eq!(context.config.checks.commands, ["make check"]);
}

#[tokio::test]
async fn containers_of_runs_that_ended_go_even_when_they_come_late() {
    let setup = setup(CHECKS).await;
    let (_, mut deltas) = setup.supervisor.subscribe().await.unwrap();
    let run = start(&setup).await;
    setup.work.asked(1).await;
    // Cancelled while its container was still starting: the cancel asks
    // for whatever came of it to go.
    setup
        .supervisor
        .command(Command::CancelRun { run })
        .await
        .unwrap();
    wait_for_stage(&mut deltas, run, RunStage::Cancelled).await;
    assert!(
        setup
            .work
            .asked
            .lock()
            .unwrap()
            .iter()
            .any(|(_, effect)| *effect == RunEffect::RemoveContainer)
    );
    // The container starting then comes too late: it goes as well.
    setup
        .work
        .report(run, Report::Container("c-late".to_owned()));
    setup.supervisor.command(Command::Ping).await.unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(setup.work.removed.lock().unwrap().as_slice(), [run]);

    // A daemon starting again clears what is left of ended runs.
    let Setup {
        dir,
        database,
        supervisor,
        ..
    } = setup;
    drop(supervisor);
    tokio::time::sleep(Duration::from_millis(50)).await;
    let work = Arc::new(FakeWork::default());
    let _supervisor = spawn(&database, work.clone(), dir.path()).await;
    assert_eq!(work.removed.lock().unwrap().as_slice(), [run]);
}
