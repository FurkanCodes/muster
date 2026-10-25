//! End-to-end tests for worktrees (M1 step 6), against the real binary and
//! real git repositories.

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

use std::path::PathBuf;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame};
use muster_core::{ProjectId, WorkspaceId};

use support::{Sandbox, expect_event, git};

async fn add_project(connection: &mut Connection, root: &std::path::Path) -> ProjectId {
    match connection
        .command(Command::AddProject {
            root: root.to_owned(),
            name: None,
            space: None,
        })
        .await
        .unwrap()
    {
        Reply::ProjectAdded { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

async fn create(
    connection: &mut Connection,
    project: ProjectId,
    branch: Option<&str>,
) -> Result<WorkspaceId, ClientError> {
    connection
        .command(Command::CreateWorktree {
            project,
            branch: branch.map(str::to_owned),
            on: None,
            named_for: None,
        })
        .await
        .map(|reply| match reply {
            Reply::WorktreeCreated { id } => id,
            other => panic!("unexpected {other:?}"),
        })
}

/// The path and branch of worktree `id`, from a fresh snapshot.
async fn worktree(connection: &mut Connection, id: WorkspaceId) -> (PathBuf, String) {
    let snapshot = connection.subscribe().await.unwrap();
    let workspace = snapshot
        .workspaces
        .into_iter()
        .find(|workspace| workspace.id == id)
        .expect("the worktree is in the snapshot");
    assert!(!workspace.kind.is_checkout());
    (
        workspace.kind.host_path().unwrap().to_owned(),
        workspace.kind.branch().to_owned(),
    )
}

fn code(result: Result<impl std::fmt::Debug, ClientError>) -> ErrorCode {
    match result {
        Err(ClientError::Daemon(error)) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn a_worktree_is_created_on_a_new_branch_and_announced() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut watcher = daemon.connect().await;
    watcher.subscribe().await.unwrap();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, Some("feature/login"))
        .await
        .unwrap();
    // The project's own checkout is announced with it; then the worktree.
    let announced = expect_event(&mut watcher, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceUpserted(workspace))
            if !workspace.kind.is_checkout() =>
        {
            Some(workspace.clone())
        }
        _ => None,
    })
    .await;
    assert_eq!(announced.id, id);

    let (path, branch) = worktree(&mut connection, id).await;
    assert_eq!(branch, "feature/login");
    let worktrees_dir = sandbox.paths().worktrees_dir.canonicalize().unwrap();
    assert_eq!(path, worktrees_dir.join("app/feature-login"));
    assert!(path.join("README.md").exists());
    assert!(git(&root, &["worktree", "list"]).contains(path.to_str().unwrap()));
    assert_eq!(git(&path, &["branch", "--show-current"]), "feature/login");
}

#[tokio::test]
async fn without_a_branch_a_namespaced_one_is_made() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, None).await.unwrap();
    let (_, branch) = worktree(&mut connection, id).await;
    assert!(branch.starts_with("muster/"), "{branch}");
}

#[tokio::test]
async fn an_existing_branch_is_checked_out_not_recreated() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    git(&root, &["branch", "existing"]);
    let tip = git(&root, &["rev-parse", "existing"]);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, Some("existing"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), tip);
}

#[tokio::test]
async fn a_new_branch_starts_from_the_base_not_from_the_branch_checked_out() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let main = git(&root, &["rev-parse", "main"]);
    // The person works on a branch of their own in the checkout.
    git(&root, &["switch", "--quiet", "-c", "feature/mine"]);
    std::fs::write(root.join("mine.txt"), "mine\n").unwrap();
    git(&root, &["add", "mine.txt"]);
    git(&root, &["commit", "--quiet", "--message", "mine"]);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let id = create(&mut connection, project, Some("fix/login"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;
    assert_eq!(git(&path, &["rev-parse", "HEAD"]), main);
    assert!(!path.join("mine.txt").exists());
}

#[tokio::test]
async fn two_projects_of_one_name_keep_their_worktrees_and_builds_apart() {
    let sandbox = Sandbox::new();
    let first = sandbox.repository("app");
    // Another repository called `app`, elsewhere.
    let second = sandbox.root().join("elsewhere/app");
    std::fs::create_dir_all(&second).unwrap();
    git(&second, &["init", "--quiet", "--initial-branch", "main"]);
    std::fs::write(second.join("README.md"), "other\n").unwrap();
    git(&second, &["add", "README.md"]);
    git(&second, &["commit", "--quiet", "--message", "first"]);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let one = add_project(&mut connection, &first).await;
    let other = add_project(&mut connection, &second.canonicalize().unwrap()).await;

    let id = create(&mut connection, one, Some("a")).await.unwrap();
    let worktrees_dir = sandbox.paths().worktrees_dir.canonicalize().unwrap();
    assert_eq!(
        worktree(&mut connection, id).await.0,
        worktrees_dir.join("app/a")
    );
    let id = create(&mut connection, other, Some("b")).await.unwrap();
    let (path, _) = worktree(&mut connection, id).await;
    let group = path.parent().unwrap();
    assert_ne!(group, worktrees_dir.join("app"));
    assert!(
        group
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("app-"),
        "{}",
        path.display()
    );
    // Each keeps its own folder for the next.
    let id = create(&mut connection, other, Some("c")).await.unwrap();
    assert_eq!(worktree(&mut connection, id).await.0, group.join("c"));
    let id = create(&mut connection, one, Some("d")).await.unwrap();
    assert_eq!(
        worktree(&mut connection, id).await.0,
        worktrees_dir.join("app/d")
    );
}

/// A stand-in for git, first on the daemon's `PATH`, that holds `git
/// worktree add` and `git worktree remove` until the test opens their gate
/// (`<gates>/<add|remove>/open`), saying it has started first; anything
/// else goes straight to git.
fn gated_git(sandbox: &Sandbox) -> PathBuf {
    let gates = sandbox.root().join("gates");
    for gate in ["add", "remove"] {
        std::fs::create_dir_all(gates.join(gate)).unwrap();
    }
    let real = std::process::Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap()
        .stdout;
    let real = String::from_utf8(real).unwrap().trim().to_owned();
    let script = format!(
        "#!/bin/sh\n\
         if [ \"$1\" = worktree ] && {{ [ \"$2\" = add ] || [ \"$2\" = remove ]; }}; then\n\
         \x20 touch '{gates}/'\"$2\"/started\n\
         \x20 while [ ! -e '{gates}/'\"$2\"/open ]; do sleep 0.05; done\n\
         fi\n\
         exec '{real}' \"$@\"\n",
        gates = gates.display(),
    );
    let path = sandbox.root().join("bin").join("git");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    gates
}

/// Waits until git has started the work behind `gate`.
async fn started(gates: &std::path::Path, gate: &str) {
    let flag = gates.join(gate).join("started");
    until(&format!("git worktree {gate}"), || flag.exists()).await;
}

/// `future`, which must not wait on git's work.
async fn at_once<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(5), future)
        .await
        .expect("answered while git works")
}

#[tokio::test]
async fn the_daemon_answers_while_git_makes_or_removes_a_worktree() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let gates = gated_git(&sandbox);
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    let mut making = daemon.connect().await;
    let made = tokio::spawn(async move { create(&mut making, project, Some("slow")).await });
    started(&gates, "add").await;
    at_once(connection.command(Command::Ping)).await.unwrap();
    // The branch is taken while it is made.
    assert_eq!(
        code(at_once(create(&mut connection, project, Some("slow"))).await),
        ErrorCode::Conflict
    );
    std::fs::write(gates.join("add/open"), "").unwrap();
    let id = made.await.unwrap().unwrap();
    let (path, branch) = worktree(&mut connection, id).await;
    assert_eq!(branch, "slow");
    assert!(path.join("README.md").exists());

    let mut removing = daemon.connect().await;
    let removed = tokio::spawn(async move {
        removing
            .command(Command::RemoveWorktree {
                workspace: id,
                force: false,
            })
            .await
    });
    started(&gates, "remove").await;
    at_once(connection.command(Command::Ping)).await.unwrap();
    // Nothing starts in a worktree going, and it goes once.
    let shell = Command::StartSession {
        workspace: Some(id),
        harness: "shell".to_owned(),
        title: None,
        size: muster_core::protocol::TermSize::default(),
        prompt: None,
        started_by: None,
        space: None,
    };
    assert_eq!(
        code(at_once(connection.command(shell)).await),
        ErrorCode::Conflict
    );
    let again = Command::RemoveWorktree {
        workspace: id,
        force: false,
    };
    assert_eq!(
        code(at_once(connection.command(again)).await),
        ErrorCode::Conflict
    );
    std::fs::write(gates.join("remove/open"), "").unwrap();
    assert_eq!(removed.await.unwrap().unwrap(), Reply::Done);
    assert!(!path.exists());
    let left = connection.subscribe().await.unwrap().workspaces;
    assert!(left.iter().all(|workspace| workspace.kind.is_checkout()));
}

#[tokio::test]
async fn bad_requests_are_refused_with_their_code() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;

    assert_eq!(
        code(create(&mut connection, project, Some("bad..name")).await),
        ErrorCode::BadRequest
    );
    assert_eq!(
        code(create(&mut connection, ProjectId::new(), None).await),
        ErrorCode::NotFound
    );
    // The main checkout already has `main`.
    assert_eq!(
        code(create(&mut connection, project, Some("main")).await),
        ErrorCode::Conflict
    );

    create(&mut connection, project, Some("twice"))
        .await
        .unwrap();
    assert_eq!(
        code(create(&mut connection, project, Some("twice")).await),
        ErrorCode::Conflict
    );

    // A branch name that would also be another branch's folder, both ways.
    git(&root, &["branch", "nest/inner"]);
    let clash = create(&mut connection, project, Some("nest"))
        .await
        .unwrap_err();
    let ClientError::Daemon(clash) = clash else {
        panic!("expected a refusal");
    };
    assert_eq!(clash.code, ErrorCode::Conflict);
    assert!(
        clash.message.contains("the branch nest/inner exists"),
        "{}",
        clash.message
    );
    let clash = create(&mut connection, project, Some("main/x"))
        .await
        .unwrap_err();
    let ClientError::Daemon(clash) = clash else {
        panic!("expected a refusal");
    };
    assert!(
        clash.message.contains("the branch main exists"),
        "{}",
        clash.message
    );

    // Refusals are logged with their reason, for reading afterwards.
    let log = daemon.log();
    assert!(log.contains("command refused"), "{log}");
    assert!(log.contains("not a valid branch name"), "{log}");
}

#[tokio::test]
async fn removing_keeps_the_branch_and_protects_uncommitted_work() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut watcher = daemon.connect().await;
    watcher.subscribe().await.unwrap();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("work"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;

    std::fs::write(path.join("README.md"), "changed\n").unwrap();
    let remove = |force| Command::RemoveWorktree {
        workspace: id,
        force,
    };
    assert_eq!(
        code(connection.command(remove(false)).await),
        ErrorCode::Conflict
    );
    assert!(path.exists());

    assert_eq!(connection.command(remove(true)).await.unwrap(), Reply::Done);
    expect_event(&mut watcher, |frame| match frame {
        ServerFrame::Delta(Delta::WorkspaceRemoved { id: removed }) if *removed == id => Some(()),
        _ => None,
    })
    .await;
    assert!(!path.exists());
    assert_eq!(git(&root, &["branch", "--list", "work"]), "work");
    let left = connection.subscribe().await.unwrap().workspaces;
    assert!(left.iter().all(|workspace| workspace.kind.is_checkout()));
    assert_eq!(
        code(connection.command(remove(false)).await),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_worktree_deleted_by_hand_can_still_be_removed() {
    // Removed before the worktree sync would notice and forget it.
    let sandbox = Sandbox::new().with_env("MUSTER_WORKTREE_MS", "3600000");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("gone"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;

    std::fs::remove_dir_all(&path).unwrap();
    let removed = connection
        .command(Command::RemoveWorktree {
            workspace: id,
            force: false,
        })
        .await;
    assert_eq!(removed.unwrap(), Reply::Done);
    assert!(!git(&root, &["worktree", "list"]).contains("gone"));
}

#[tokio::test]
async fn worktrees_survive_a_daemon_restart() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("kept"))
        .await
        .unwrap();
    drop(connection);
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    let (_, branch) = worktree(&mut daemon.connect().await, id).await;
    assert_eq!(branch, "kept");
}

#[test]
fn the_command_line_manages_worktrees_and_sessions() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let _daemon = sandbox.start();
    let run = |args: &[&str]| {
        let output = sandbox.run(args);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        (output.status.success(), stdout, stderr)
    };

    assert!(run(&["project", "add", root.to_str().unwrap()]).0);
    let (ok, stdout, stderr) = run(&["workspace", "add", "-p", "app", "--branch", "cli-work"]);
    assert!(ok, "{stderr}");
    assert!(
        stdout.contains("Made workspace cli-work on cli-work at"),
        "{stdout}"
    );

    let (ok, stdout, stderr) = run(&["term", "new", "--in", "cli-work", "--bg"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("terminal 1 in cli-work"), "{stdout}");

    let (_, status, _) = run(&["status"]);
    assert!(
        status.contains("1 project, 1 worktree, 1 session"),
        "{status}"
    );
    assert!(status.contains("cli-work"), "{status}");
    // A shell is named by what runs in it once the sampler looks.
    assert!(status.contains("fresh"), "{status}");

    // Its sessions are stopped only on the person's word: asked, or with
    // --yes; a script that says neither removes nothing.
    let (ok, _, stderr) = run(&["claude", "--in", "cli-work"]);
    assert!(ok, "{stderr}");
    let (ok, _, stderr) = run(&["workspace", "rm", "cli-work"]);
    assert!(!ok);
    assert!(stderr.contains("nothing was removed"), "{stderr}");
    let (ok, stdout, stderr) = run(&["workspace", "rm", "cli-work", "-y"]);
    assert!(ok, "{stderr}");
    assert!(stdout.contains("its branch is kept"), "{stdout}");
    assert!(run(&["status"]).1.contains("0 worktrees"));
}

/// Waits until `done` holds, for what the daemon finishes off its loop.
async fn until(what: &str, done: impl Fn() -> bool) {
    wait::until_async(what, async || done()).await;
}

#[tokio::test]
async fn a_project_s_worktrees_build_into_one_folder_beside_them_until_turned_off() {
    use muster_core::settings::PreferenceChange;

    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let project = add_project(&mut connection, &root).await;
    let id = create(&mut connection, project, Some("feature"))
        .await
        .unwrap();
    let (path, _) = worktree(&mut connection, id).await;
    let group = path.parent().unwrap().to_owned();
    let config = group.join(".cargo/config.toml");
    let builds = group.join(".target");

    // Cargo builds the worktree into the folder beside it; the repository
    // and its own checkout are not touched.
    assert!(
        std::fs::read_to_string(&config)
            .unwrap()
            .starts_with("# Written by Muster")
    );
    assert!(!root.join(".cargo").exists() && !path.join(".cargo").exists());
    std::fs::write(
        path.join("Cargo.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::create_dir_all(path.join("src")).unwrap();
    std::fs::write(path.join("src/main.rs"), "fn main() {}\n").unwrap();
    let metadata = std::process::Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(&path)
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .unwrap();
    assert!(metadata.status.success(), "{metadata:?}");
    let told: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let built = std::fs::canonicalize(&group).unwrap().join(".target");
    assert_eq!(told["target_directory"], built.to_string_lossy().as_ref());

    // Turned off, each worktree builds into its own again, and the shared
    // builds go; turned on, the config is back.
    std::fs::create_dir_all(builds.join("debug")).unwrap();
    let off = PreferenceChange::SharedBuilds(false);
    connection
        .command(Command::ChangePreference(off))
        .await
        .unwrap();
    assert!(!config.exists());
    until("the shared builds to go", || {
        std::fs::read_dir(&group).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".target")
        })
    })
    .await;
    let on = PreferenceChange::SharedBuilds(true);
    connection
        .command(Command::ChangePreference(on))
        .await
        .unwrap();
    assert!(config.exists());

    // The project's last worktree there takes them along.
    std::fs::remove_file(path.join("Cargo.toml")).unwrap();
    std::fs::remove_dir_all(path.join("src")).unwrap();
    connection
        .command(Command::RemoveWorktree {
            workspace: id,
            force: true,
        })
        .await
        .unwrap();
    assert!(!config.exists() && !group.join(".cargo").exists());
}
