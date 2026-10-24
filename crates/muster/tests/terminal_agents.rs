//! End-to-end tests of coding agents found in terminals (`PRODUCT.md` §6,
//! `ARC-28`): an agent typed by hand into a terminal Muster opened makes
//! that terminal its chat while it runs, with its status from what its
//! terminal shows, among the agents; when it leaves, the terminal is a
//! terminal again. Any other program is a terminal by its name. From the
//! real daemon, with a real `/bin/sh` and stand-in agents on `PATH`.

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

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use muster_client::Connection;
use muster_core::protocol::{Command, Reply, TermSize};
use muster_core::{Session, SessionId, SessionStatus, WorkspaceId};

use support::{Daemon, Sandbox};

/// A stand-in agent: it waits for a line, says on its terminal's progress
/// sequence that it is busy, waits for another and says it is idle, then
/// ends with a third.
const AGENT: &str = "#!/bin/sh\nread go\nprintf '\\033]9;4;3;\\007'\nread done\nprintf '\\033]9;4;0;\\007'\nread quit\n";

struct Setup {
    sandbox: Sandbox,
    _daemon: Daemon,
    connection: Connection,
    terminal: SessionId,
}

/// A daemon over Muster's own `settings`, with `agents` on `PATH` as the
/// stand-in, a project, and a terminal in its checkout.
async fn setup(agents: &[&str], settings: Option<&str>) -> Setup {
    setup_looking_every("50", agents, settings).await
}

/// The same, with the terminals looked at every `millis`.
async fn setup_looking_every(millis: &str, agents: &[&str], settings: Option<&str>) -> Setup {
    let sandbox = Sandbox::new().with_env("MUSTER_FRONT_MS", millis);
    let bin = sandbox.root().join("bin");
    for agent in agents {
        let program = bin.join(agent);
        std::fs::write(&program, AGENT).unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    if let Some(settings) = settings {
        sandbox.own_settings(settings);
    }
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap();
    let checkout = connection.subscribe().await.unwrap().workspaces[0].id;
    let terminal = start_terminal(&mut connection, checkout).await;
    Setup {
        sandbox,
        _daemon: daemon,
        connection,
        terminal,
    }
}

async fn start_terminal(connection: &mut Connection, workspace: WorkspaceId) -> SessionId {
    let Reply::SessionStarted { id } = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    id
}

/// Waits until `session`, as the state says, is as `wanted` says.
async fn until(
    connection: &mut Connection,
    session: SessionId,
    what: &str,
    wanted: impl Fn(&Session) -> bool,
) -> Session {
    wait::found_async(what, async || {
        let snapshot = connection.subscribe().await.unwrap();
        Some(
            snapshot
                .sessions
                .into_iter()
                .find(|stored| stored.id == session)
                .unwrap(),
        )
        .filter(|stored| wanted(stored))
    })
    .await
}

fn agent(session: &Session) -> Option<&str> {
    session.agent.as_ref().map(|agent| agent.harness.as_str())
}

#[tokio::test]
async fn an_agent_typed_into_a_terminal_is_its_chat_while_it_runs_arc_28() {
    let Setup {
        sandbox,
        _daemon,
        mut connection,
        terminal,
    } = setup(&["codex"], None).await;
    let shell = until(&mut connection, terminal, "named", |s| s.title != "Shell").await;
    assert_eq!(agent(&shell), None);
    assert!(shell.kind().is_shell());

    connection.input(terminal, b"codex\n").await.unwrap();
    let found = until(&mut connection, terminal, "codex found", |s| {
        agent(s) == Some("codex")
    })
    .await;
    assert_eq!(found.title, "codex");
    assert!(found.kind().is_agent() && found.is_terminal());
    assert_eq!(found.status, SessionStatus::Fresh);
    // Among the agents, as the command line lists them, still a terminal.
    let listed = sandbox
        .command()
        .args(["list", "agents", "--all", "--json"])
        .output()
        .unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).unwrap();
    let entry = listed["sessions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["id"] == terminal.to_string())
        .unwrap_or_else(|| panic!("listed among the agents: {listed}"))
        .clone();
    assert_eq!(entry["harness"], "shell");
    assert_eq!(entry["agent"], "codex");

    // Its status is what its terminal shows: at work, then done and not
    // yet seen, as nobody watches it.
    connection.input(terminal, b"go\n").await.unwrap();
    until(&mut connection, terminal, "working", |s| {
        s.status == SessionStatus::Running
    })
    .await;
    connection.input(terminal, b"done\n").await.unwrap();
    until(&mut connection, terminal, "finished", |s| {
        s.status == SessionStatus::Finished && s.unseen
    })
    .await;

    // It leaves: the terminal is a terminal again, its news still news.
    connection.input(terminal, b"quit\n").await.unwrap();
    let back = until(&mut connection, terminal, "left", |s| s.agent.is_none()).await;
    assert_eq!(back.title, shell.title);
    assert!(back.kind().is_shell() && back.unseen, "{back:?}");
    assert!(back.ended_at.is_none(), "the terminal runs on");
}

#[tokio::test]
async fn an_agent_the_person_added_is_found_by_its_program_and_others_are_programs() {
    let Setup {
        sandbox: _sandbox,
        _daemon,
        mut connection,
        terminal,
    } = setup(
        &["pi-agent"],
        Some("[agents.pi]\nlabel = \"Pi\"\nprogram = \"pi-agent\"\n"),
    )
    .await;
    until(&mut connection, terminal, "named", |s| s.title != "Shell").await;
    connection.input(terminal, b"pi-agent\n").await.unwrap();
    let found = until(&mut connection, terminal, "pi found", |s| {
        agent(s) == Some("pi")
    })
    .await;
    assert_eq!(found.title, "pi-agent");
    connection.input(terminal, b"\n\n\n").await.unwrap();
    until(&mut connection, terminal, "left", |s| s.agent.is_none()).await;

    // Any other program is the terminal's, by its name.
    connection.input(terminal, b"sleep 3\n").await.unwrap();
    let sleeping = until(&mut connection, terminal, "sleep", |s| s.title == "sleep").await;
    assert_eq!(agent(&sleeping), None);
    assert!(sleeping.kind().is_shell());
}

#[tokio::test]
async fn a_reload_keeps_the_agent_in_a_terminal_and_its_status() {
    let Setup {
        sandbox,
        _daemon: daemon,
        mut connection,
        terminal,
    } = setup(&["codex"], None).await;
    until(&mut connection, terminal, "named", |s| s.title != "Shell").await;
    connection.input(terminal, b"codex\ngo\n").await.unwrap();
    until(&mut connection, terminal, "working", |s| {
        agent(s) == Some("codex") && s.status == SessionStatus::Running
    })
    .await;

    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success(), "{reloaded:?}");
    let mut connection = daemon.connect().await;
    let after = until(&mut connection, terminal, "taken over", |s| {
        agent(s) == Some("codex")
    })
    .await;
    assert_eq!(after.status, SessionStatus::Running, "not started over");
    // Found again by the next daemon, the same agent: nothing changes.
    wait::holds_async(
        "the same agent at work",
        Duration::from_millis(500),
        async || {
            let later = until(&mut connection, terminal, "still", |_| true).await;
            (agent(&later), later.status) == (Some("codex"), SessionStatus::Running)
        },
    )
    .await;
}

#[tokio::test]
async fn an_agent_muster_exec_runs_in_a_terminal_is_heard_by_hooks_and_ends_there_arc_28() {
    let Setup {
        sandbox,
        _daemon,
        mut connection,
        terminal,
    } = setup_looking_every("600000", &[], None).await;
    // Nothing looks at the terminal in this test after its first moment:
    // the agent is heard as `muster exec` runs it and ends it alone.
    // As `muster init`'s `claude` function runs it, typed in the terminal.
    let typed = format!("{} exec claude fix the login\n", support::BINARY);
    connection.input(terminal, typed.as_bytes()).await.unwrap();
    let launch = sandbox.wait_for_launches(terminal, 1).remove(0);
    assert!(
        launch.args.iter().any(|arg| arg == "--settings"),
        "{launch:?}"
    );
    assert!(
        launch.args.iter().any(|arg| arg == "--session-id"),
        "{launch:?}"
    );
    assert_eq!(
        launch.args.last().map(String::as_str),
        Some("fix the login")
    );
    assert!(!launch.token.is_empty(), "the terminal's own hooks");
    let working = until(&mut connection, terminal, "claude at work", |s| {
        agent(s) == Some("claude") && s.status == SessionStatus::Running
    })
    .await;
    assert!(working.is_terminal() && working.kind().is_agent());
    // Its chat is named by its first prompt, as any agent's is.
    assert_eq!(working.title, "fix the login");

    // It ends (the stand-in is a shell by now): the terminal is a terminal
    // again at once, still running.
    connection.input(terminal, b"exit 0\n").await.unwrap();
    let back = until(&mut connection, terminal, "ended", |s| s.agent.is_none()).await;
    assert!(
        back.ended_at.is_none() && back.kind().is_shell(),
        "{back:?}"
    );
    assert_eq!(back.status, SessionStatus::Fresh);
}

#[tokio::test]
async fn an_agent_found_after_it_showed_its_work_is_heard_at_work_arc_28() {
    // Looked at seldom: the agent says it is busy long before it is found.
    let Setup {
        sandbox: _sandbox,
        _daemon,
        mut connection,
        terminal,
    } = setup_looking_every("500", &["codex"], None).await;
    until(&mut connection, terminal, "named", |s| s.title != "Shell").await;
    connection.input(terminal, b"codex\ngo\n").await.unwrap();
    until(&mut connection, terminal, "found at work", |s| {
        agent(s) == Some("codex") && s.status == SessionStatus::Running
    })
    .await;
}
