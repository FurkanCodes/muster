use std::path::PathBuf;

use muster_core::config::ProjectConfig;
use muster_core::{ProjectId, Ticket};

use super::*;
use crate::git::tests::{git, repository};
use crate::runs::mirror;

fn context(root: PathBuf) -> RunContext {
    RunContext {
        run: RunId::new(),
        project: ProjectId::new(),
        root,
        ticket: Ticket {
            number: 42,
            title: "Fix it".to_owned(),
            url: "https://github.com/o/app/issues/42".to_owned(),
            body: String::new(),
            labels: Vec::new(),
            author: None,
        },
        branch: "muster/issue-42-fix-it".to_owned(),
        config: ProjectConfig {
            memory_bytes: 700 << 20,
            ..ProjectConfig::default()
        },
        from_branch: false,
        agent_session: None,
        container: None,
        pr: None,
        cost_micros: 0,
        current: tokio::sync::watch::channel(0).1,
    }
}

#[test]
fn a_run_s_container_is_limited_labelled_and_reads_the_mirror_only() {
    let context = context(PathBuf::from("/src/app"));
    let mirror = Mirror {
        path: PathBuf::from("/data/mirrors/p.git"),
    };
    let spec = spec(&context, &mirror, "muster-base:1");
    assert_eq!(spec.name, container_name(context.run));
    assert!(spec.name.starts_with("muster-run-") && spec.name == spec.name.to_lowercase());
    assert_eq!(spec.memory_bytes, 700 << 20);
    assert!(
        spec.labels
            .contains(&(LABEL_RUN.to_owned(), context.run.to_string()))
    );
    let mirror_mount = &spec.mounts[0];
    assert_eq!(mirror_mount.target, "/mirror");
    assert!(
        mirror_mount.read_only,
        "a container never writes to the host"
    );
    assert!(
        spec.mounts
            .iter()
            .skip(1)
            .all(|mount| matches!(mount.source, MountSource::Volume(_)))
    );
}

#[tokio::test]
async fn a_run_on_the_base_branch_itself_is_refused_before_any_container() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = repository(dir.path(), "app");
    let project = ProjectId::new();
    let mirror = mirror::refresh(&dir.path().join("data"), project, &checkout)
        .await
        .unwrap();
    let mut context = context(checkout);
    context.project = project;
    context.branch = "main".to_owned();
    // No Docker is reached: the stand-in program does not exist.
    let docker = Docker::with_program("muster-test-no-such-docker");
    let identity = Identity {
        name: "Test Person".to_owned(),
        email: "test@example.com".to_owned(),
    };
    let error = provision(&docker, &context, &mirror, "image", &identity)
        .await
        .unwrap_err();
    assert!(
        error.contains("main is the branch runs start from"),
        "{error}"
    );
}

#[test]
fn messages_keep_their_last_lines() {
    assert_eq!(last_lines("a\n\nb\nc\n", 2), "b · c");
    assert_eq!(last_lines("", 3), "");
}

// Against the real Docker, when asked: MUSTER_DOCKER_TESTS=1. Builds a
// small image with git from alpine:3.

const TEST_IMAGE: &str = "muster-test-git:1";

pub(crate) async fn test_image(docker: &Docker) -> String {
    if !docker.has_image(TEST_IMAGE).await.unwrap() {
        docker
            .build(
                TEST_IMAGE,
                "FROM alpine:3\nRUN apk add --no-cache git\n",
                None,
                Duration::from_secs(600),
            )
            .await
            .unwrap();
    }
    TEST_IMAGE.to_owned()
}

#[tokio::test]
async fn real_docker_gives_a_run_its_clone_on_its_branch_and_keeps_its_work() {
    if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
        return;
    }
    let docker = Docker::default();
    let image = test_image(&docker).await;
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
    let context = context(checkout.clone());
    let mirror = mirror::refresh(&dir.path().join("data"), context.project, &checkout)
        .await
        .unwrap();
    let _remove = RemoveOnDrop::for_run(&context);
    let identity = Identity {
        name: "Test Person".to_owned(),
        email: "test@example.com".to_owned(),
    };

    let first = provision(&docker, &context, &mirror, &image, &identity)
        .await
        .unwrap();
    assert_eq!(first.base, "main");
    let run = |script: &'static str| {
        let docker = docker.clone();
        let container = first.container.clone();
        async move {
            let output = docker
                .exec(
                    &container,
                    &["sh", "-c", script],
                    &[],
                    Duration::from_secs(60),
                )
                .await
                .unwrap();
            assert!(output.success(), "{script}: {}", output.stderr);
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
    };
    assert_eq!(
        run("git -C /work symbolic-ref --short HEAD").await,
        "muster/issue-42-fix-it"
    );
    assert_eq!(run("git -C /work config user.name").await, "Test Person");
    run("cd /work && echo fixed > fix.txt && git add fix.txt && git commit -q -m Fix").await;
    // The mirror cannot be written from inside.
    let output = docker
        .exec(
            &first.container,
            &["sh", "-c", "touch /mirror/x"],
            &[],
            Duration::from_secs(30),
        )
        .await
        .unwrap();
    assert!(!output.success());

    // Again, as after a restart: the same container, the work kept.
    let again = provision(&docker, &context, &mirror, &image, &identity)
        .await
        .unwrap();
    assert_eq!(again, first);
    assert_eq!(run("git -C /work log -1 --format=%s").await, "Fix");
}

/// Removes a run's container and its project's cache volume when dropped.
pub(crate) struct RemoveOnDrop {
    container: String,
    volume: String,
}

impl RemoveOnDrop {
    pub(crate) fn for_run(context: &RunContext) -> Self {
        Self {
            container: container_name(context.run),
            volume: format!("muster-cache-{}", context.project),
        }
    }
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        for args in [
            ["rm", "--force", &self.container],
            ["volume", "rm", &self.volume],
        ] {
            let _ = std::process::Command::new("docker")
                .args(args)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }
}
