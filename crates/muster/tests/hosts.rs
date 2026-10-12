//! End-to-end tests for git hosts and trackers beside GitHub: GitLab's
//! issues and merge requests through a stand-in `glab`, chosen by the
//! project's remote or its `muster.toml`.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::path::PathBuf;

use support::{Sandbox, git};

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// A repository whose `origin` is on a GitLab host, with GitLab issue 12.
fn on_gitlab(sandbox: &Sandbox) -> PathBuf {
    let root = sandbox.repository("app");
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "git@gitlab.example.com:g/app.git",
        ],
    );
    let issue = serde_json::json!({
        "id": 991, "iid": 12, "title": "Fix the login redirect", "state": "opened",
        "labels": ["bug"], "author": {"username": "someone"},
        "web_url": "https://gitlab.example.com/g/app/-/issues/12",
    });
    let glab = sandbox.fake_glab();
    std::fs::write(
        glab.join("issues.json"),
        serde_json::json!([issue]).to_string(),
    )
    .unwrap();
    let mut detail = issue.clone();
    detail["description"] = serde_json::Value::from("After signing in, users land on /404.");
    std::fs::write(glab.join("issue-12.json"), detail.to_string()).unwrap();
    root
}

#[test]
fn a_gitlab_project_s_tickets_come_through_glab() {
    let sandbox = Sandbox::new();
    let root = on_gitlab(&sandbox);
    let _daemon = sandbox.start();
    let added = sandbox.run(&["project", "add", root.to_str().unwrap()]);
    assert!(added.status.success(), "{}", said(&added));

    let listed = sandbox.run(&["issue", "list", "app"]);
    assert!(listed.status.success(), "{}", said(&listed));
    let listed = said(&listed);
    assert!(listed.contains("#12"), "{listed}");
    assert!(listed.contains("Fix the login redirect  [bug]"), "{listed}");

    let started = sandbox.run(&["issue", "start", "app", "12", "--harness", "shell"]);
    assert!(started.status.success(), "{}", said(&started));
    assert!(said(&started).contains("for #12 in"), "{}", said(&started));
    let status = said(&sandbox.run(&["status"]));
    assert!(
        status.contains("muster/issue-12-fix-the-login-redirect"),
        "{status}"
    );

    let closed = sandbox.run(&["issue", "close", "app", "#12"]);
    assert!(closed.status.success(), "{}", said(&closed));
    assert!(said(&closed).contains("Closed #12."));
    let glab = sandbox.fake_glab();
    assert_eq!(
        std::fs::read_to_string(glab.join("closed")).unwrap().trim(),
        "12"
    );
    let calls = std::fs::read_to_string(glab.join("calls")).unwrap();
    assert!(
        calls.contains("issue list --output json --per-page 100"),
        "{calls}"
    );
    assert!(calls.contains("issue view 12 --output json"), "{calls}");
    assert!(
        !std::fs::read_to_string(sandbox.fake_gh().join("calls"))
            .unwrap_or_default()
            .contains("issue"),
        "GitHub's gh was not asked"
    );
}

#[test]
fn a_merge_request_by_hand_is_a_draft_through_glab_and_found_again() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let remote = root.parent().unwrap().join("remote.git");
    git(
        root.parent().unwrap(),
        &[
            "clone",
            "--bare",
            "--quiet",
            root.to_str().unwrap(),
            remote.to_str().unwrap(),
        ],
    );
    git(
        &root,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    // A local `origin` names no host: the project says it is GitLab.
    std::fs::write(
        root.join("muster.toml"),
        "[pull_requests]\nhost = \"gitlab\"\n",
    )
    .unwrap();
    git(&root, &["add", "muster.toml"]);
    git(&root, &["commit", "--quiet", "--message", "Name the host"]);
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        sandbox
            .run(&["worktree", "add", "app", "--branch", "feature"])
            .status
            .success()
    );
    let tree = sandbox.root().join("worktrees/app/feature");
    std::fs::write(tree.join("fix.txt"), "fixed\n").unwrap();
    git(&tree, &["add", "fix.txt"]);
    git(
        &tree,
        &["commit", "--quiet", "--message", "Fix the login redirect"],
    );

    let opened = sandbox.run(&["pr", "feature"]);
    assert!(opened.status.success(), "{}", said(&opened));
    assert_eq!(
        String::from_utf8_lossy(&opened.stdout),
        "Draft pull request #17: https://gitlab.example.com/g/app/-/merge_requests/17\n"
    );
    // Muster pushed it with git; glab only opened the request.
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "feature"]),
        "Fix the login redirect"
    );
    let glab = sandbox.fake_glab();
    let asked: Vec<String> = std::fs::read_to_string(glab.join("mr-create"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    for (flag, value) in [
        ("--source-branch", "feature"),
        ("--target-branch", "main"),
        ("--title", "Fix the login redirect"),
    ] {
        let at = asked
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("{flag}"));
        assert_eq!(asked[at + 1], value, "{asked:?}");
    }
    assert!(asked.contains(&"--draft".to_owned()) && asked.contains(&"--yes".to_owned()));
    assert!(!asked.iter().any(|arg| arg == "--push" || arg == "--fill"));

    // Again: the open one is found, not a second opened.
    let again = sandbox.run(&["pr", "feature"]);
    assert!(again.status.success(), "{}", said(&again));
    assert!(said(&again).contains("merge_requests/17"));
    let calls = std::fs::read_to_string(glab.join("calls")).unwrap();
    assert_eq!(calls.matches("mr create").count(), 1, "{calls}");
}

#[test]
fn a_project_whose_remote_names_no_host_says_what_to_write() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    git(
        &root,
        &[
            "remote",
            "add",
            "origin",
            "https://code.example.com/g/app.git",
        ],
    );
    let _daemon = sandbox.start();
    assert!(
        sandbox
            .run(&["project", "add", root.to_str().unwrap()])
            .status
            .success()
    );
    let listed = sandbox.run(&["issue", "list", "app"]);
    assert!(!listed.status.success());
    let listed = said(&listed);
    assert!(
        listed.contains("code.example.com is not a host Muster knows"),
        "{listed}"
    );
    assert!(listed.contains("[tickets] tracker"), "{listed}");
}
