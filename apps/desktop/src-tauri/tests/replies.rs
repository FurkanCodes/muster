//! What the real daemon answers, and refuses, to the commands the window
//! sends, recorded so the mock daemon can be held to it (`ARC-24`): the
//! webview's own tests (`src/ipc/mock/replies.test.ts`) send the mock the
//! same steps and must get the same answers, ids and times aside.
//!
//! The steps and what the daemon answered are in
//! `src/ipc/mock/replies.json`. This test runs the steps against the real
//! daemon through the link and fails where the daemon now answers
//! otherwise; `pnpm replies` (`MUSTER_WRITE_REPLIES=1`) writes it afresh.
//! Ids the steps make are named (`$project`), as are the repository's path
//! (`$repo`) and an id nothing has (`$missing`).

#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own tasks; ARC-03 and ARC-13 bind the app's own code"
)]

#[expect(
    dead_code,
    reason = "the link's own tests use the rest of the support module"
)]
mod support;

use std::path::Path;

use muster_core::SessionId;
use muster_core::protocol::Command;
use muster_desktop::contract::BridgeError;
use muster_desktop::link::{Link, Options};
use serde_json::{Value, json};
use support::{Sandbox, Webview};

/// What a step does: send a command, find a project's checkout, or wait
/// for a session's program to end.
enum Step {
    Send {
        what: &'static str,
        command: Value,
        save: Option<&'static str>,
    },
    Checkout {
        what: &'static str,
        of: &'static str,
        save: &'static str,
    },
    Exited {
        what: &'static str,
        session: &'static str,
    },
}

fn send(what: &'static str, command: Value) -> Step {
    Step::Send {
        what,
        command,
        save: None,
    }
}

fn saving(what: &'static str, command: Value, save: &'static str) -> Step {
    Step::Send {
        what,
        command,
        save: Some(save),
    }
}

/// An id no project, workspace or session has.
const MISSING: &str = "01JD0000000000000000000999";

/// A terminal's size, as the window sends it.
fn size() -> Value {
    json!({ "cols": 80, "rows": 24 })
}

/// The steps: the commands the window sends, the refusals it shows.
#[expect(clippy::too_many_lines, reason = "the script, one step after another")]
fn steps() -> Vec<Step> {
    vec![
        send("asks whether the daemon answers", json!("Ping")),
        saving(
            "adds a repository as a project",
            json!({ "AddProject": { "root": "$repo", "name": null } }),
            "project",
        ),
        send(
            "adds the same folder again",
            json!({ "AddProject": { "root": "$repo", "name": null } }),
        ),
        send(
            "adds a path that is not absolute",
            json!({ "AddProject": { "root": "repos/app", "name": null } }),
        ),
        send(
            "adds a folder that is not there",
            json!({ "AddProject": { "root": "$repo/nothing-here", "name": null } }),
        ),
        Step::Checkout {
            what: "finds the project's own checkout",
            of: "project",
            save: "checkout",
        },
        send(
            "reads a timeline with nothing kept",
            json!({ "Timeline": { "workspace": "$checkout" } }),
        ),
        send(
            "reads the timeline of a workspace that is not there",
            json!({ "Timeline": { "workspace": "$missing" } }),
        ),
        send(
            "reads an entry the timeline does not have",
            json!({ "EntryDiff": { "workspace": "$checkout", "number": 99 } }),
        ),
        send(
            "reads the changes of a workspace that is not there",
            json!({ "WorkspaceChanges": { "workspace": "$missing" } }),
        ),
        send(
            "reads the diff of a file not changed",
            json!({ "FileDiff": { "workspace": "$checkout", "part": "uncommitted", "path": "README.md" } }),
        ),
        saving(
            "starts a shell",
            json!({ "StartSession": { "workspace": "$checkout", "harness": "shell", "title": null, "size": size(), "prompt": null } }),
            "session",
        ),
        send(
            "starts a session in a workspace that is not there",
            json!({ "StartSession": { "workspace": "$missing", "harness": "shell", "title": null, "size": size(), "prompt": null } }),
        ),
        send(
            "renames a session on one line",
            json!({ "RenameSession": { "session": "$session", "title": "  A\n name  " } }),
        ),
        send(
            "renames a session to nothing",
            json!({ "RenameSession": { "session": "$session", "title": " " } }),
        ),
        send(
            "renames a session that is not there",
            json!({ "RenameSession": { "session": "$missing", "title": "Gone" } }),
        ),
        send(
            "removes a session still running",
            json!({ "RemoveSession": { "session": "$session" } }),
        ),
        send(
            "removes a project with a session running",
            json!({ "RemoveProject": { "project": "$project" } }),
        ),
        send(
            "makes a worktree on a branch git would not take",
            json!({ "CreateWorktree": { "project": "$project", "branch": "bad..name", "on": null } }),
        ),
        saving(
            "makes a worktree on a new branch",
            json!({ "CreateWorktree": { "project": "$project", "branch": "feature", "on": null } }),
            "worktree",
        ),
        send(
            "makes a second worktree on the same branch",
            json!({ "CreateWorktree": { "project": "$project", "branch": "feature", "on": null } }),
        ),
        send(
            "makes a worktree for a project that is not there",
            json!({ "CreateWorktree": { "project": "$missing", "branch": null, "on": null } }),
        ),
        send(
            "stops the shell",
            json!({ "StopSession": { "session": "$session" } }),
        ),
        Step::Exited {
            what: "waits for the shell to end",
            session: "session",
        },
        send(
            "stops the shell again",
            json!({ "StopSession": { "session": "$session" } }),
        ),
        send(
            "stops a session that is not there",
            json!({ "StopSession": { "session": "$missing" } }),
        ),
        send(
            "removes the shell",
            json!({ "RemoveSession": { "session": "$session" } }),
        ),
        send(
            "removes it again",
            json!({ "RemoveSession": { "session": "$session" } }),
        ),
        send(
            "removes a project that has a worktree",
            json!({ "RemoveProject": { "project": "$project" } }),
        ),
        send(
            "removes the worktree",
            json!({ "RemoveWorktree": { "workspace": "$worktree", "force": false } }),
        ),
        send(
            "removes a worktree that is not there",
            json!({ "RemoveWorktree": { "workspace": "$missing", "force": false } }),
        ),
        send(
            "removes the project",
            json!({ "RemoveProject": { "project": "$project" } }),
        ),
        send(
            "removes it again",
            json!({ "RemoveProject": { "project": "$project" } }),
        ),
    ]
}

/// The names given to what the steps make, and the values they stand for.
struct Names(Vec<(String, String)>);

impl Names {
    fn insert(&mut self, name: &str, value: String) {
        self.0.push((format!("${name}"), value));
        // Longest first, so `$repo/x` is filled in before a shorter name.
        self.0
            .sort_by_key(|(_, value)| std::cmp::Reverse(value.len()));
    }

    /// `text` with each name filled in.
    fn fill(&self, text: &str) -> String {
        self.0.iter().fold(text.to_owned(), |text, (name, value)| {
            text.replace(name, value)
        })
    }

    /// `text` with each value named.
    fn name(&self, text: &str) -> String {
        self.0.iter().fold(text.to_owned(), |text, (name, value)| {
            text.replace(value, name)
        })
    }
}

/// The id in a reply that made something (`ProjectAdded`, `WorktreeCreated`, `SessionStarted`).
fn made(reply: &Value) -> String {
    reply
        .as_object()
        .and_then(|reply| reply.values().next())
        .and_then(|made| made.get("id"))
        .and_then(Value::as_str)
        .expect("a reply that made something")
        .to_owned()
}

async fn record(sandbox: &Sandbox, repo: &Path) -> Value {
    let (link, task) = Link::new(Options {
        socket: sandbox.paths().socket(),
        start_daemon: None,
    });
    tokio::spawn(task);
    let (mut webview, states) = Webview::new();
    link.open(states).unwrap();
    webview.until("the state", Webview::up).await;

    let mut names = Names(Vec::new());
    names.insert("repo", repo.display().to_string());
    names.insert("missing", MISSING.to_owned());
    let mut recorded = Vec::new();
    for step in steps() {
        match step {
            Step::Send {
                what,
                command,
                save,
            } => {
                let filled: Command =
                    serde_json::from_str(&names.fill(&command.to_string())).unwrap();
                let outcome = match link.command(filled).await {
                    Ok(reply) => {
                        let reply = serde_json::to_value(reply).unwrap();
                        if let Some(save) = save {
                            names.insert(save, made(&reply));
                        }
                        json!({ "reply": reply })
                    }
                    Err(BridgeError::Refused { code, message }) => {
                        json!({ "refused": { "code": code, "message": message } })
                    }
                    Err(other) => panic!("{what}: {other:?}"),
                };
                let outcome: Value =
                    serde_json::from_str(&names.name(&outcome.to_string())).unwrap();
                recorded.push(
                    json!({ "what": what, "send": command, "save": save, "outcome": outcome }),
                );
            }
            Step::Checkout { what, of, save } => {
                // Ids as the protocol writes them, which is how the steps name them.
                let project = json!(names.fill(&format!("${of}")));
                let of_project = |w: &&muster_core::Workspace| json!(w.project) == project;
                webview
                    .until("the project's checkout", |webview| {
                        webview.workspaces.values().any(|w| of_project(&w))
                    })
                    .await;
                let checkout = webview.workspaces.values().find(of_project).unwrap().id;
                names.insert(save, json!(checkout).as_str().unwrap().to_owned());
                recorded.push(json!({ "what": what, "checkout": format!("${of}"), "save": save }));
            }
            Step::Exited { what, session } => {
                let id: SessionId =
                    serde_json::from_value(json!(names.fill(&format!("${session}")))).unwrap();
                webview
                    .until(what, |webview| {
                        webview
                            .sessions
                            .get(&id)
                            .is_some_and(|s| s.status.is_exited())
                    })
                    .await;
                recorded.push(json!({ "what": what, "exited": format!("${session}") }));
            }
        }
    }
    Value::Array(recorded)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_mock_is_held_to_what_the_real_daemon_answers_arc_24() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let repo = sandbox.repository("app");
    let recorded = record(&sandbox, &repo).await;
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/ipc/mock/replies.json");
    let text = format!("{}\n", serde_json::to_string_pretty(&recorded).unwrap());
    if std::env::var_os("MUSTER_WRITE_REPLIES").is_some() {
        std::fs::write(&fixture, text).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(&fixture).unwrap_or_default();
    assert!(
        committed == text,
        "the daemon now answers otherwise than src/ipc/mock/replies.json says: \
         run `pnpm replies`, make the mock answer the same, and commit both.\nNow:\n{text}"
    );
}
