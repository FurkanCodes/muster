//! End-to-end tests for workspaces' timelines (`PRODUCT.md` §6,
//! `PRD-14`): the real daemon on a real repository keeps what an agent's
//! turn changed, refuses to put files back while the agent works, puts
//! them back when asked, and can undo that too, writing nothing of its
//! own into the repository.

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

use std::path::{Path, PathBuf};
use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::timeline::EntryKind;
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use serde_json::json;

use support::{ClaudeLaunch, Sandbox, expect_event, git};

/// A project with a worktree on `work`, and a stand-in Claude Code there,
/// started with `prompt`.
struct Agent {
    root: PathBuf,
    workspace: WorkspaceId,
    path: PathBuf,
    session: SessionId,
    launch: ClaudeLaunch,
}

impl Agent {
    async fn start(sandbox: &Sandbox, connection: &mut Connection, prompt: Option<&str>) -> Self {
        let root = sandbox.repository("app");
        let Reply::ProjectAdded { id: project } = connection
            .command(Command::AddProject {
                root: root.clone(),
                name: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected ProjectAdded");
        };
        let Reply::WorktreeCreated { id: workspace } = connection
            .command(Command::CreateWorktree {
                project,
                branch: Some("work".to_owned()),
                on: None,
                named_for: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected WorktreeCreated");
        };
        let snapshot = connection.subscribe().await.unwrap();
        let path = snapshot
            .workspaces
            .iter()
            .find(|found| found.id == workspace)
            .and_then(|found| found.kind.host_path())
            .unwrap()
            .to_owned();
        let Reply::SessionStarted { id: session } = connection
            .command(Command::StartSession {
                workspace,
                harness: "claude".to_owned(),
                title: None,
                size: TermSize::default(),
                prompt: prompt.map(str::to_owned),
                started_by: None,
            })
            .await
            .unwrap()
        else {
            panic!("expected SessionStarted");
        };
        let launch = sandbox.wait_for_launches(session, 1).remove(0);
        Self {
            root,
            workspace,
            path,
            session,
            launch,
        }
    }

    /// Runs a hook as Claude Code would.
    fn fire(&self, event: &str) {
        let id = self.launch.value("--session-id").unwrap();
        self.launch.fire(
            event,
            &json!({ "session_id": id, "hook_event_name": event, "cwd": "/work" }),
        );
    }
}

/// Waits for `session` to come to a status `wanted` says yes to.
async fn status(
    connection: &mut Connection,
    session: SessionId,
    wanted: impl Fn(SessionStatus) -> bool,
) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            ..
        }) if *changed == session && wanted(*status) => Some(()),
        _ => None,
    })
    .await;
}

/// Waits for the newest entry of `workspace`'s timeline to be told.
async fn newest(connection: &mut Connection, workspace: WorkspaceId) -> u64 {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(shown)) if shown.id == workspace => {
            shown.timeline
        }
        _ => None,
    })
    .await
}

async fn undo(connection: &mut Connection, workspace: WorkspaceId, number: u64) -> Reply {
    connection
        .command(Command::Undo {
            workspace,
            number,
            move_branch: false,
        })
        .await
        .unwrap()
}

fn timeline_dir(sandbox: &Sandbox, workspace: WorkspaceId) -> PathBuf {
    sandbox
        .paths()
        .data_dir
        .join("timelines")
        .join(workspace.to_string())
}

/// Waits until a turn's start is kept, so the agent's edits come after.
fn wait_for_start(sandbox: &Sandbox, workspace: WorkspaceId) {
    let open = timeline_dir(sandbox, workspace).join("refs/open");
    let kept = (0..200).any(|_| {
        let found = std::fs::read_dir(&open).is_ok_and(|mut entries| entries.next().is_some());
        if !found {
            std::thread::sleep(Duration::from_millis(25));
        }
        found
    });
    assert!(kept, "the turn's start was never kept");
}

fn read(path: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(path.join(file)).ok()
}

#[tokio::test]
async fn a_turn_is_kept_and_its_files_put_back_only_while_no_agent_works() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let agent = Agent::start(&sandbox, &mut connection, None).await;
    let (workspace, path) = (agent.workspace, agent.path.clone());
    let refs_before = git(&agent.root, &["for-each-ref"]);

    // A turn: the agent changes a file and writes another.
    agent.fire("UserPromptSubmit");
    wait_for_start(&sandbox, workspace);
    std::fs::write(path.join("README.md"), "test\nby the agent\n").unwrap();
    std::fs::write(path.join("app.rs"), "fn main() {}\n").unwrap();
    agent.fire("Stop");
    assert_eq!(newest(&mut connection, workspace).await, 1);

    let Reply::Timeline(timeline) = connection
        .command(Command::Timeline { workspace })
        .await
        .unwrap()
    else {
        panic!("expected the timeline");
    };
    let [turn] = timeline.entries.as_slice() else {
        panic!("{timeline:?}");
    };
    assert_eq!(
        turn.kind,
        EntryKind::Turn {
            session: agent.session,
            agent: "Claude Code".to_owned()
        }
    );
    assert_eq!(turn.summary.map(|summary| summary.files), Some(2));
    let Reply::FileDiff(diff) = connection
        .command(Command::EntryDiff {
            workspace,
            number: 1,
        })
        .await
        .unwrap()
    else {
        panic!("expected the turn's diff");
    };
    assert!(diff.text.contains("+by the agent"), "{}", diff.text);
    assert!(diff.text.contains("+fn main() {}"), "{}", diff.text);

    // While the agent works, nothing is put back (PRD-14).
    agent.fire("UserPromptSubmit");
    status(&mut connection, agent.session, |now| {
        now == SessionStatus::Running
    })
    .await;
    let refused = connection
        .command(Command::Undo {
            workspace,
            number: 1,
            move_branch: false,
        })
        .await
        .unwrap_err();
    let ClientError::Daemon(refused) = refused else {
        panic!("{refused}");
    };
    assert_eq!(refused.code, ErrorCode::Conflict, "{}", refused.message);
    assert!(path.join("app.rs").exists());
    agent.fire("Stop");
    status(&mut connection, agent.session, |now| {
        now == SessionStatus::Finished
    })
    .await;

    let Reply::UndoPlan(plan) = connection
        .command(Command::UndoPlan {
            workspace,
            number: 1,
        })
        .await
        .unwrap()
    else {
        panic!("expected the plan");
    };
    assert_eq!((plan.later, plan.branch), (0, None));
    let Reply::Undone {
        entry,
        moved_branch,
    } = undo(&mut connection, workspace, 1).await
    else {
        panic!("expected the files back");
    };
    assert_eq!((entry, moved_branch), (2, false));
    assert_eq!(read(&path, "README.md").as_deref(), Some("test\n"));
    assert_eq!(read(&path, "app.rs"), None);

    // And the undo undone.
    undo(&mut connection, workspace, 2).await;
    assert_eq!(
        read(&path, "README.md").as_deref(),
        Some("test\nby the agent\n")
    );
    assert!(path.join("app.rs").exists());

    // The person's repository has nothing of Muster's in it, and the log
    // none of their code.
    assert_eq!(git(&agent.root, &["for-each-ref"]), refs_before);
    let log = daemon.log();
    assert!(
        !log.contains("by the agent") && !log.contains("fn main"),
        "{log}"
    );
}

#[tokio::test]
async fn a_session_s_first_prompt_begins_a_turn_and_the_timeline_goes_with_its_worktree() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let agent = Agent::start(&sandbox, &mut connection, Some("write the app")).await;
    let workspace = agent.workspace;
    wait_for_start(&sandbox, workspace);
    std::fs::write(agent.path.join("app.rs"), "fn main() {}\n").unwrap();
    agent.fire("Stop");
    assert_eq!(newest(&mut connection, workspace).await, 1);

    connection
        .command(Command::StopSession {
            session: agent.session,
        })
        .await
        .unwrap();
    status(&mut connection, agent.session, SessionStatus::is_exited).await;
    connection
        .command(Command::RemoveWorktree {
            workspace,
            force: true,
        })
        .await
        .unwrap();
    let store = timeline_dir(&sandbox, workspace);
    let gone = (0..100).any(|_| {
        std::thread::sleep(Duration::from_millis(50));
        !store.exists()
    });
    assert!(gone, "{} is still there", store.display());
}
