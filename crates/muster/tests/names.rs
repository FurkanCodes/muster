//! End-to-end test of what chats are called (`PRODUCT.md` §6): a chat is
//! named by its first prompt, given in the dialog or typed in its
//! terminal (as Claude Code reports it), its credentials hidden; later
//! prompts leave the name be; and the person renames it, from the real
//! daemon with a stand-in Claude Code.

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
use muster_core::{SessionId, WorkspaceId};
use serde_json::json;

use support::{Sandbox, expect_event};

async fn start(
    connection: &mut Connection,
    workspace: WorkspaceId,
    prompt: Option<&str>,
) -> SessionId {
    let Reply::SessionStarted { id } = connection
        .command(Command::StartSession {
            workspace: Some(workspace),
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: prompt.map(str::to_owned),
            started_by: None,
            space: None,
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

/// Every name told for `session` until it is `wanted`.
async fn named_until(connection: &mut Connection, session: SessionId, wanted: &str) -> Vec<String> {
    let mut told = Vec::new();
    loop {
        let name = named(connection, session).await;
        let done = name == wanted;
        told.push(name);
        if done {
            return told;
        }
    }
}

#[tokio::test]
async fn a_chat_is_named_by_its_first_prompt_and_renamed_by_the_person() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { .. } = connection
        .command(Command::AddProject {
            root,
            name: None,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let checkout = snapshot.workspaces[0].id;

    // From the dialog: its first line.
    let given = start(
        &mut connection,
        checkout,
        Some("Fix the login redirect\nUsers land on /404."),
    )
    .await;
    assert_eq!(
        named(&mut connection, given).await,
        "Fix the login redirect"
    );

    // Typed in its terminal: the first prompt Claude Code reports.
    let typed = start(&mut connection, checkout, None).await;
    assert_eq!(named(&mut connection, typed).await, "Claude Code");
    let launch = sandbox.wait_for_launches(typed, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let prompt = |text: &str| {
        launch.fire(
            "UserPromptSubmit",
            &json!({
                "session_id": id,
                "hook_event_name": "UserPromptSubmit",
                "cwd": "/work",
                "prompt": text,
            }),
        );
    };
    let token = format!("ghp_{}", "q7W8e9R0t1Y2u3I4o5P6".repeat(2));
    prompt(&format!("Push it with {token}"));
    let name = named(&mut connection, typed).await;
    assert!(name.starts_with("Push it with "), "{name}");
    assert!(name.contains("[hidden by Muster]"), "{name}");
    assert!(!name.contains(&token), "{name}");
    // A later prompt leaves the name be.
    launch.fire(
        "Stop",
        &json!({ "session_id": id, "hook_event_name": "Stop", "cwd": "/work" }),
    );
    prompt("Something else entirely");
    let Reply::Done = connection
        .command(Command::RenameSession {
            session: typed,
            title: "  Release\nnotes ".to_owned(),
        })
        .await
        .unwrap()
    else {
        panic!("expected Done");
    };
    // Only the person's name follows: the later prompt named nothing.
    let told = named_until(&mut connection, typed, "Release notes").await;
    assert!(
        told.iter().all(|name| !name.contains("Something else")),
        "{told:?}"
    );

    let refused = connection
        .command(Command::RenameSession {
            session: typed,
            title: " \n ".to_owned(),
        })
        .await
        .unwrap_err();
    assert!(
        matches!(&refused, ClientError::Daemon(body) if body.code == ErrorCode::BadRequest),
        "{refused}"
    );

    // Kept over a restart of the daemon's view: the state says it too.
    let snapshot = connection.subscribe().await.unwrap();
    let kept = snapshot
        .sessions
        .iter()
        .find(|session| session.id == typed)
        .unwrap();
    assert_eq!(kept.title, "Release notes");

    // Neither a prompt nor a credential reaches the daemon's log.
    let log = daemon.log();
    assert!(!log.contains("Users land on"), "{log}");
    assert!(!log.contains(&token), "{log}");
}
