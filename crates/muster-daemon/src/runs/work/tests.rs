//! The real work against real Docker and git, when asked
//! (`MUSTER_DOCKER_TESTS=1`): a stand-in `claude` in a test image, a fake
//! git host and fake credentials, and a real run machine fed by the
//! work's reports.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use muster_core::config::{ProjectConfig, RunLimits};
use muster_core::{RunId, RunStage, Ticket};
use muster_state::run::{Limits, RunMachine};
use tokio::sync::mpsc;

use super::*;
use crate::git::tests::{git, repository};
use crate::github::{BoxFuture, GitHubError};
use crate::runs::provision::tests::test_image;

/// Stands in for Claude Code in the container: a builder that commits
/// (and fixes its work when told the checks failed), or, with
/// `--allowedTools`, a reviewer that approves. It speaks stream-json.
const FAKE_CLAUDE: &str = r#"#!/bin/sh
prompt="$(cat)"
session=""; review=no
while [ $# -gt 0 ]; do
  case "$1" in --session-id|--resume) session="$2"; shift ;; --allowedTools) review=yes ;; esac
  shift
done
printf '{"type":"system","subtype":"init","session_id":"%s","model":"fake"}\n' "$session"
mkdir -p "$HOME/.claude/projects/-work" && touch "$HOME/.claude/projects/-work/$session.jsonl"
env | grep -q '^ANTHROPIC_API_KEY=fake-key$' || { echo "no credential" >&2; exit 9; }
if [ "$review" = yes ]; then
  printf '{"type":"result","subtype":"success","is_error":false,"result":"Fine. {\\"verdict\\":\\"approve\\",\\"summary\\":\\"Good.\\"}","total_cost_usd":0.1,"num_turns":1}\n'
  exit 0
fi
case "$prompt" in
  *"checks failed"*) echo fixed >> fix.txt ;;
  *) echo first > fix.txt ;;
esac
git add fix.txt && git commit --quiet --message "Fix the thing"
printf '{"type":"result","subtype":"success","is_error":false,"result":"Done.","total_cost_usd":0.25,"num_turns":2}\n'
"#;

async fn fake_claude_image(docker: &Docker) -> String {
    let base = test_image(docker).await;
    let tag = "muster-test-fake-claude:2";
    if !docker.has_image(tag).await.unwrap() {
        let encoded = base64(FAKE_CLAUDE.as_bytes());
        let recipe = format!(
            "FROM {base}\nRUN echo {encoded} | base64 -d > /usr/local/bin/claude && chmod +x /usr/local/bin/claude\n"
        );
        docker
            .build(tag, &recipe, None, Duration::from_secs(600))
            .await
            .unwrap();
    }
    tag.to_owned()
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for (i, shift) in [18, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(TABLE[((n >> shift) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[derive(Debug, Default)]
struct FakeHost {
    opened: Mutex<Vec<(String, String, String)>>,
    comments: Mutex<Vec<String>>,
    ready: Mutex<Vec<u64>>,
}

impl GitHost for FakeHost {
    fn open_draft<'a>(
        &'a self,
        _root: &'a Path,
        branch: &'a str,
        base: &'a str,
        title: &'a str,
        _body: &'a str,
    ) -> BoxFuture<'a, Result<PullRequest, GitHubError>> {
        self.opened
            .lock()
            .unwrap()
            .push((branch.to_owned(), base.to_owned(), title.to_owned()));
        Box::pin(async {
            Ok(PullRequest {
                number: 17,
                url: "https://github.com/o/app/pull/17".to_owned(),
            })
        })
    }

    fn mark_ready<'a>(
        &'a self,
        _root: &'a Path,
        number: u64,
    ) -> BoxFuture<'a, Result<(), GitHubError>> {
        self.ready.lock().unwrap().push(number);
        Box::pin(async { Ok(()) })
    }

    fn mark_draft<'a>(
        &'a self,
        _root: &'a Path,
        _number: u64,
    ) -> BoxFuture<'a, Result<(), GitHubError>> {
        Box::pin(async { Ok(()) })
    }

    fn review_comment<'a>(
        &'a self,
        _root: &'a Path,
        _number: u64,
        body: &'a str,
    ) -> BoxFuture<'a, Result<(), GitHubError>> {
        self.comments.lock().unwrap().push(body.to_owned());
        Box::pin(async { Ok(()) })
    }

    fn state<'a>(
        &'a self,
        _root: &'a Path,
        _number: u64,
    ) -> BoxFuture<'a, Result<PrState, GitHubError>> {
        Box::pin(async { Ok(PrState::Open) })
    }
}

#[derive(Debug)]
struct FakeCredentials;

impl Credentials for FakeCredentials {
    fn claude(&self) -> BoxFuture<'_, Option<Secret>> {
        Box::pin(async { Some(Secret::new("ANTHROPIC_API_KEY", "fake-key".to_owned())) })
    }
}

struct RemoveOnDrop(String);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", &self.0])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }
}

fn context(root: PathBuf) -> RunContext {
    RunContext {
        run: RunId::new(),
        project: muster_core::ProjectId::new(),
        root,
        ticket: Ticket {
            number: 42,
            title: "Fix the thing".to_owned(),
            url: "https://github.com/o/app/issues/42".to_owned(),
            body: "Make fix.txt say fixed.".to_owned(),
            labels: Vec::new(),
            author: None,
        },
        branch: "muster/issue-42-fix-the-thing".to_owned(),
        config: ProjectConfig {
            memory_bytes: 700 << 20,
            checks: muster_core::config::Checks {
                // Fails on the first attempt, passes once the agent fixed it.
                commands: vec!["grep -q fixed fix.txt".to_owned()],
                timeout: Duration::from_secs(60),
            },
            limits: RunLimits {
                attempts: 3,
                run_timeout: Duration::from_secs(600),
                max_cost_usd: None,
            },
            ..ProjectConfig::default()
        },
        agent_session: None,
        container: None,
        pr: None,
        cost_micros: 0,
    }
}

#[tokio::test]
async fn real_docker_takes_a_run_from_pickup_to_ready_through_a_failed_check() {
    if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
        return;
    }
    let docker = Docker::default();
    let image = fake_claude_image(&docker).await;
    let dir = tempfile::tempdir().unwrap();
    let remote = repository(dir.path(), "remote");
    let checkout = dir.path().join("checkout");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    let mut context = context(checkout);
    context.config.dockerfile = None;
    let _remove = RemoveOnDrop(provision::container_name(context.run));
    let host = Arc::new(FakeHost::default());
    let work = DockerWork::new(
        dir.path().join("data"),
        host.clone(),
        Arc::new(FakeCredentials),
        Duration::from_millis(100),
        Some(image),
    );

    let mut machine = RunMachine::new(Limits {
        attempts: 3,
        run_timeout: Duration::from_secs(600),
        max_cost_micros: None,
    });
    let (reports, mut queue) = mpsc::unbounded_channel();
    let mut effects = machine.step(RunEvent::SlotGranted, Instant::now());
    let deadline = Instant::now() + Duration::from_secs(240);
    while !matches!(machine.stage(), RunStage::ReadyForYou | RunStage::NeedsYou) {
        for effect in effects.drain(..) {
            if !matches!(effect, RunEffect::Transition { .. }) {
                context.cost_micros = machine.cost_micros();
                work.start(context.clone(), effect, reports.clone());
            }
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        let (_, report) = tokio::time::timeout(remaining, queue.recv())
            .await
            .unwrap_or_else(|_| panic!("stuck at {:?}", machine.stage()))
            .unwrap();
        match report {
            Report::Event(event) => effects = machine.step(event, Instant::now()),
            Report::Container(container) => context.container = Some(container),
            Report::AgentSession(session) => context.agent_session = Some(session),
            Report::PullRequest(pr) => context.pr = Some(pr),
        }
    }
    assert_eq!(
        machine.stage(),
        RunStage::ReadyForYou,
        "{:?}",
        machine.reason()
    );
    assert_eq!(machine.attempt(), 2, "the failed check sent it back once");
    assert!(
        machine.cost_micros() >= 600_000,
        "{}",
        machine.cost_micros()
    );

    // The branch reached the "remote", with the fix, pushed from the host.
    let log = std::process::Command::new("git")
        .args([
            "-C",
            remote.to_str().unwrap(),
            "log",
            "--format=%s",
            "muster/issue-42-fix-the-thing",
        ])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&log.stdout);
    assert_eq!(
        log.lines().filter(|line| *line == "Fix the thing").count(),
        2,
        "{log}"
    );
    let shown = std::process::Command::new("git")
        .args([
            "-C",
            remote.to_str().unwrap(),
            "show",
            "muster/issue-42-fix-the-thing:fix.txt",
        ])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&shown.stdout), "first\nfixed\n");

    // The pull request, the review comment, and ready.
    assert_eq!(
        host.opened.lock().unwrap().as_slice(),
        [(
            "muster/issue-42-fix-the-thing".to_owned(),
            "main".to_owned(),
            "Fix the thing (#42)".to_owned()
        )]
    );
    assert!(host.comments.lock().unwrap()[0].contains("approves"));
    for effect in effects.drain(..) {
        if !matches!(effect, RunEffect::Transition { .. }) {
            work.start(context.clone(), effect, reports.clone());
        }
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(host.ready.lock().unwrap().as_slice(), [17]);

    // The credential reached only the agents' runs, never the container.
    let container = context.container.clone().unwrap();
    let inspect = std::process::Command::new("docker")
        .args(["inspect", "--format", "{{json .Config.Env}}", &container])
        .output()
        .unwrap();
    assert!(!String::from_utf8_lossy(&inspect.stdout).contains("fake-key"));
}
