//! End-to-end tests for tickets from a shell (`PRODUCT.md` §6b): `muster
//! list tickets`, `muster ticket 42` reading one, `muster ticket 42 claude`
//! putting an agent on it in the workspace the person is in, `assign`, and
//! `close`; refused inside an agent's session (`PRD-01`, `PRD-07`). A
//! stand-in `gh` answers from fixture files.

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

use std::path::{Path, PathBuf};
use std::process::Output;

use muster_core::protocol::{Command, Reply, Snapshot};
use muster_core::{TicketKey, Workspace};
use serde_json::json;

use support::{Daemon, Sandbox};

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    repository: PathBuf,
}

async fn setup() -> Setup {
    let sandbox = Sandbox::new();
    let repository = sandbox.repository("app");
    let gh = sandbox.fake_gh();
    std::fs::write(
        gh.join("issues.json"),
        json!([
            {"number": 42, "title": "Fix the login redirect", "url": "https://github.com/o/app/issues/42",
             "labels": [{"name": "bug"}], "author": {"login": "furkan"}},
            {"number": 7, "title": "Dark mode", "url": "https://github.com/o/app/issues/7",
             "labels": [], "author": {"login": "someone"}}
        ])
        .to_string(),
    )
    .unwrap();
    for (number, title) in [(42, "Fix the login redirect"), (7, "Dark mode")] {
        std::fs::write(
            gh.join(format!("issue-{number}.json")),
            json!({"number": number, "title": title,
            "url": format!("https://github.com/o/app/issues/{number}"),
            "labels": [], "author": {"login": "furkan"},
            "body": "After signing in, users land on /404.",
            "comments": [
                {"author": {"login": "octocat"}, "body": "Seen on `main` too.",
                 "createdAt": "2026-10-22T09:30:00Z", "isMinimized": false}
            ]})
            .to_string(),
        )
        .unwrap();
    }
    let daemon = sandbox.start();
    let reply = daemon
        .connect()
        .await
        .command(Command::AddProject {
            root: repository.clone(),
            name: None,
            space: None,
        })
        .await
        .unwrap();
    assert!(matches!(reply, Reply::ProjectAdded { .. }), "{reply:?}");
    Setup {
        sandbox,
        daemon,
        repository,
    }
}

impl Setup {
    fn run_in(&self, folder: &Path, args: &[&str], env: &[(&str, &str)]) -> Output {
        self.sandbox
            .command()
            .current_dir(folder)
            .args(args)
            .envs(env.iter().copied())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap()
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = self.run_in(&self.repository, args, &[]);
        assert!(
            output.status.success(),
            "`muster {}`: {}",
            args.join(" "),
            text(&output.stderr)
        );
        output
    }

    async fn snapshot(&self) -> Snapshot {
        self.daemon.connect().await.subscribe().await.unwrap()
    }

    /// The workspace on `branch`, once it is there.
    async fn workspace(&self, branch: &str) -> Workspace {
        wait::found_async(&format!("a workspace on {branch}"), async || {
            self.snapshot()
                .await
                .workspaces
                .into_iter()
                .find(|w| w.kind.in_git() && w.kind.branch() == branch)
        })
        .await
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[tokio::test]
async fn tickets_are_listed_and_read_in_the_terminal_prd_6b() {
    let setup = setup().await;
    let listed = text(&setup.run(&["list", "tickets"]).stdout);
    assert!(listed.contains("#42"), "{listed}");
    assert!(listed.contains("Fix the login redirect"), "{listed}");
    assert!(listed.contains("Dark mode"), "{listed}");

    let read = text(&setup.run(&["ticket", "42"]).stdout);
    assert!(read.starts_with("#42  Fix the login redirect"), "{read}");
    assert!(read.contains("users land on /404"), "{read}");
    assert!(read.contains("── octocat"), "{read}");
    assert!(read.contains("No workspace has it"), "{read}");
}

#[tokio::test]
async fn an_agent_on_a_ticket_works_in_the_workspace_the_person_is_in_prd_6b() {
    let setup = setup().await;
    setup.run(&["workspace", "add", "fix-login"]);
    let workspace = setup.workspace("muster/fix-login").await;
    let folder = workspace.kind.host_path().unwrap().to_owned();
    let started = setup.run_in(&folder, &["ticket", "42", "claude"], &[]);
    assert!(started.status.success(), "{}", text(&started.stderr));
    let said = text(&started.stdout);
    assert!(
        said.contains("Started claude on #42 in fix-login (1)"),
        "{said}"
    );

    let snapshot = setup.snapshot().await;
    let now = snapshot
        .workspaces
        .iter()
        .find(|w| w.id == workspace.id)
        .unwrap();
    assert_eq!(now.ticket, TicketKey::parse("42"));
    let agent = snapshot
        .sessions
        .iter()
        .find(|session| session.harness == "claude")
        .unwrap();
    assert_eq!(
        agent.workspace,
        Some(workspace.id),
        "no worktree made for it"
    );
    assert_eq!(snapshot.workspaces.len(), 2);
    let read = text(&setup.run(&["ticket", "42"]).stdout);
    assert!(read.contains("Worked on in fix-login: 1 agent"), "{read}");

    // Assigned elsewhere by name, and no longer.
    setup.run(&["workspace", "add", "dark-mode"]);
    setup.workspace("muster/dark-mode").await;
    setup.run(&["ticket", "7", "assign", "dark-mode"]);
    let dark = setup.workspace("muster/dark-mode").await;
    assert_eq!(dark.ticket, TicketKey::parse("7"));
    let refused = setup.run_in(
        &setup.repository,
        &["ticket", "42", "assign", "dark-mode"],
        &[],
    );
    assert!(!refused.status.success(), "fix-login has #42");
    assert!(
        text(&refused.stderr).contains("already"),
        "{}",
        text(&refused.stderr)
    );
    setup.run(&["ticket", "7", "unassign"]);
    assert_eq!(setup.workspace("muster/dark-mode").await.ticket, None);
}

#[tokio::test]
async fn closing_asks_and_an_agent_may_not_prd_01_prd_07() {
    let setup = setup().await;
    let refused = setup.run_in(&setup.repository, &["ticket", "42", "close"], &[]);
    assert!(!refused.status.success(), "no terminal to ask on, no --yes");
    setup.run(&["ticket", "42", "close", "--yes"]);
    let closed = std::fs::read_to_string(setup.sandbox.fake_gh().join("closed")).unwrap();
    assert_eq!(closed.trim(), "42");

    // Inside an agent's session, which tickets are worked on is not its to say.
    let started = setup.run(&["claude", "--here", "--json", "Plan"]);
    let started: serde_json::Value = serde_json::from_slice(&started.stdout).unwrap();
    let agent = started["session"].as_str().unwrap();
    let inside = setup.run_in(
        &setup.repository,
        &["ticket", "7", "claude"],
        &[("MUSTER_SESSION", agent)],
    );
    assert!(!inside.status.success());
    assert!(
        text(&inside.stderr).contains("agents do not put agents on tickets"),
        "{}",
        text(&inside.stderr)
    );
    // Nor a program an agent `muster exec` ran started, whatever the
    // terminal it runs in (`ARC-28`).
    let under = setup.run_in(
        &setup.repository,
        &["ticket", "7", "claude"],
        &[("MUSTER_AGENT", "codex")],
    );
    assert!(!under.status.success());
    assert!(
        text(&under.stderr).contains("runs under a coding agent"),
        "{}",
        text(&under.stderr)
    );
}
