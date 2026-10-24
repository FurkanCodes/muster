//! End-to-end tests for what the read commands print with `--json`, for
//! the person's own scripts: one document per command, with its `format`,
//! nothing secret in it (`PRD-06`) and no control character raw
//! (`PRD-07`).

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

use serde_json::{Value, json};

use support::Sandbox;

/// Runs `muster` with `args`, which must succeed, and returns what it
/// printed.
fn said(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = sandbox.run(args);
    assert!(
        output.status.success(),
        "muster {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// What `muster` printed for `args`, read as one JSON document.
fn document(sandbox: &Sandbox, args: &[&str]) -> Value {
    let text = said(sandbox, args);
    serde_json::from_str(&text).unwrap_or_else(|error| panic!("{error}: {text}"))
}

#[test]
fn status_and_the_session_list_are_one_json_document_each() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    said(&sandbox, &["project", "add", root.to_str().unwrap()]);
    said(
        &sandbox,
        &["workspace", "add", "-p", "app", "--branch", "fix-login"],
    );
    said(
        &sandbox,
        &["term", "new", "Look around", "--in", "fix-login", "--bg"],
    );
    let listed = document(&sandbox, &["list", "--json"]);
    let session = listed["sessions"][0]["id"]
        .as_str()
        .expect("the session id")
        .to_owned();

    let status = document(&sandbox, &["status", "--json"]);
    assert_eq!(status["format"], 2, "{status}");
    assert_eq!(status["running"], true);
    assert_eq!(status["daemon"]["pid"], daemon.pid());
    assert_eq!(
        status["daemon"]["version"],
        env!("CARGO_PKG_VERSION"),
        "{status}"
    );
    let projects = status["projects"].as_array().unwrap();
    assert_eq!(projects.len(), 1, "{status}");
    assert_eq!(projects[0]["name"], "app");
    assert_eq!(projects[0]["path"], root.display().to_string());
    let workspaces = projects[0]["workspaces"].as_array().unwrap();
    let checkout = workspaces
        .iter()
        .find(|workspace| workspace["kind"] == "checkout")
        .unwrap();
    assert_eq!(checkout["branch"], "main");
    let worktree = workspaces
        .iter()
        .find(|workspace| workspace["kind"] == "worktree")
        .unwrap();
    assert_eq!(worktree["branch"], "fix-login");
    assert_eq!(worktree["stacked_on"], Value::Null);
    let sessions = worktree["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "{status}");
    assert_eq!(sessions[0]["id"], session.as_str());
    assert_eq!(sessions[0]["harness"], "shell");
    assert_eq!(sessions[0]["title"], "Look around");
    assert_eq!(sessions[0]["status"], "fresh");
    assert_eq!(sessions[0]["needs"], Value::Null);
    assert!(sessions[0]["created_at_ms"].as_i64().unwrap() > 0);

    let listed = document(&sandbox, &["list", "--json"]);
    assert_eq!(listed["format"], 2);
    let sessions = listed["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 1, "{listed}");
    assert_eq!(sessions[0]["id"], session.as_str());
    assert_eq!(sessions[0]["branch"], "fix-login");
    assert_eq!(sessions[0]["project"], projects[0]["id"]);
    assert_eq!(sessions[0]["workspace"], worktree["id"]);

    // A workspace's sessions are given as every list gives one, with their
    // number there.
    let workspaces = document(&sandbox, &["list", "workspaces", "-p", "app", "--json"]);
    let under = workspaces["workspaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workspace| workspace["name"] == "fix-login")
        .unwrap();
    assert_eq!(under["sessions"][0]["id"], session.as_str(), "{workspaces}");
    assert_eq!(under["sessions"][0]["number"], 1);
    assert_eq!(under["sessions"][0]["status"], "fresh");

    // Words stay the default.
    let words = said(&sandbox, &["list"]);
    assert!(words.contains("Look around"), "{words}");
    assert!(words.contains("fix-login"), "{words}");
    assert!(serde_json::from_str::<Value>(&words).is_err());

    // An ended session says how it ended.
    said(&sandbox, &["stop", "fix-login/1"]);
    let found = wait::found("the session to have ended", || {
        let listed = document(&sandbox, &["list", "--json"]);
        Some(listed["sessions"][0].clone()).filter(|found| found["status"] == "exited")
    });
    assert!(found["success"].is_boolean(), "{found}");
    assert!(found["ended_at_ms"].as_i64().is_some(), "{found}");
}

#[test]
fn status_in_json_says_so_when_no_daemon_runs() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["status", "--json"]);
    assert!(!output.status.success());
    let read: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(read, json!({"format": 2, "running": false}));
    let listed = sandbox.run(&["list", "--json"]);
    assert!(
        !listed.status.success(),
        "a list from no daemon says so, as status does"
    );
    let read: Value = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(read, json!({"format": 2, "running": false}));
}

#[test]
fn a_ticket_list_in_json_has_its_agents_and_keeps_its_credentials_hidden() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    // Made up, in the shape people paste.
    let token = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZGEiLCJleHAiOjk5fQ.c2lnbmF0dXJlLWZvci10ZXN0cw";
    let hostile = format!("Fix it\u{1b}]0;owned\u{7} Bearer {token} \u{9b}6n now");
    let issue = json!({"number": 3, "title": hostile, "url": "https://github.com/o/app/issues/3",
                       "labels": [{"name": "b\u{1b}[31mug"}], "author": {"login": "x"}});
    std::fs::write(
        sandbox.fake_gh().join("issues.json"),
        json!([issue.clone(), {"number": 5, "title": "Dark mode",
               "url": "https://github.com/o/app/issues/5", "labels": [], "author": {"login": "y"}}])
        .to_string(),
    )
    .unwrap();
    let mut viewed = issue;
    viewed["body"] = json!("teh");
    std::fs::write(sandbox.fake_gh().join("issue-3.json"), viewed.to_string()).unwrap();
    let _daemon = sandbox.start();
    said(&sandbox, &["project", "add", root.to_str().unwrap()]);
    said(&sandbox, &["ticket", "3", "shell", "-p", "app"]);

    let text = said(&sandbox, &["list", "tickets", "-p", "app", "--json"]);
    assert!(!text.contains(token), "the token was printed: {text}");
    assert!(
        !text.chars().any(|c| c.is_control() && c != '\n'),
        "a control character reached the terminal: {text:?}"
    );
    let listed: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(listed["format"], 2);
    assert_eq!(listed["tracker"], "GitHub issues");
    assert_eq!(listed["mine"], false);
    let tickets = listed["tickets"].as_array().unwrap();
    assert_eq!(tickets.len(), 2, "{listed}");
    let three = tickets.iter().find(|ticket| ticket["key"] == "#3").unwrap();
    // Its text as the UI shows it: no control character, no credential.
    assert_eq!(
        three["title"],
        "Fix it]0;owned Bearer [hidden by Muster] 6n now"
    );
    assert_eq!(three["labels"], json!(["b[31mug"]));
    assert_eq!(three["url"], "https://github.com/o/app/issues/3");
    let agents = three["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1, "{listed}");
    assert_eq!(agents[0]["harness"], "shell");
    assert!(
        agents[0]["branch"]
            .as_str()
            .unwrap()
            .starts_with("muster/issue-3-fix-it"),
        "{listed}"
    );
    let five = tickets.iter().find(|ticket| ticket["key"] == "#5").unwrap();
    assert_eq!(five["agents"], json!([]));
}
