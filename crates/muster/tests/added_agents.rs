//! End-to-end tests for coding agents the person adds in Muster's own
//! settings (`[agents.<name>]` in `settings.toml`), which Muster does not
//! know: a stand-in on `PATH` plays one. It is started as its settings say,
//! heard without hooks, by its terminal's progress sequence, its window
//! title and its exit (`PRD-13`), carries on its conversation after a
//! restart by its resume arguments (`PRD-09`), and takes a ticket as its
//! first prompt (`PRD-01`). A definition that does not read is refused
//! with the key that is wrong (`ARC-05`).

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

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::settings::AddedAgent;
use muster_core::{ProjectId, SessionId, SessionStatus, WaitReason, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event, read_terminal};

/// An agent as the person would add one.
const MY_AGENT: &str = r#"
[agents.my-agent]
label = "My Agent"
program = "my-agent"
args = ["--quiet"]
prompt = ["--message", "{prompt}"]
resume = ["--continue"]
env = { MY_AGENT_THEME = "dark" }
mark = "M"
colour = "green"

[agents.my-agent.title]
waiting = "Approve?"
"#;

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    project: ProjectId,
    workspace: WorkspaceId,
}

/// A daemon over Muster's own `settings`, if any, with a project and a
/// worktree, and the stand-in on `PATH` as `my-agent`.
async fn setup(settings: Option<&str>) -> Setup {
    let sandbox = Sandbox::new();
    sandbox.add_agent_program("my-agent");
    if let Some(settings) = settings {
        sandbox.own_settings(settings);
    }
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
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    Setup {
        sandbox,
        daemon,
        project,
        workspace,
    }
}

async fn start(
    connection: &mut Connection,
    workspace: WorkspaceId,
    prompt: Option<&str>,
) -> Result<SessionId, ClientError> {
    connection
        .command(Command::StartSession {
            workspace,
            harness: "my-agent".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: prompt.map(str::to_owned),
        })
        .await
        .map(|reply| match reply {
            Reply::SessionStarted { id } => id,
            other => panic!("unexpected {other:?}"),
        })
}

/// Waits until `session`'s status is `wanted`.
async fn expect_until(connection: &mut Connection, session: SessionId, wanted: SessionStatus) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            ..
        }) if *changed == session && *status == wanted => Some(()),
        _ => None,
    })
    .await;
}

fn refusal(result: Result<SessionId, ClientError>) -> (ErrorCode, String) {
    match result {
        Err(ClientError::Daemon(error)) => (error.code, error.message),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_agent_from_the_person_s_settings_is_heard_by_its_progress_title_and_exit() {
    let setup = setup(Some(MY_AGENT)).await;
    let mut connection = setup.daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    // Clients are told what to offer, and nothing of its environment.
    assert_eq!(
        snapshot.preferences.agents,
        [AddedAgent {
            name: "my-agent".to_owned(),
            label: "My Agent".to_owned(),
            mark: "M".to_owned(),
            colour: Some("green".to_owned()),
            takes_prompt: true,
            asks: false,
        }]
    );
    assert_eq!(snapshot.preferences.problem, None);

    let session = start(&mut connection, setup.workspace, Some("fix it"))
        .await
        .unwrap();
    let launch = setup
        .sandbox
        .wait_for_agent("my-agent", session, 1)
        .remove(0);
    assert_eq!(launch.args, ["--quiet", "--message", "fix it"]);
    // No hooks: nothing to report to, and nothing of Muster's in its own
    // settings.
    assert_eq!((launch.socket.as_str(), launch.token.as_str()), ("", ""));
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot
        .sessions
        .iter()
        .find(|stored| stored.id == session)
        .unwrap();
    // Named by its prompt, as any chat is.
    assert_eq!(stored.title, "fix it");
    assert_eq!(stored.harness, "my-agent");
    assert_eq!(stored.status, SessionStatus::Running);

    // Its progress sequence: working, then done with its turn.
    connection
        .input(
            session,
            b"printf '\\033]9;4;3\\007'; printf '\\033]9;4;0\\007'\n",
        )
        .await
        .unwrap();
    expect_until(&mut connection, session, SessionStatus::Finished).await;
    // Its title, by the words its settings name: waiting on the person.
    connection
        .input(session, b"printf '\\033]0;Approve? (y/n)\\007'\n")
        .await
        .unwrap();
    let action = SessionStatus::NeedsYou {
        reason: WaitReason::Action,
    };
    expect_until(&mut connection, session, action).await;
    // Its environment, as its settings add to it.
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"echo \"theme-$MY_AGENT_THEME-$((20+22))\"\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "theme-dark-42").await;
    assert!(text.contains("theme-dark-42"), "{text:?}");
    // Its exit.
    connection.input(session, b"exit 3\n").await.unwrap();
    expect_until(
        &mut connection,
        session,
        SessionStatus::Exited { success: false },
    )
    .await;

    // Started again, it carries on its conversation: no id to resume by,
    // its resume arguments instead of the prompt.
    let Reply::SessionStarted { .. } = connection
        .command(Command::RestartSession {
            session,
            size: TermSize::default(),
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let launches = setup.sandbox.wait_for_agent("my-agent", session, 2);
    assert_eq!(launches[1].args, ["--quiet", "--continue"]);

    // And so after the daemon restarts, which its terminal died with.
    drop(connection);
    let Setup {
        sandbox, daemon, ..
    } = setup;
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let launches = sandbox.wait_for_agent("my-agent", session, 3);
    assert_eq!(launches[2].args, ["--quiet", "--continue"]);
    drop(daemon);
}

#[tokio::test]
async fn the_person_s_settings_are_read_again_on_asking_and_a_mistake_named_by_its_key() {
    let setup = setup(None).await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let (code, message) = refusal(start(&mut connection, setup.workspace, None).await);
    assert_eq!(code, ErrorCode::BadRequest);
    assert!(
        message.contains("no harness called \"my-agent\"") && message.contains("[agents.my-agent]"),
        "{message}"
    );

    // A misspelt key: the file is refused whole, and says which key.
    setup.sandbox.own_settings(
        "[agents.my-agent]\nprogram = \"my-agent\"\ncolor = \"green\"\n\n[notch]\nshow = false\n",
    );
    assert_eq!(
        connection.command(Command::ReadPreferences).await.unwrap(),
        Reply::Done
    );
    let problem = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::Preferences(preferences)) => preferences.problem.clone(),
        _ => None,
    })
    .await;
    assert!(problem.contains("agents.my-agent.color"), "{problem}");
    assert!(problem.contains("settings.toml"), "{problem}");
    let (_, message) = refusal(start(&mut connection, setup.workspace, None).await);
    assert!(
        message.contains("do not read") && message.contains("agents.my-agent.color"),
        "{message}"
    );
    assert!(setup.daemon.log().contains("agents.my-agent.color"));

    // Put right, it is offered, and starts from the command line too.
    setup
        .sandbox
        .own_settings("[agents.my-agent]\nprogram = \"my-agent\"\n");
    connection.command(Command::ReadPreferences).await.unwrap();
    let agents = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::Preferences(preferences)) if preferences.problem.is_none() => {
            Some(preferences.agents.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(agents.len(), 1, "{agents:?}");
    assert!(!agents[0].takes_prompt);
    let started = setup
        .sandbox
        .run(&["session", "start", "work", "--harness", "my-agent"]);
    assert!(
        started.status.success(),
        "{}",
        String::from_utf8_lossy(&started.stderr)
    );
    // One that takes no prompt is given none.
    let (code, message) = refusal(start(&mut connection, setup.workspace, Some("fix it")).await);
    assert_eq!(code, ErrorCode::BadRequest);
    assert!(message.contains("takes no prompt"), "{message}");
    // Muster's own names are never the person's.
    setup
        .sandbox
        .own_settings("[agents.claude]\nprogram = \"my-agent\"\n");
    connection.command(Command::ReadPreferences).await.unwrap();
    let problem = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::Preferences(preferences)) => preferences.problem.clone(),
        _ => None,
    })
    .await;
    assert!(
        problem.contains("agents.claude") && problem.contains("Muster's own"),
        "{problem}"
    );
}

#[tokio::test]
async fn a_ticket_is_handed_to_an_agent_from_the_person_s_settings_as_its_first_prompt() {
    let setup = setup(Some(MY_AGENT)).await;
    let gh = setup.sandbox.fake_gh();
    let issue = json!({"number": 42, "title": "Fix the login redirect",
                       "url": "https://github.com/o/app/issues/42",
                       "labels": [], "author": {"login": "furkan"},
                       "body": "After signing in, users land on /404."});
    std::fs::write(gh.join("issues.json"), json!([issue]).to_string()).unwrap();
    std::fs::write(gh.join("issue-42.json"), issue.to_string()).unwrap();
    let mut connection = setup.daemon.connect().await;
    let Reply::IssueStarted { session, .. } = connection
        .command(Command::StartFromIssue {
            project: setup.project,
            key: muster_core::TicketKey::number(42),
            harness: "my-agent".to_owned(),
            size: TermSize::default(),
            another: false,
            on: None,
            brief: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected IssueStarted");
    };
    let launch = setup
        .sandbox
        .wait_for_agent("my-agent", session, 1)
        .remove(0);
    assert_eq!(launch.args[..2], ["--quiet", "--message"]);
    let prompt = launch.args.last().unwrap();
    assert!(
        prompt.contains("issue #42") && prompt.contains("After signing in, users land on /404."),
        "{prompt}"
    );
}
