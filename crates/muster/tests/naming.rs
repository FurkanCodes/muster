//! End-to-end tests of chats named by an agent (`PRODUCT.md` §6): a chat
//! named by its first prompt is named again by the few words its agent
//! writes, asked one question in safe mode, credentials hidden; a prompt
//! of a few words is named once more after the first turn; the person's
//! own name is never replaced; and none of it happens once they turn it
//! off. From the real daemon, with a stand-in Claude Code.

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

use std::time::{Duration, Instant};

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::settings::PreferenceChange;
use muster_core::{SessionId, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event};

/// A daemon with one project, connected, and the project's checkout.
async fn project(sandbox: &Sandbox) -> (Daemon, Connection, WorkspaceId) {
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { .. } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    (daemon, connection, snapshot.workspaces[0].id)
}

async fn start(connection: &mut Connection, workspace: WorkspaceId, prompt: &str) -> SessionId {
    let Reply::SessionStarted { id } = connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: Some(prompt.to_owned()),
            started_by: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    id
}

/// The name next told for `session`.
async fn named(connection: &mut Connection, session: SessionId) -> String {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionUpserted(shown)) if shown.id == session => {
            Some(shown.title.clone())
        }
        _ => None,
    })
    .await
}

/// What the stand-in answers when asked.
fn answers(sandbox: &Sandbox, name: &str) {
    std::fs::create_dir_all(sandbox.fake_claude_flag("")).unwrap();
    std::fs::write(sandbox.fake_claude_flag("FAKE_CLAUDE_NAME"), name).unwrap();
}

/// Waits until the stand-in has been asked `count` questions.
fn asked(sandbox: &Sandbox, count: usize) -> Vec<(String, Vec<String>)> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let asked = sandbox.claude_asked();
        if asked.len() >= count {
            return asked;
        }
        assert!(Instant::now() < deadline, "asked {asked:?}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `session`'s name as the state says, after a moment for anything on its
/// way.
async fn settled_name(connection: &mut Connection, session: SessionId) -> String {
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let snapshot = connection.subscribe().await.unwrap();
    snapshot
        .sessions
        .iter()
        .find(|stored| stored.id == session)
        .unwrap()
        .title
        .clone()
}

#[tokio::test]
async fn a_chat_named_by_its_prompt_is_named_again_by_its_agent_in_safe_mode() {
    let sandbox = Sandbox::new();
    answers(&sandbox, "\"Login redirect fix.\"\nand more words");
    let (daemon, mut connection, checkout) = project(&sandbox).await;
    let token = format!("ghp_{}", "q7W8e9R0t1Y2u3I4o5P6".repeat(2));
    let session = start(
        &mut connection,
        checkout,
        &format!("Fix the login redirect: users land on /404 after signing in, push with {token}"),
    )
    .await;
    let first = named(&mut connection, session).await;
    assert!(first.starts_with("Fix the login redirect"), "{first}");
    assert_eq!(named(&mut connection, session).await, "Login redirect fix");

    let (cwd, args) = asked(&sandbox, 1).remove(0);
    // Asked in a folder of Muster's own, not the project's.
    assert!(cwd.ends_with("/ask"), "{cwd}");
    for kept_out in [
        "--safe-mode",
        "--strict-mcp-config",
        "--no-session-persistence",
    ] {
        assert!(
            args.iter().any(|arg| arg == kept_out),
            "{kept_out}: {args:?}"
        );
    }
    assert_eq!(&args[..2], ["--model", "haiku"]);
    let question = args.last().unwrap();
    assert!(question.contains("users land on /404"), "{question}");
    assert!(question.contains("[hidden by Muster]"), "{question}");
    assert!(!question.contains(&token), "{question}");
    // Neither the prompt nor the answer reaches the daemon's log.
    let log = daemon.log();
    assert!(!log.contains("users land on"), "{log}");
    assert!(!log.contains("Login redirect fix"), "{log}");
}

#[tokio::test]
async fn a_name_the_person_gives_is_never_replaced_by_an_agent_s() {
    let sandbox = Sandbox::new();
    answers(&sandbox, "Agent's name");
    std::fs::write(sandbox.fake_claude_flag("FAKE_CLAUDE_ASK_SLOW"), "").unwrap();
    let (_daemon, mut connection, checkout) = project(&sandbox).await;
    let session = start(
        &mut connection,
        checkout,
        "Write the release notes for the next version",
    )
    .await;
    named(&mut connection, session).await;
    let Reply::Done = connection
        .command(Command::RenameSession {
            session,
            title: "Mine".to_owned(),
        })
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    asked(&sandbox, 1);
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(settled_name(&mut connection, session).await, "Mine");
}

#[tokio::test]
async fn turned_off_no_agent_is_asked_and_an_agent_that_fails_leaves_the_first_line() {
    let sandbox = Sandbox::new();
    answers(&sandbox, "Never asked");
    let (_daemon, mut connection, checkout) = project(&sandbox).await;
    let Reply::Done = connection
        .command(Command::ChangePreference(PreferenceChange::NamesByAgent(
            false,
        )))
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    let session = start(&mut connection, checkout, "Add a dark mode to the settings").await;
    assert_eq!(
        settled_name(&mut connection, session).await,
        "Add a dark mode to the settings"
    );
    assert!(sandbox.claude_asked().is_empty());

    let Reply::Done = connection
        .command(Command::ChangePreference(PreferenceChange::NamesByAgent(
            true,
        )))
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    std::fs::write(sandbox.fake_claude_flag("FAKE_CLAUDE_ASK_FAILS"), "").unwrap();
    let session = start(&mut connection, checkout, "Speed up the test suite").await;
    asked(&sandbox, 1);
    assert_eq!(
        settled_name(&mut connection, session).await,
        "Speed up the test suite"
    );
}

#[tokio::test]
async fn a_prompt_of_a_few_words_is_named_again_from_the_first_turn_s_answer() {
    let sandbox = Sandbox::new();
    answers(&sandbox, "Look at this");
    let (_daemon, mut connection, checkout) = project(&sandbox).await;
    let session = start(&mut connection, checkout, "look at this").await;
    assert_eq!(named(&mut connection, session).await, "look at this");
    assert_eq!(named(&mut connection, session).await, "Look at this");
    asked(&sandbox, 1);

    answers(&sandbox, "Flaky login test");
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    launch.fire(
        "Stop",
        &json!({
            "session_id": id,
            "hook_event_name": "Stop",
            "cwd": "/work",
            "last_assistant_message": "The login test was flaky: it raced the redirect.",
        }),
    );
    assert_eq!(named(&mut connection, session).await, "Flaky login test");
    let asked = asked(&sandbox, 2);
    let question = asked[1].1.last().unwrap();
    assert!(question.contains("it raced the redirect"), "{question}");
    // Once only: a second turn asks nothing more.
    launch.fire(
        "Stop",
        &json!({
            "session_id": id,
            "hook_event_name": "Stop",
            "cwd": "/work",
            "last_assistant_message": "Done again.",
        }),
    );
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    assert_eq!(sandbox.claude_asked().len(), 2);
}

#[tokio::test]
async fn an_agent_s_models_are_offered_as_it_lists_them_or_as_muster_suggests() {
    let sandbox = Sandbox::new();
    let (_daemon, mut connection, _) = project(&sandbox).await;
    assert_eq!(
        connection
            .command(Command::ListModels {
                agent: "claude".to_owned()
            })
            .await
            .unwrap(),
        Reply::Models(vec![
            "haiku".to_owned(),
            "sonnet".to_owned(),
            "opus".to_owned()
        ])
    );
    let refused = connection
        .command(Command::ListModels {
            agent: "shell".to_owned(),
        })
        .await
        .unwrap_err();
    assert!(refused.to_string().contains("cannot be asked"), "{refused}");
}
