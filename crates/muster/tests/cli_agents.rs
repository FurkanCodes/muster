//! End-to-end tests for the verbs that drive agents from a shell
//! (`PRODUCT.md` §6b): `muster claude "…"` starting an agent in a worktree
//! of its own and returning, `ls`, `send`, `prompt`, `wait` and `logs`, and
//! agents setting agents working within the person's limits (`PRD-01`). The
//! stand-in `claude` records how it was started and then is a shell; its
//! hooks are fired as Claude Code would fire them.

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

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Child, Output, Stdio};
use std::time::{Duration, Instant};

use muster_core::protocol::{Command, Reply, Snapshot, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use serde_json::json;

use support::{ClaudeLaunch, Daemon, Sandbox};

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    repository: PathBuf,
}

/// A daemon with one project, `app`, and `settings` as Muster's own.
async fn setup_with(settings: &str) -> Setup {
    let sandbox = Sandbox::new();
    let repository = sandbox.repository("app");
    let config = sandbox.paths().config_dir;
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("settings.toml"), settings).unwrap();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let reply = connection
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

async fn setup() -> Setup {
    setup_with("").await
}

impl Setup {
    /// `muster` with `args`, run in `folder` with `env` and `stdin`, waited
    /// for (ten seconds at most).
    fn run_in(&self, folder: &Path, args: &[&str], env: &[(&str, &str)], stdin: &str) -> Output {
        let mut child = self.spawn_in(folder, args, env);
        child
            .stdin
            .take()
            .unwrap()
            .write_all(stdin.as_bytes())
            .unwrap();
        finish(child, args)
    }

    fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.repository, args, &[], "")
    }

    fn spawn_in(&self, folder: &Path, args: &[&str], env: &[(&str, &str)]) -> Child {
        self.sandbox
            .command()
            .current_dir(folder)
            .args(args)
            .envs(env.iter().copied())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    }

    fn spawn(&self, args: &[&str]) -> Child {
        let mut child = self.spawn_in(&self.repository, args, &[]);
        drop(child.stdin.take());
        child
    }

    async fn snapshot(&self) -> Snapshot {
        self.daemon.connect().await.subscribe().await.unwrap()
    }

    /// `muster claude --json …`: the session it started.
    fn start_json(&self, args: &[&str], env: &[(&str, &str)]) -> SessionId {
        let mut all = vec!["claude", "--json"];
        all.extend_from_slice(args);
        let output = self.run_in(&self.repository, &all, env, "");
        assert!(output.status.success(), "{}", text(&output.stderr));
        let started: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        started["session"].as_str().unwrap().parse().unwrap()
    }

    /// The stand-in's first start in `session`, with its hooks.
    fn launch(&self, session: SessionId) -> ClaudeLaunch {
        self.sandbox.wait_for_launches(session, 1).remove(0)
    }

    /// Waits until `session` has `status`.
    async fn until(&self, session: SessionId, status: SessionStatus) {
        wait::until_async(&format!("{session} to be {status:?}"), async || {
            self.snapshot()
                .await
                .sessions
                .into_iter()
                .find(|found| found.id == session)
                .map(|found| found.status)
                == Some(status)
        })
        .await;
    }
}

/// Waits for `child`, ten seconds at most.
fn finish(mut child: Child, args: &[&str]) -> Output {
    if wait::within(wait::WAIT, || child.try_wait().unwrap()).is_none() {
        let _ = child.kill();
        let output = child.wait_with_output().unwrap();
        panic!(
            "`muster {}` still ran after {:?}; stderr:\n{}",
            args.join(" "),
            wait::WAIT,
            text(&output.stderr)
        );
    }
    child.wait_with_output().unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Fires `event` from `launch` as Claude Code would, with `extra` in it.
fn fire(launch: &ClaudeLaunch, event: &str, extra: serde_json::Value) {
    let mut payload = json!({
        "session_id": launch.value("--session-id").unwrap(),
        "hook_event_name": event,
        "cwd": "/work",
        "transcript_path": "/claude/projects/-work/x.jsonl",
    });
    let serde_json::Value::Object(extra) = extra else {
        panic!("a hook's fields: {extra}");
    };
    payload.as_object_mut().unwrap().extend(extra);
    launch.fire(event, &payload);
}

fn branch_of(snapshot: &Snapshot, session: SessionId) -> String {
    let workspace = snapshot
        .sessions
        .iter()
        .find(|found| found.id == session)
        .unwrap()
        .workspace
        .unwrap();
    workspace_branch(snapshot, workspace)
}

fn workspace_branch(snapshot: &Snapshot, workspace: WorkspaceId) -> String {
    snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .unwrap()
        .kind
        .branch()
        .to_owned()
}

#[tokio::test]
async fn an_agent_starts_on_a_prompt_in_a_worktree_named_for_it_and_the_shell_comes_back() {
    let setup = setup().await;
    let started = Instant::now();
    let output = setup.run(&["claude", "Fix", "the", "login", "redirect"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    let said = text(&output.stdout);
    assert!(said.starts_with("Started "), "{said}");
    assert!(said.contains("on muster/fix-the-login-redirect"), "{said}");
    assert!(said.contains("muster attach "), "{said}");
    // The same again: another worktree, never the first one's.
    let again = setup.run(&["claude", "Fix the login redirect"]);
    assert!(again.status.success(), "{}", text(&again.stderr));
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "it waited for nothing"
    );

    let snapshot = setup.snapshot().await;
    let agents: Vec<_> = snapshot
        .sessions
        .iter()
        .filter(|session| session.harness == "claude")
        .collect();
    assert_eq!(agents.len(), 2);
    let mut branches: Vec<String> = agents
        .iter()
        .map(|session| branch_of(&snapshot, session.id))
        .collect();
    branches.sort();
    assert_eq!(
        branches,
        [
            "muster/fix-the-login-redirect",
            "muster/fix-the-login-redirect-2"
        ]
    );
    for session in agents {
        assert_eq!(session.status, SessionStatus::Running);
        assert_eq!(session.started_by, None, "the person started it");
        let launch = setup.launch(session.id);
        assert_eq!(launch.args.last().unwrap(), "Fix the login redirect");
    }
}

#[tokio::test]
async fn here_reads_the_prompt_from_standard_input_and_works_in_the_checkout() {
    let setup = setup().await;
    let output = setup.run_in(
        &setup.repository,
        &["claude", "--here", "--json", "-"],
        &[],
        "Tidy the README\nand nothing else\n",
    );
    assert!(output.status.success(), "{}", text(&output.stderr));
    let started: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(started["format"], 2);
    assert_eq!(started["branch"], "main");
    let session: SessionId = started["session"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        setup.launch(session).args.last().unwrap(),
        "Tidy the README\nand nothing else\n"
    );
    assert_eq!(branch_of(&setup.snapshot().await, session), "main");

    // `list` names it by the id the other verbs take.
    let listed = setup.run(&["list"]);
    let line = text(&listed.stdout);
    assert!(line.contains(started["short"].as_str().unwrap()), "{line}");
    assert!(line.contains("Tidy the README"), "{line}");
    let json = setup.run(&["list", "--json"]);
    let list: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(list["sessions"][0]["id"], session.to_string());
}

#[tokio::test]
async fn an_unknown_agent_is_refused_and_its_worktree_goes_with_it() {
    let setup = setup().await;
    let output = setup.run(&["no-such-agent", "do it"]);
    assert!(!output.status.success());
    let snapshot = setup.snapshot().await;
    assert!(snapshot.sessions.is_empty());
    assert!(
        snapshot.workspaces.iter().all(|w| w.kind.is_checkout()),
        "{:?}",
        snapshot.workspaces
    );
}

#[tokio::test]
async fn wait_exits_as_the_agent_rested_and_prints_its_answer() {
    let setup = setup().await;
    let session = setup.start_json(&["Fix it"], &[]);
    let short = session.ulid().to_string().to_lowercase()[20..].to_owned();
    let launch = setup.launch(session);

    // Whether it is waiting already or not: a session that has finished
    // its turn has rested.
    let waiting = setup.spawn(&["wait", &short]);
    fire(
        &launch,
        "Stop",
        json!({ "last_assistant_message": "Fixed the login." }),
    );
    let output = finish(waiting, &["wait"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(text(&output.stdout).trim(), "Fixed the login.");
    assert!(text(&output.stderr).contains("finished its turn"));

    let reply = setup.run(&["logs", "--reply", &short]);
    assert_eq!(text(&reply.stdout).trim(), "Fixed the login.");

    // Asking for a permission: exit 3.
    fire(
        &launch,
        "UserPromptSubmit",
        json!({ "prompt": "and the tests" }),
    );
    setup.until(session, SessionStatus::Running).await;
    let waiting = setup.spawn(&["wait", &short]);
    fire(&launch, "PermissionRequest", json!({ "tool_name": "Bash" }));
    let output = finish(waiting, &["wait"]);
    assert_eq!(output.status.code(), Some(3), "{}", text(&output.stderr));

    // Out of time while it works: 124.
    fire(&launch, "PostToolUse", json!({ "tool_name": "Bash" }));
    setup.until(session, SessionStatus::Running).await;
    let output = setup.run(&["wait", &short, "--timeout", "1"]);
    assert_eq!(output.status.code(), Some(124), "{}", text(&output.stderr));
}

#[tokio::test]
async fn send_types_into_a_session_and_logs_shows_its_screen() {
    let setup = setup().await;
    let session = setup.start_json(&["--here", "Look around"], &[]);
    setup.launch(session);
    // The stand-in is a shell once started: what is sent runs there.
    let id = session.to_string();
    let output = setup.run(&["send", &id, "echo", "sent-$((40+2))"]);
    assert!(output.status.success(), "{}", text(&output.stderr));
    wait::until("sent-42 in its terminal", || {
        text(&setup.run(&["logs", &id]).stdout).contains("sent-42")
    });
    let last = setup.run(&["logs", "-n", "1", &id]);
    assert_eq!(text(&last.stdout).lines().count(), 1);
}

#[tokio::test]
async fn prompt_sends_and_waits_for_the_answer() {
    let setup = setup().await;
    let session = setup.start_json(&["First"], &[]);
    let launch = setup.launch(session);
    fire(
        &launch,
        "Stop",
        json!({ "last_assistant_message": "first answer" }),
    );
    setup.until(session, SessionStatus::Finished).await;

    let id = session.to_string();
    let prompting = setup.spawn(&["prompt", &id, "Second"]);
    // The stand-in echoes what it is given: the prompt went in.
    wait::until("the prompt in its terminal", || {
        text(&setup.run(&["logs", &id]).stdout).contains("Second")
    });
    fire(&launch, "UserPromptSubmit", json!({ "prompt": "Second" }));
    setup.until(session, SessionStatus::Running).await;
    fire(
        &launch,
        "Stop",
        json!({ "last_assistant_message": "second answer" }),
    );
    let output = finish(prompting, &["prompt"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout).trim(),
        "second answer",
        "not the first turn's"
    );
}

#[tokio::test]
async fn prd_01_an_agent_may_start_agents_and_what_it_starts_names_it() {
    let setup = setup().await;
    let parent = setup.start_json(&["Plan the work"], &[]);
    let child = setup.start_json(&["Do part one"], &[("MUSTER_SESSION", &parent.to_string())]);
    let snapshot = setup.snapshot().await;
    let started = snapshot.sessions.iter().find(|s| s.id == child).unwrap();
    assert_eq!(started.started_by, Some(parent));
    let listed = setup.run(&["list"]);
    assert!(
        text(&listed.stdout).contains("started by"),
        "{}",
        text(&listed.stdout)
    );

    // A shell of the person's is the person.
    let mut connection = setup.daemon.connect().await;
    let workspace = started.workspace.unwrap();
    let Reply::SessionStarted { id: shell } = connection
        .command(Command::StartSession {
            workspace: Some(workspace),
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
            space: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let by_person = setup.start_json(&["Do part two"], &[("MUSTER_SESSION", &shell.to_string())]);
    let snapshot = setup.snapshot().await;
    let started = snapshot
        .sessions
        .iter()
        .find(|s| s.id == by_person)
        .unwrap();
    assert_eq!(started.started_by, None);
}

#[tokio::test]
async fn prd_01_agents_start_agents_only_within_the_persons_limits() {
    let setup = setup_with("[delegation]\nat_once = 1\n").await;
    let parent = setup.start_json(&["Plan the work"], &[]);
    let env = [("MUSTER_SESSION", parent.to_string())];
    let env: Vec<(&str, &str)> = env.iter().map(|(k, v)| (*k, v.as_str())).collect();
    setup.start_json(&["Part one"], &env);
    let refused = setup.run_in(&setup.repository, &["claude", "Part two"], &env, "");
    assert!(!refused.status.success());
    assert!(
        text(&refused.stderr).contains("at_once"),
        "{}",
        text(&refused.stderr)
    );
    // Its worktree went with it.
    let snapshot = setup.snapshot().await;
    assert_eq!(snapshot.sessions.len(), 2);
    assert!(
        !snapshot
            .workspaces
            .iter()
            .any(|w| w.kind.branch().starts_with("muster/part-two")),
        "{:?}",
        snapshot.workspaces
    );
}

#[tokio::test]
async fn prd_01_turned_off_no_agent_starts_one() {
    let setup = setup_with("[delegation]\nallowed = false\n").await;
    let parent = setup.start_json(&["Plan the work"], &[]);
    let refused = setup.run_in(
        &setup.repository,
        &["claude", "Part one"],
        &[("MUSTER_SESSION", &parent.to_string())],
        "",
    );
    assert!(!refused.status.success());
    let said = text(&refused.stderr);
    assert!(said.contains("Agents start agents is off"), "{said}");
}
