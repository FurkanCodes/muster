//! Runs end to end against the real Docker (`MUSTER_DOCKER_TESTS=1`): the
//! real daemon and command line, real containers and git, a stand-in
//! `claude` in the run's image and a stand-in `gh` on the host. They prove
//! `PRODUCT.md`'s M2 bar: an issue becomes a reviewed pull request marked
//! ready, a daemon killed mid-build resumes the run, and a person can take
//! a run over in its container and hand it back.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::path::{Path, PathBuf};
use std::process::Command as Process;
use std::time::{Duration, Instant};

use muster_core::protocol::{Command, Reply};
use muster_core::{ProjectId, Run, RunStage, RunTransition, SessionId};
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::json;

use support::{Daemon, Sandbox, git, read_terminal};

/// Stands in for Claude Code in a run's container. Headless (`-p`), it
/// speaks stream-json: a reviewer (`--allowedTools`) approves; a builder
/// writes `fix.txt` and commits, after `slow-build`'s seconds on its first
/// turn, and adds `fixed` once told the checks failed. Interactive, for a
/// person taking the run over, a shell stands in for it.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
session=""; review=no; headless=no; previous=""
for arg in "$@"; do
  case "$previous" in --session-id|--resume) session="$arg" ;; esac
  case "$arg" in -p) headless=yes ;; --allowedTools) review=yes ;; esac
  previous="$arg"
done
if [ "$headless" = no ]; then
  echo "interactive claude on $session, credential ${CLAUDE_CODE_OAUTH_TOKEN:+given}"
  exec sh
fi
prompt="$(cat)"
printf '{"type":"system","subtype":"init","session_id":"%s","model":"fake"}\n' "$session"
mkdir -p "$HOME/.claude/projects/-work" && touch "$HOME/.claude/projects/-work/$session.jsonl"
[ "$CLAUDE_CODE_OAUTH_TOKEN" = "fake-token" ] || { echo "no credential" >&2; exit 9; }
if [ "$review" = yes ]; then
  printf '{"type":"result","subtype":"success","is_error":false,"result":"Fine. {\\"verdict\\":\\"approve\\",\\"summary\\":\\"Good.\\"}","total_cost_usd":0.1,"num_turns":1}\n'
  exit 0
fi
case "$prompt" in
  *"checks failed"*) echo fixed >> fix.txt ;;
  *) sleep "$(cat slow-build 2>/dev/null || echo 0)"; echo first > fix.txt ;;
esac
git add fix.txt && git commit --quiet --message "Fix the thing"
printf '{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.25,"num_turns":2}\n'
"#;

/// Bump with any change to the image.
const IMAGE: &str = "muster-e2e-claude:1";

fn docker_tests() -> bool {
    std::env::var("MUSTER_DOCKER_TESTS").as_deref() == Ok("1")
}

/// Builds the runs' image, unless it is there.
fn image() -> &'static str {
    let there = Process::new("docker")
        .args(["image", "inspect", "--format", "{{.Id}}", IMAGE])
        .output()
        .unwrap();
    if there.status.success() {
        return IMAGE;
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Dockerfile"),
        "FROM alpine:3\nRUN apk add --no-cache git\nCOPY claude /usr/local/bin/claude\n\
         RUN chmod +x /usr/local/bin/claude\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("claude"), FAKE_CLAUDE).unwrap();
    let built = Process::new("docker")
        .args(["build", "--quiet", "--tag", IMAGE])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    IMAGE
}

/// Removes a project's run containers and cache when dropped, whatever
/// the test did.
struct Cleanup(Option<ProjectId>);

impl Drop for Cleanup {
    fn drop(&mut self) {
        let Some(project) = self.0 else {
            return;
        };
        let listed = Process::new("docker")
            .args(["ps", "--all", "--quiet", "--filter"])
            .arg(format!("label=muster.project={project}"))
            .output();
        if let Ok(listed) = listed {
            for id in String::from_utf8_lossy(&listed.stdout).split_whitespace() {
                let _ = Process::new("docker").args(["rm", "--force", id]).output();
            }
        }
        let _ = Process::new("docker")
            .args(["volume", "rm", "--force"])
            .arg(format!("muster-cache-{project}"))
            .output();
    }
}

/// A sandbox on the real Docker, with project `app`: its checkout has an
/// `origin` to push to, `muster.toml` and issue #42.
fn sandbox(muster_toml: &str, slow_build: Option<u32>) -> (Sandbox, PathBuf, PathBuf) {
    let sandbox = Sandbox::new()
        .with_real_docker()
        .with_env("MUSTER_RUN_IMAGE", image())
        .with_env("CLAUDE_CODE_OAUTH_TOKEN", "fake-token")
        .with_env("MUSTER_PR_MS", "300");
    let root = sandbox.repository("app");
    std::fs::write(root.join("muster.toml"), muster_toml).unwrap();
    if let Some(seconds) = slow_build {
        std::fs::write(root.join("slow-build"), seconds.to_string()).unwrap();
    }
    git(&root, &["add", "--all"]);
    git(&root, &["commit", "--quiet", "--message", "Set up runs"]);
    let remote = sandbox.root().join("remote.git");
    git(
        sandbox.root(),
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
    std::fs::write(
        sandbox.fake_gh().join("issue-42.json"),
        json!({"number": 42, "title": "Fix the thing", "url": "https://github.com/o/app/issues/42",
               "labels": [], "author": {"login": "someone"}, "body": "Make fix.txt say fixed."})
        .to_string(),
    )
    .unwrap();
    (sandbox, root, remote)
}

fn muster(sandbox: &Sandbox, args: &[&str]) -> String {
    let output = sandbox.run(args);
    assert!(
        output.status.success(),
        "`muster {}` failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

async fn add_project(daemon: &Daemon, root: &Path) -> ProjectId {
    match daemon
        .connect()
        .await
        .command(Command::AddProject {
            root: root.to_owned(),
            name: None,
        })
        .await
        .unwrap()
    {
        Reply::ProjectAdded { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

/// Waits, up to `limit`, until the only run is at `stage`; returns it and
/// its timeline.
async fn wait_for_stage(
    daemon: &Daemon,
    stage: RunStage,
    limit: Duration,
) -> (Run, Vec<RunTransition>) {
    let deadline = Instant::now() + limit;
    let mut connection = daemon.connect().await;
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        if let [run] = snapshot.runs.as_slice() {
            let Reply::RunHistory { transitions } = connection
                .command(Command::RunHistory { run: run.id })
                .await
                .unwrap()
            else {
                panic!("expected RunHistory");
            };
            if run.stage == stage {
                return (run.clone(), transitions);
            }
            assert!(
                Instant::now() < deadline,
                "the run is at {:?}, not {stage:?}: {:?} {transitions:#?}\n{}",
                run.stage,
                run.reason,
                daemon.log()
            );
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn branch_log(remote: &Path) -> Vec<String> {
    git(
        remote,
        &["log", "--format=%s", "muster/issue-42-fix-the-thing"],
    )
    .lines()
    .map(str::to_owned)
    .collect()
}

fn containers_of(run: &Run) -> String {
    let listed = Process::new("docker")
        .args(["ps", "--all", "--quiet", "--filter"])
        .arg(format!("label=muster.run={}", run.id))
        .output()
        .unwrap();
    String::from_utf8_lossy(&listed.stdout).trim().to_owned()
}

#[tokio::test]
async fn real_docker_takes_an_issue_to_a_ready_pull_request_through_a_daemon_killed_mid_build() {
    if !docker_tests() {
        return;
    }
    let toml = "[checks]\ncommands = [\"grep -q fixed fix.txt\"]\n";
    let (sandbox, root, remote) = sandbox(toml, Some(4));
    let mut cleanup = Cleanup(None);
    let daemon = sandbox.start();
    cleanup.0 = Some(add_project(&daemon, &root).await);
    muster(&sandbox, &["run", "start", "app", "42"]);

    // Killed outright while the agent works in the container.
    wait_for_stage(&daemon, RunStage::Building, Duration::from_secs(120)).await;
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let pid = Pid::from_raw(i32::try_from(daemon.pid()).unwrap());
    kill(pid, Signal::SIGKILL).unwrap();
    drop(daemon);

    // A new daemon follows the same agent's turn, not a second one, and
    // carries the run on: the checks fail once, the agent fixes it, the
    // branch is pushed, the pull request opened, reviewed and made ready.
    let daemon = sandbox.start();
    let (run, timeline) =
        wait_for_stage(&daemon, RunStage::ReadyForYou, Duration::from_secs(180)).await;
    let stages: Vec<RunStage> = timeline.iter().map(|step| step.to).collect();
    assert_eq!(
        stages,
        [
            RunStage::Provisioning,
            RunStage::Building,
            RunStage::Checking,
            RunStage::Building,
            RunStage::Checking,
            RunStage::OpeningPr,
            RunStage::AgentReview,
            RunStage::ReadyForYou,
        ]
    );
    assert_eq!(run.attempt, 2);
    assert_eq!(run.cost_micros, Some(600_000));
    assert_eq!(
        branch_log(&remote)[..2],
        ["Fix the thing", "Fix the thing"],
        "one commit per attempt"
    );
    assert_eq!(
        git(&remote, &["show", "muster/issue-42-fix-the-thing:fix.txt"]),
        "first\nfixed"
    );
    let gh = sandbox.fake_gh();
    let calls = std::fs::read_to_string(gh.join("calls")).unwrap();
    assert_eq!(calls.matches("pr create").count(), 1, "{calls}");
    let body = std::fs::read_to_string(gh.join("pr-body")).unwrap();
    assert!(body.starts_with("Closes #42."), "{body}");
    let reviews = std::fs::read_to_string(gh.join("reviews")).unwrap();
    assert!(reviews.contains("reviewer agent approves"), "{reviews}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !std::fs::read_to_string(gh.join("ready")).is_ok_and(|ready| ready.contains("17")) {
        assert!(Instant::now() < deadline, "never marked ready");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Merged on GitHub: the run ends and its container goes.
    std::fs::write(gh.join("pr-state"), "MERGED").unwrap();
    let (merged, _) = wait_for_stage(&daemon, RunStage::Merged, Duration::from_secs(30)).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !containers_of(&merged).is_empty() {
        assert!(Instant::now() < deadline, "the container stayed");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

#[tokio::test]
async fn real_docker_lets_a_person_take_a_run_over_in_its_container_and_hand_it_back() {
    if !docker_tests() {
        return;
    }
    // One attempt, and checks the agent cannot pass: the person can.
    let toml = "[checks]\ncommands = [\"grep -q person fix.txt\"]\n\n[limits]\nattempts = 1\n";
    let (sandbox, root, remote) = sandbox(toml, None);
    let mut cleanup = Cleanup(None);
    let daemon = sandbox.start();
    cleanup.0 = Some(add_project(&daemon, &root).await);
    muster(&sandbox, &["run", "start", "app", "42"]);
    let (run, _) = wait_for_stage(&daemon, RunStage::NeedsYou, Duration::from_secs(120)).await;
    assert_eq!(
        run.reason.as_deref(),
        Some("the checks failed after 1 attempt(s)")
    );

    let taken = muster(&sandbox, &["run", "take-over", "42"]);
    let session: SessionId = taken
        .split_whitespace()
        .find(|word| word.starts_with("ses_"))
        .and_then(|word| word.trim_end_matches(',').parse().ok())
        .unwrap_or_else(|| panic!("no session in {taken:?}"));
    wait_for_stage(&daemon, RunStage::TakenOver, Duration::from_secs(10)).await;
    let status = muster(&sandbox, &["status"]);
    assert!(status.contains("(the container of run #42)"), "{status}");

    // The person's terminal is the agent's, in the container, carrying on
    // its conversation with the runs' credential.
    let mut terminal = daemon.connect().await;
    terminal.attach(session, None).await.unwrap();
    let (text, _) = read_terminal(&mut terminal, session, "credential given").await;
    assert!(text.contains("interactive claude on "), "{text}");
    terminal
        .input(
            session,
            b"echo person >> fix.txt && git add -A && git commit -qm 'By hand' && echo DONE-$((40+2))\r",
        )
        .await
        .unwrap();
    read_terminal(&mut terminal, session, "DONE-42").await;
    let mut connection = daemon.connect().await;

    // Handed back, the person's work goes through the checks and on to
    // ready; their session in the container stops.
    muster(&sandbox, &["run", "hand-back", "42"]);
    let (run, timeline) =
        wait_for_stage(&daemon, RunStage::ReadyForYou, Duration::from_secs(120)).await;
    let handed: Vec<(RunStage, &str)> = timeline
        .iter()
        .rev()
        .take(5)
        .rev()
        .map(|step| (step.to, step.reason.as_str()))
        .collect();
    assert_eq!(
        handed,
        [
            (RunStage::TakenOver, "you took over"),
            (RunStage::Checking, "you handed it back"),
            (RunStage::OpeningPr, "the checks passed"),
            (RunStage::AgentReview, "pull request #17 is open"),
            (RunStage::ReadyForYou, "the reviewer approved"),
        ]
    );
    let log = branch_log(&remote);
    assert_eq!(log[..2], ["By hand", "Fix the thing"], "{log:?}");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        let ended = snapshot
            .sessions
            .iter()
            .any(|found| found.id == session && found.ended_at.is_some());
        if ended {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the person's session kept running"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // Cancelled, the run's container and its workspace go.
    muster(&sandbox, &["run", "cancel", "42"]);
    wait_for_stage(&daemon, RunStage::Cancelled, Duration::from_secs(10)).await;
    let deadline = Instant::now() + Duration::from_secs(30);
    while !containers_of(&run).is_empty() {
        assert!(Instant::now() < deadline, "the container stayed");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let snapshot = connection.subscribe().await.unwrap();
    assert!(
        snapshot
            .workspaces
            .iter()
            .all(|workspace| workspace.kind.run().is_none())
    );
}
