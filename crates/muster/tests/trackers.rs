//! End-to-end tests for trackers named in a project's `[tickets]`: Jira
//! and Linear, through stand-ins for their APIs, with the tokens the
//! person would give in the daemon's environment.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use serde_json::json;
use support::http::serve;
use support::{Sandbox, git};

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn work_item() -> serde_json::Value {
    json!({"id": "10001", "key": "APP-7", "fields": {
        "summary": "Fix the login redirect", "labels": ["bug"],
        "reporter": {"displayName": "Some One"},
        "description": {"type": "doc", "content": [{"type": "paragraph", "content": [
            {"type": "text", "text": "After signing in, users land on /404."}]}]}
    }})
}

/// Stands in for Jira Cloud's REST API (v3): work item APP-7, its search,
/// and its transitions, one of them into Done.
fn fake_jira() -> (String, support::http::Recorded) {
    serve(|request, _| {
        let path = request.target.split('?').next().unwrap_or_default();
        match (request.method.as_str(), path) {
            ("POST", "/rest/api/3/search/jql") => (
                200,
                json!({"issues": [work_item()], "isLast": true}).to_string(),
            ),
            ("GET", "/rest/api/3/issue/APP-7") => (200, work_item().to_string()),
            ("GET", "/rest/api/3/issue/APP-7/transitions") => (
                200,
                json!({"transitions": [
                    {"id": "11", "to": {"statusCategory": {"key": "indeterminate"}}},
                    {"id": "31", "to": {"statusCategory": {"key": "done"}}}
                ]})
                .to_string(),
            ),
            ("POST", "/rest/api/3/issue/APP-7/transitions") => (204, String::new()),
            _ => (404, json!({"errorMessages": ["no such thing"]}).to_string()),
        }
    })
}

#[test]
fn jira_s_tickets_are_listed_started_and_closed_with_a_token() {
    let (jira, asked) = fake_jira();
    let sandbox = Sandbox::new()
        .with_env("MUSTER_JIRA_API", &format!("{jira}/rest/api/3"))
        .with_env("JIRA_API_TOKEN", "t0ken")
        .with_env("JIRA_EMAIL", "me@example.com");
    let root = sandbox.repository("app");
    std::fs::write(
        root.join("muster.toml"),
        "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"app\"\n",
    )
    .unwrap();
    git(&root, &["add", "muster.toml"]);
    git(
        &root,
        &["commit", "--quiet", "--message", "Tickets are in Jira"],
    );
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );

    let listed = sandbox.run(&["issue", "list", "app"]);
    assert!(listed.status.success(), "{}", said(&listed));
    let listed = said(&listed);
    assert!(
        listed.contains("APP-7   Fix the login redirect  [bug]"),
        "{listed}"
    );

    let started = sandbox.run(&["issue", "start", "app", "app-7", "--harness", "shell"]);
    assert!(started.status.success(), "{}", said(&started));
    let status = said(&sandbox.run(&["status"]));
    assert!(
        status.contains("muster/issue-APP-7-fix-the-login-redirect"),
        "{status}"
    );
    assert!(
        status.contains("APP-7 Fix the login redirect"),
        "the session is named for it: {status}"
    );

    let closed = sandbox.run(&["issue", "close", "app", "APP-7"]);
    assert!(closed.status.success(), "{}", said(&closed));
    assert!(said(&closed).contains("Closed APP-7."));

    let requests = asked.lock().unwrap().clone();
    // Basic auth with the email and the token, on every request.
    assert!(
        requests
            .iter()
            .all(|request| request.header("authorization")
                == Some("Basic bWVAZXhhbXBsZS5jb206dDBrZW4=")),
        "{requests:?}"
    );
    let search = requests
        .iter()
        .find(|request| request.target == "/rest/api/3/search/jql")
        .unwrap();
    assert_eq!(
        search.json()["jql"],
        "project = \"APP\" AND statusCategory != Done ORDER BY created DESC"
    );
    let moved = requests
        .iter()
        .find(|request| request.method == "POST" && request.target.ends_with("/transitions"))
        .unwrap();
    assert_eq!(moved.json(), json!({"transition": {"id": "31"}}));
}

#[test]
fn without_a_jira_sign_in_it_says_how_to_sign_in() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    std::fs::write(
        root.join("muster.toml"),
        "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"APP\"\n",
    )
    .unwrap();
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    let refused = sandbox.run(&["issue", "list", "app"]);
    assert!(!refused.status.success());
    let refused = said(&refused);
    assert!(
        refused.contains("not signed in to Jira at acme.atlassian.net"),
        "{refused}"
    );
    assert!(refused.contains("muster auth jira"), "{refused}");
}

#[test]
fn a_plain_folder_s_linear_issues_are_listed_and_closed_with_a_key() {
    let (linear, asked) = serve(|request, _| {
        let body = request.json();
        let query = body["query"].as_str().unwrap_or_default();
        let data = if query.contains("issues(") {
            json!({"issues": {"nodes": [{"identifier": "ENG-42", "title": "Fix the login redirect",
                "url": "https://linear.app/acme/issue/ENG-42", "createdAt": "2026-10-01T00:00:00Z",
                "labels": {"nodes": [{"name": "bug"}]}}]}})
        } else if query.contains("issueUpdate") {
            json!({"issueUpdate": {"success": true}})
        } else if query.contains("states") {
            json!({"issue": {"team": {"states": {"nodes": [
                {"id": "st-done", "name": "Done", "type": "completed", "position": 1}]}}}})
        } else {
            json!({"issue": {"identifier": "ENG-42", "title": "Fix the login redirect", "url": "u", "description": "It loops."}})
        };
        (200, json!({ "data": data }).to_string())
    });
    let sandbox = Sandbox::new()
        .with_env("MUSTER_LINEAR_API", &format!("{linear}/graphql"))
        .with_env("LINEAR_API_KEY", "lin_api_k");
    let folder = sandbox.root().join("notes");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("muster.toml"),
        "[tickets]\ntracker = \"linear\"\nteam = \"eng\"\n",
    )
    .unwrap();
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", folder.to_str().unwrap()])
            .status
            .success()
    );

    let listed = sandbox.run(&["issue", "list", "notes"]);
    assert!(listed.status.success(), "{}", said(&listed));
    assert!(
        said(&listed).contains("ENG-42  Fix the login redirect  [bug]"),
        "{}",
        said(&listed)
    );
    let closed = sandbox.run(&["issue", "close", "notes", "ENG-42"]);
    assert!(closed.status.success(), "{}", said(&closed));

    let requests = asked.lock().unwrap().clone();
    assert!(
        requests
            .iter()
            .all(|request| request.header("authorization") == Some("lin_api_k"))
    );
    assert_eq!(requests[0].json()["variables"]["team"], "ENG");
    let update = requests
        .iter()
        .find(|request| {
            request.json()["query"]
                .as_str()
                .unwrap_or_default()
                .contains("issueUpdate")
        })
        .unwrap();
    assert_eq!(
        update.json()["variables"],
        json!({"id": "ENG-42", "state": "st-done"})
    );
    // Work on it needs a worktree, which a folder has not.
    let refused = said(&sandbox.run(&["issue", "start", "notes", "ENG-42"]));
    assert!(
        refused.contains("worktrees for tickets need one"),
        "{refused}"
    );
}

#[test]
fn mine_narrows_every_tracker_to_the_person_s_own() {
    let (jira, jira_asked) = fake_jira();
    let (linear, linear_asked) =
        serve(|_, _| (200, json!({"data": {"issues": {"nodes": []}}}).to_string()));
    let sandbox = Sandbox::new()
        .with_env("MUSTER_JIRA_API", &format!("{jira}/rest/api/3"))
        .with_env("JIRA_API_TOKEN", "t0ken")
        .with_env("JIRA_EMAIL", "me@example.com")
        .with_env("MUSTER_LINEAR_API", &format!("{linear}/graphql"))
        .with_env("LINEAR_API_KEY", "lin_api_k");
    std::fs::write(sandbox.fake_gh().join("issues.json"), "[]").unwrap();
    std::fs::write(sandbox.fake_glab().join("issues.json"), "[]").unwrap();
    let project = |name: &str, toml: &str, remote: Option<&str>| {
        let root = sandbox.repository(name);
        if let Some(remote) = remote {
            git(&root, &["remote", "add", "origin", remote]);
        }
        std::fs::write(root.join("muster.toml"), toml).unwrap();
        root
    };
    let roots = [
        project("onhub", "[tickets]\nmine = true\n", None),
        project(
            "onlab",
            "[tickets]\nmine = true\n",
            Some("git@gitlab.example.com:g/onlab.git"),
        ),
        project(
            "onjira",
            "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"APP\"\nmine = true\n",
            None,
        ),
        project(
            "onlinear",
            "[tickets]\ntracker = \"linear\"\nteam = \"ENG\"\nmine = true\n",
            None,
        ),
    ];
    let _daemon = sandbox.start();
    for root in &roots {
        assert!(
            sandbox
                .run(&["project", "add", root.to_str().unwrap()])
                .status
                .success()
        );
    }
    for name in ["onhub", "onlab", "onjira", "onlinear"] {
        let listed = sandbox.run(&["issue", "list", name]);
        assert!(listed.status.success(), "{name}: {}", said(&listed));
    }

    let gh = std::fs::read_to_string(sandbox.fake_gh().join("calls")).unwrap();
    assert!(gh.contains("issue list --state open --limit 100 --json number,title,url,labels,author --assignee @me"), "{gh}");
    let glab = std::fs::read_to_string(sandbox.fake_glab().join("calls")).unwrap();
    assert!(
        glab.contains("issue list --output json --per-page 100 --assignee=@me"),
        "{glab}"
    );
    let search = jira_asked.lock().unwrap()[0].json();
    assert_eq!(
        search["jql"],
        "(project = \"APP\" AND statusCategory != Done) AND assignee = currentUser() ORDER BY created DESC"
    );
    let query = linear_asked.lock().unwrap()[0].json()["query"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(query.contains("isMe"), "{query}");

    let config = said(&sandbox.run(&["project", "config", "onjira"]));
    assert!(
        config.contains("tickets       Jira at acme.atlassian.net, project APP, assigned to you"),
        "{config}"
    );
}
