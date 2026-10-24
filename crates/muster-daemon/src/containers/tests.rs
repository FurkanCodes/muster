use std::path::Path;
use std::process::Stdio;

use super::*;

fn spec() -> ContainerSpec {
    ContainerSpec {
        name: "muster-run-1".to_owned(),
        image: "muster-base:1".to_owned(),
        labels: vec![(LABEL_RUN.to_owned(), "run_1".to_owned())],
        memory_bytes: 6 << 30,
        millicpus: 2_500,
        mounts: vec![
            Mount {
                source: MountSource::Host(PathBuf::from("/data/mirrors/app.git")),
                target: "/mirror".to_owned(),
                read_only: true,
            },
            Mount {
                source: MountSource::Volume("muster-cache-app".to_owned()),
                target: "/cache".to_owned(),
                read_only: false,
            },
        ],
    }
}

#[test]
fn a_container_is_started_idle_limited_and_labelled() {
    let args = run_args(&spec());
    let joined = args.join(" ");
    assert!(
        joined.starts_with("run --detach --init --name muster-run-1"),
        "{joined}"
    );
    assert!(joined.contains("--label muster.run=run_1"), "{joined}");
    assert!(joined.contains(&format!(
        "--memory {} --memory-swap {}",
        6u64 << 30,
        6u64 << 30
    )));
    assert!(joined.contains("--cpus 2.500"), "{joined}");
    assert!(
        joined.contains("--mount type=bind,source=/data/mirrors/app.git,target=/mirror,readonly")
    );
    assert!(joined.contains("--mount type=volume,source=muster-cache-app,target=/cache "));
    assert!(
        joined.ends_with("--entrypoint tail muster-base:1 -f /dev/null"),
        "{joined}"
    );
}

#[test]
fn listed_containers_are_read() {
    let output = br#"{"Command":"\"tail -f /dev/null\"","ID":"abc123","Names":"muster-run-1","State":"running","Status":"Up 2 minutes"}
{"ID":"def456","Names":"muster-run-2","State":"exited","Status":"Exited (0)"}
"#;
    assert_eq!(
        parse_ps(output).unwrap(),
        [
            Found {
                id: "abc123".to_owned(),
                name: "muster-run-1".to_owned(),
                running: true,
            },
            Found {
                id: "def456".to_owned(),
                name: "muster-run-2".to_owned(),
                running: false,
            },
        ]
    );
    assert!(parse_ps(b"").unwrap().is_empty());
    assert!(matches!(
        parse_ps(b"nonsense"),
        Err(RuntimeError::Unreadable(_))
    ));
}

#[test]
fn a_stopped_runtime_is_told_apart() {
    assert!(is_not_running(
        "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
    ));
    assert!(!is_not_running("Error: No such container: x"));
}

/// A stand-in `docker` that records its arguments and the `SECRET` it
/// was given, then answers as `docker exec` would.
fn recording_docker(dir: &Path) -> Docker {
    let script = dir.join("docker");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done > {args}\nprintf '%s' \"$SECRET\" > {secret}\necho out\necho err >&2\nexit 3\n",
            args = dir.join("args").display(),
            secret = dir.join("secret").display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    Docker {
        program: script.display().to_string(),
    }
}

#[tokio::test]
async fn secrets_reach_the_container_through_the_environment_not_the_arguments() {
    let dir = tempfile::tempdir().unwrap();
    let docker = recording_docker(dir.path());
    let output = docker
        .exec(
            "abc123",
            &["claude", "-p"],
            &[("SECRET", "s3cr3t-value")],
            Duration::from_secs(10),
        )
        .await
        .unwrap();
    assert_eq!(output.code, Some(3));
    assert!(!output.success());
    assert_eq!(output.stdout, b"out\n");
    assert_eq!(output.stderr, "err");
    let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
    assert_eq!(args, "exec\n--env\nSECRET\nabc123\nclaude\n-p\n");
    assert!(!args.contains("s3cr3t"));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("secret")).unwrap(),
        "s3cr3t-value"
    );
}

#[tokio::test]
async fn a_missing_docker_is_said_plainly() {
    let docker = Docker {
        program: "/nonexistent/docker".to_owned(),
    };
    assert!(matches!(
        docker.capacity().await,
        Err(RuntimeError::Missing)
    ));
}

// Against the real Docker, when asked: MUSTER_DOCKER_TESTS=1, with an image
// that has a shell in MUSTER_TEST_IMAGE (default alpine:3).

fn real() -> Option<(Docker, String)> {
    (std::env::var("MUSTER_DOCKER_TESTS").as_deref() == Ok("1")).then(|| {
        (
            Docker::default(),
            std::env::var("MUSTER_TEST_IMAGE").unwrap_or_else(|_| "alpine:3".to_owned()),
        )
    })
}

#[tokio::test]
async fn real_docker_runs_a_limited_labelled_container_and_removes_it() {
    let Some((docker, image)) = real() else {
        return;
    };
    let capacity = docker.capacity().await.unwrap();
    assert!(capacity.memory_bytes > 0 && capacity.cpus > 0);
    let unique = format!("test-{}", std::process::id());
    let spec = ContainerSpec {
        name: format!("muster-{unique}"),
        image,
        labels: vec![(LABEL_RUN.to_owned(), unique.clone())],
        memory_bytes: 600 << 20,
        millicpus: 500,
        mounts: Vec::new(),
    };
    let id = docker.start(&spec).await.unwrap();
    // Removed however the test ends, so a failure leaves nothing running.
    let _remove = RemoveOnDrop(spec.name.clone());
    {
        let found = docker.find(LABEL_RUN, &unique).await.unwrap();
        assert_eq!(found.len(), 1);
        assert!(found[0].running);
        assert!(id.starts_with(&found[0].id) || found[0].id.starts_with(&id));

        let output = docker
            .exec(
                &id,
                &[
                    "sh",
                    "-c",
                    "echo \"$SECRET\"; cat /sys/fs/cgroup/memory.max",
                ],
                &[("SECRET", "shh")],
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        assert!(output.success(), "{output:?}");
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.starts_with("shh\n"), "{text}");
        assert!(text.contains(&(600u64 << 20).to_string()), "{text}");

        // A second container of the same name is refused.
        assert!(docker.start(&spec).await.is_err());
    }
    docker.remove(&id).await.unwrap();
    docker.remove(&id).await.unwrap();
    assert!(docker.find(LABEL_RUN, &unique).await.unwrap().is_empty());
}

struct RemoveOnDrop(String);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::process::Command::new("docker")
            .args(["rm", "--force", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
