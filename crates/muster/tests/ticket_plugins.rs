//! End-to-end tests for ticket plugins: Muster's own GitHub plugin
//! (`muster plugin-run github`) speaking the ticket protocol around a
//! stand-in `gh`, and the daemon running it as a child of its own, once,
//! and stopping it as it stops. The ticket suites (`issues`, `trackers`,
//! `hosts`) go through the same plugins unchanged.

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

use serde_json::{Value, json};
use support::Sandbox;

/// A repository whose stand-in `gh` has issue 42.
fn with_an_issue(sandbox: &Sandbox) -> std::path::PathBuf {
    let root = sandbox.repository("app");
    let issue = json!({"number": 42, "title": "Fix the login redirect",
        "url": "https://github.com/o/app/issues/42", "labels": [{"name": "bug"}],
        "author": {"login": "furkan"}, "body": "After signing in, users land on /404."});
    std::fs::write(
        sandbox.fake_gh().join("issues.json"),
        json!([issue]).to_string(),
    )
    .unwrap();
    std::fs::write(sandbox.fake_gh().join("issue-42.json"), issue.to_string()).unwrap();
    root
}

#[test]
fn muster_s_own_github_plugin_speaks_the_ticket_protocol() {
    let sandbox = Sandbox::new();
    let root = with_an_issue(&sandbox);
    let project = json!({"root": root, "remote": "github.com/o/app", "settings": {}});
    let requests = [
        json!({"jsonrpc": "2.0", "id": 1, "method": "hello", "params": {"muster": "0.4.0", "protocol": 1}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "list", "params": {"project": project, "mine": true}}),
        json!({"jsonrpc": "2.0", "id": 3, "method": "view", "params": {"project": project, "key": "42"}}),
        json!({"jsonrpc": "2.0", "id": 4, "method": "view", "params": {"project": project, "key": "999"}}),
        json!({"jsonrpc": "2.0", "id": 5, "method": "close", "params": {"project": project, "key": "42"}}),
    ];
    let mut plugin = sandbox
        .command()
        .args(["plugin-run", "github"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = plugin.stdin.take().unwrap();
    for request in &requests {
        writeln!(stdin, "{request}").unwrap();
    }
    // Its stdin ending ends it, once what is under way is answered.
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
            assert_eq!(reply["jsonrpc"], "2.0");
            (reply["id"].as_u64().unwrap(), reply)
        })
        .collect();
    assert_eq!(replies.len(), requests.len());
    assert_eq!(replies[&1]["result"], json!({"protocol": 1}));
    assert_eq!(
        replies[&2]["result"],
        json!({"where": "GitHub issues", "source": null, "tickets": [{"key": "42",
            "title": "Fix the login redirect", "url": "https://github.com/o/app/issues/42",
            "labels": ["bug"], "author": "furkan"}]})
    );
    assert_eq!(
        replies[&3]["result"]["body"],
        "After signing in, users land on /404."
    );
    assert_eq!(replies[&4]["error"]["code"], "not_found");
    let missing = replies[&4]["error"]["message"].as_str().unwrap();
    assert!(missing.contains("Could not resolve"), "{missing}");
    assert_eq!(replies[&5]["result"], json!({}));

    let calls = std::fs::read_to_string(sandbox.fake_gh().join("calls")).unwrap();
    assert!(
        calls.contains("issue list --state open --limit 100 --json number,title,url,labels,author --assignee @me"),
        "{calls}"
    );
    let closed = std::fs::read_to_string(sandbox.fake_gh().join("closed")).unwrap();
    assert_eq!(closed.trim(), "42");
}

/// The processes whose parent is `parent`, with their command lines.
fn children(parent: u32) -> Vec<(u32, String)> {
    let listed = std::process::Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .unwrap();
    String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let ppid: u32 = fields.next()?.parse().ok()?;
            (ppid == parent).then(|| (pid, fields.collect::<Vec<_>>().join(" ")))
        })
        .collect()
}

fn alive(pid: u32) -> bool {
    std::process::Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .status()
        .unwrap()
        .success()
}

#[test]
fn the_daemon_runs_muster_s_own_plugin_once_and_stops_it_with_itself() {
    let sandbox = Sandbox::new();
    let root = with_an_issue(&sandbox);
    let daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    let plugins = |daemon: &support::Daemon| -> Vec<u32> {
        children(daemon.pid())
            .into_iter()
            .filter(|(_, command)| command.ends_with("muster plugin-run github"))
            .map(|(pid, _)| pid)
            .collect()
    };
    assert_eq!(
        plugins(&daemon),
        Vec::<u32>::new(),
        "none until tickets are asked for"
    );
    for _ in 0..2 {
        let listed = sandbox.run(&["issue", "list", "app"]);
        assert!(listed.status.success());
        assert!(String::from_utf8_lossy(&listed.stdout).contains("#42"));
    }
    let running = plugins(&daemon);
    assert_eq!(running.len(), 1, "one process serves every request");
    daemon.stop();
    assert!(!alive(running[0]), "the plugin outlived the daemon");
}
