//! End-to-end tests for trackers named in a project's `[tickets]`: Jira
//! and Linear, through stand-ins for their APIs, with the tokens the
//! person would give in the daemon's environment. Muster's own plugins
//! serve them (`muster plugin-run jira|linear`), so these go through the
//! ticket protocol, and the plugins are asked by it here too.

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

use std::collections::HashMap;
use std::io::Write as _;
use std::process::Stdio;

use muster_core::protocol::{Command, Reply};
use serde_json::{Value, json};
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

    let listed = sandbox.run(&["list", "tickets", "-p", "app"]);
    assert!(listed.status.success(), "{}", said(&listed));
    let listed = said(&listed);
    assert!(
        listed.contains("APP-7   Fix the login redirect  [bug]"),
        "{listed}"
    );

    let started = sandbox.run(&["ticket", "app-7", "shell", "-p", "app"]);
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

    let closed = sandbox.run(&["ticket", "APP-7", "close", "-p", "app", "-y"]);
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
    let refused = sandbox.run(&["list", "tickets", "-p", "app"]);
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

    let listed = sandbox.run(&["list", "tickets", "-p", "notes"]);
    assert!(listed.status.success(), "{}", said(&listed));
    assert!(
        said(&listed).contains("ENG-42  Fix the login redirect  [bug]"),
        "{}",
        said(&listed)
    );
    let closed = sandbox.run(&["ticket", "ENG-42", "close", "-p", "notes", "-y"]);
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
    let refused = said(&sandbox.run(&["ticket", "ENG-42", "claude", "-p", "notes"]));
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
        let listed = sandbox.run(&["list", "tickets", "-p", name]);
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

/// Repositories whose tickets are in the same Jira project say so alike,
/// and a ticket there can be worked on in either: the UI offers them
/// together and asks which (`PRODUCT.md` §6, Tickets).
#[tokio::test]
async fn projects_listing_the_same_jira_tickets_share_a_source() {
    let (jira, _asked) = fake_jira();
    let sandbox = Sandbox::new()
        .with_env("MUSTER_JIRA_API", &format!("{jira}/rest/api/3"))
        .with_env("JIRA_API_TOKEN", "t0ken")
        .with_env("JIRA_EMAIL", "me@example.com");
    let jira_project = |key: &str| {
        format!(
            "[tickets]\ntracker = \"jira\"\nsite = \"acme.atlassian.net\"\nproject = \"{key}\"\n"
        )
    };
    let mut roots = Vec::new();
    for (name, key) in [("web", "APP"), ("api", "APP"), ("ops", "OPS")] {
        let root = sandbox.repository(name);
        std::fs::write(root.join("muster.toml"), jira_project(key)).unwrap();
        git(&root, &["add", "muster.toml"]);
        git(
            &root,
            &["commit", "--quiet", "--message", "Tickets are in Jira"],
        );
        roots.push(root);
    }
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let mut sources = Vec::new();
    for root in &roots {
        let Reply::ProjectAdded { id } = connection
            .command(Command::AddProject {
                root: root.clone(),
                name: None,
                space: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected ProjectAdded");
        };
        let Reply::Issues { source, .. } = connection
            .command(Command::ListIssues { project: id })
            .await
            .unwrap()
        else {
            panic!("expected Issues");
        };
        sources.push(source.expect("Jira's tickets are no repository's own"));
    }
    assert_eq!(sources[0], sources[1], "web and api list the same tickets");
    assert_ne!(sources[0], sources[2], "ops lists another project's");
    assert!(!sources[0].contains("t0ken") && !sources[0].contains("me@example.com"));

    // The ticket goes to the repository the person chose.
    let started = sandbox.run(&["ticket", "APP-7", "shell", "-p", "api"]);
    assert!(started.status.success(), "{}", said(&started));
    let status = said(&sandbox.run(&["status"]));
    let api = status.split("\napi ").nth(1).expect(&status);
    assert!(
        api.contains("muster/issue-APP-7-fix-the-login-redirect"),
        "{status}"
    );
    let web = status.split("\nweb ").nth(1).expect(&status);
    let web = web.split("\napi ").next().unwrap_or(web);
    assert!(!web.contains("issue-APP-7"), "{status}");
}

/// A request of the ticket protocol.
fn request(id: u64, method: &str, params: Value) -> Value {
    let mut request = json!({"jsonrpc": "2.0", "id": id, "method": method});
    request["params"] = params;
    request
}

/// Muster's own ticket plugin `id` (`muster plugin-run jira|linear`), in
/// the sandbox's environment as the daemon gives it, asked `requests` in
/// one go: its replies, by id.
fn ask_plugin(sandbox: &Sandbox, id: &str, requests: &[Value]) -> HashMap<u64, Value> {
    let mut plugin = sandbox
        .command()
        .args(["plugin-run", id])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = plugin.stdin.take().unwrap();
    for request in requests {
        writeln!(stdin, "{request}").unwrap();
    }
    drop(stdin);
    let output = plugin.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let replies: HashMap<u64, Value> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let reply: Value = serde_json::from_str(line).unwrap();
            (reply["id"].as_u64().unwrap(), reply)
        })
        .collect();
    assert_eq!(replies.len(), requests.len(), "{replies:?}");
    replies
}

#[test]
fn muster_s_own_jira_plugin_speaks_the_ticket_protocol() {
    let (jira, asked) = fake_jira();
    let sandbox = Sandbox::new()
        .with_env("MUSTER_JIRA_API", &format!("{jira}/rest/api/3"))
        .with_env("JIRA_API_TOKEN", "t0ken")
        .with_env("JIRA_EMAIL", "me@example.com");
    // What the project's `[tickets]` says comes as its settings.
    let project = json!({"root": "/work/app", "remote": null,
        "settings": {"site": "acme.atlassian.net", "project": "APP"}});
    let replies = ask_plugin(
        &sandbox,
        "jira",
        &[
            request(1, "hello", json!({"muster": "0.4.0", "protocol": 1})),
            request(2, "list", json!({"project": project, "mine": true})),
            request(3, "view", json!({"project": project, "key": "APP-7"})),
            request(4, "view", json!({"project": project, "key": "APP-9"})),
            request(5, "close", json!({"project": project, "key": "APP-7"})),
            request(
                6,
                "list",
                json!({"project": {"root": "/work/app", "settings": {"site": "not a site", "project": "APP"}}}),
            ),
        ],
    );
    assert_eq!(replies[&1]["result"], json!({"protocol": 1}));
    let mine = "(project = \"APP\" AND statusCategory != Done) AND assignee = currentUser() \
                ORDER BY created DESC";
    assert_eq!(
        replies[&2]["result"],
        json!({"where": "Jira (acme.atlassian.net)",
            "source": format!("jira https://acme.atlassian.net {mine}"),
            "tickets": [{"key": "APP-7", "title": "Fix the login redirect",
                "url": "https://acme.atlassian.net/browse/APP-7", "labels": ["bug"],
                "author": "Some One"}]})
    );
    assert_eq!(replies[&3]["result"]["ticket"]["key"], "APP-7");
    assert_eq!(
        replies[&3]["result"]["body"],
        "After signing in, users land on /404."
    );
    assert_eq!(replies[&4]["error"]["code"], "not_found");
    assert_eq!(replies[&5]["result"], json!({}));
    assert_eq!(replies[&6]["error"]["code"], "not_configured");
    let unconfigured = replies[&6]["error"]["message"].as_str().unwrap();
    assert!(unconfigured.contains("not a Jira site"), "{unconfigured}");

    let requests = asked.lock().unwrap().clone();
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
    assert_eq!(search.json()["jql"], mine);
    let moved = requests
        .iter()
        .find(|request| request.method == "POST" && request.target.ends_with("/transitions"))
        .unwrap();
    assert_eq!(moved.json(), json!({"transition": {"id": "31"}}));
}

#[test]
fn muster_s_own_linear_plugin_speaks_the_ticket_protocol() {
    let (linear, asked) = serve(|request, _| {
        let body = request.json();
        let query = body["query"].as_str().unwrap_or_default();
        let data = if query.contains("issues(") {
            json!({"issues": {"nodes": [{"identifier": "ENG-42", "title": "Fix the login redirect",
                "url": "https://linear.app/acme/issue/ENG-42", "createdAt": "2026-10-01T00:00:00Z",
                "labels": {"nodes": [{"name": "bug"}]}, "creator": {"name": "Some One"}}]}})
        } else if query.contains("issueUpdate") {
            json!({"issueUpdate": {"success": true}})
        } else if query.contains("states") {
            json!({"issue": {"team": {"states": {"nodes": [
                {"id": "st-done", "name": "Done", "type": "completed", "position": 1}]}}}})
        } else if body["variables"]["id"] == "ENG-42" {
            json!({"issue": {"identifier": "ENG-42", "title": "Fix the login redirect",
                "url": "https://linear.app/acme/issue/ENG-42", "description": "It loops."}})
        } else {
            return (
                200,
                json!({"errors": [{"message": "Entity not found: Issue",
                    "extensions": {"code": "ENTITY_NOT_FOUND"}}]})
                .to_string(),
            );
        };
        (200, json!({ "data": data }).to_string())
    });
    let sandbox = Sandbox::new()
        .with_env("MUSTER_LINEAR_API", &format!("{linear}/graphql"))
        .with_env("LINEAR_API_KEY", "lin_api_k");
    let project = json!({"root": "/work/notes", "settings": {"team": "ENG"}});
    let replies = ask_plugin(
        &sandbox,
        "linear",
        &[
            request(1, "hello", json!({"muster": "0.4.0", "protocol": 1})),
            request(2, "list", json!({"project": project, "mine": false})),
            request(3, "view", json!({"project": project, "key": "ENG-42"})),
            request(4, "view", json!({"project": project, "key": "ENG-9"})),
            request(5, "close", json!({"project": project, "key": "ENG-42"})),
            request(6, "list", json!({"project": {"root": "/work/notes"}})),
        ],
    );
    assert_eq!(replies[&1]["result"], json!({"protocol": 1}));
    assert_eq!(
        replies[&2]["result"],
        json!({"where": "Linear (ENG)", "source": "linear ENG all",
            "tickets": [{"key": "ENG-42", "title": "Fix the login redirect",
                "url": "https://linear.app/acme/issue/ENG-42", "labels": ["bug"],
                "author": "Some One"}]})
    );
    assert_eq!(replies[&3]["result"]["body"], "It loops.");
    assert_eq!(replies[&4]["error"]["code"], "not_found");
    assert_eq!(replies[&5]["result"], json!({}));
    assert_eq!(replies[&6]["error"]["code"], "not_configured");

    let requests = asked.lock().unwrap().clone();
    assert!(
        requests
            .iter()
            .all(|request| request.header("authorization") == Some("lin_api_k"))
    );
    let listing = requests
        .iter()
        .find(|request| {
            request.json()["query"]
                .as_str()
                .unwrap()
                .contains("issues(")
        })
        .unwrap();
    assert_eq!(listing.json()["variables"]["team"], "ENG");
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
}

/// Signed in to neither, each of Muster's own plugins says how to sign in,
/// whatever it is asked: the Tickets screen shows it.
#[test]
fn muster_s_own_jira_and_linear_plugins_signed_out_say_how_to_sign_in() {
    let sandbox = Sandbox::new();
    for (id, settings) in [
        (
            "jira",
            json!({"site": "acme.atlassian.net", "project": "APP"}),
        ),
        ("linear", json!({"team": "ENG"})),
    ] {
        let project = json!({"root": "/work/app", "settings": settings});
        let key = if id == "jira" { "APP-7" } else { "ENG-7" };
        let replies = ask_plugin(
            &sandbox,
            id,
            &[
                request(1, "list", json!({"project": project})),
                request(2, "view", json!({"project": project, "key": key})),
                request(3, "close", json!({"project": project, "key": key})),
            ],
        );
        for reply in replies.values() {
            assert_eq!(reply["error"]["code"], "signed_out", "{id}: {reply}");
            let message = reply["error"]["message"].as_str().unwrap();
            assert!(
                message.contains(&format!("run `muster auth {id}`")),
                "{id}: {message}"
            );
        }
    }
}
