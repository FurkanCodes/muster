//! Work that runs in a run's container on its own, outliving the docker
//! command that started it and the daemon (`ARC-04`): a headless agent's
//! turn, or the checks. Each job has a name and its files under the state
//! directory: what it was given (`.in`), its process id (`.pid`), what it
//! wrote (`.out`), its errors (`.err`) and its exit code (`.exit`, written
//! last). It runs in a session of its own, so stopping it reaches whatever
//! it started. The daemon reads the output back from an offset, again from
//! the start after a restart, and knows a job is over when its exit code
//! is there.

use std::time::Duration;

use super::provision::STATE_DIR;
use crate::containers::{Docker, RuntimeError};

/// How long a quick look into a container may take.
const LOOK: Duration = Duration::from_secs(60);

/// A job's files, by name: `/tmp/muster/out/<name>.*`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Job {
    name: String,
}

/// What a look at a running job found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Progress {
    /// Output past the offset asked from.
    pub(crate) output: Vec<u8>,
    /// The exit code, once it has ended; all its output is in by then.
    pub(crate) exit: Option<i32>,
}

/// Starts a job: `$1` is its files' stem; the rest is the command, run in
/// `/work` in a session of its own with the job's input on stdin.
const RUN: &str = r#"stem="$1"; shift
cd /work 2>/dev/null || cd /
setsid "$@" < "$stem.in" > "$stem.out" 2> "$stem.err" &
echo $! > "$stem.pid"
wait $!
echo $? > "$stem.exit.part" && mv "$stem.exit.part" "$stem.exit"
"#;

/// Stops every job that has not ended: its whole process group.
const STOP: &str = r#"for pid in /tmp/muster/out/*.pid; do
  [ -e "$pid" ] || continue
  stem="${pid%.pid}"
  [ -e "$stem.exit" ] && continue
  kill -TERM -- "-$(cat "$pid")" 2>/dev/null || kill -TERM "$(cat "$pid")" 2>/dev/null
done
true
"#;

impl Job {
    pub(crate) fn named(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }

    fn stem(&self) -> String {
        format!("{STATE_DIR}/out/{}", self.name)
    }

    /// Whether the job was started: its process id says so.
    pub(crate) async fn started(
        &self,
        docker: &Docker,
        container: &str,
    ) -> Result<bool, RuntimeError> {
        let pid = format!("{}.pid", self.stem());
        Ok(docker
            .exec(container, &["test", "-e", &pid], &[], LOOK)
            .await?
            .success())
    }

    /// Starts the job unless it already was (a restart asking again): its
    /// process id says so.
    pub(crate) async fn start(
        &self,
        docker: &Docker,
        container: &str,
        argv: &[&str],
        env: &[(&str, &str)],
        input: &[u8],
    ) -> Result<(), RuntimeError> {
        let stem = self.stem();
        let pid = format!("{stem}.pid");
        let started = docker
            .exec(container, &["test", "-e", &pid], &[], LOOK)
            .await?;
        if started.success() {
            return Ok(());
        }
        let input_file = format!("{stem}.in");
        let written = docker
            .exec_with(
                container,
                &[
                    "sh",
                    "-c",
                    "mkdir -p \"${1%/*}\" && cat > \"$1\"",
                    "sh",
                    &input_file,
                ],
                &[],
                Some(input),
                LOOK,
            )
            .await?;
        if !written.success() {
            return Err(RuntimeError::Failed {
                args: "exec (job input)".to_owned(),
                stderr: written.stderr,
            });
        }
        let mut command = vec!["sh", "-c", RUN, "sh", &stem];
        command.extend_from_slice(argv);
        docker.exec_detached(container, &command, env).await
    }

    /// The job's output from byte `offset` on, and its exit code once it
    /// has ended. The code is read first, so output read after it is all
    /// there is.
    pub(crate) async fn look(
        &self,
        docker: &Docker,
        container: &str,
        offset: u64,
    ) -> Result<Progress, RuntimeError> {
        let stem = self.stem();
        let exit = docker
            .exec(
                container,
                &[
                    "sh",
                    "-c",
                    "cat \"$1\" 2>/dev/null || true",
                    "sh",
                    &format!("{stem}.exit"),
                ],
                &[],
                LOOK,
            )
            .await?;
        let exit = String::from_utf8_lossy(&exit.stdout).trim().parse().ok();
        let start = (offset + 1).to_string();
        let output = docker
            .exec(
                container,
                &[
                    "sh",
                    "-c",
                    "tail -c \"+$1\" \"$2\" 2>/dev/null || true",
                    "sh",
                    &start,
                    &format!("{stem}.out"),
                ],
                &[],
                LOOK,
            )
            .await?;
        Ok(Progress {
            output: output.stdout,
            exit,
        })
    }

    /// The end of what the job wrote to stderr.
    pub(crate) async fn errors(&self, docker: &Docker, container: &str) -> String {
        let file = format!("{}.err", self.stem());
        docker
            .exec(
                container,
                &[
                    "sh",
                    "-c",
                    "tail -c 4000 \"$1\" 2>/dev/null || true",
                    "sh",
                    &file,
                ],
                &[],
                LOOK,
            )
            .await
            .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
            .unwrap_or_default()
    }
}

/// Stops every job in the container that has not ended.
pub(crate) async fn stop_all(docker: &Docker, container: &str) -> Result<(), RuntimeError> {
    docker
        .exec(container, &["sh", "-c", STOP], &[], LOOK)
        .await
        .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runs::provision::tests::test_image;

    struct Container(String);

    impl Drop for Container {
        fn drop(&mut self) {
            let _ = std::process::Command::new("docker")
                .args(["rm", "--force", &self.0])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    async fn container(docker: &Docker) -> Container {
        let image = test_image(docker).await;
        let name = format!(
            "muster-job-test-{}",
            muster_core::RunId::new().ulid().to_string().to_lowercase()
        );
        let id = docker
            .start(&crate::containers::ContainerSpec {
                name: name.clone(),
                image,
                labels: Vec::new(),
                memory_bytes: 512 << 20,
                millicpus: 1000,
                mounts: Vec::new(),
            })
            .await
            .unwrap();
        let _ = id;
        Container(name)
    }

    async fn until_over(job: &Job, docker: &Docker, container: &str) -> (Vec<u8>, i32) {
        let mut output = Vec::new();
        for _ in 0..200 {
            let progress = job
                .look(docker, container, output.len() as u64)
                .await
                .unwrap();
            output.extend(progress.output);
            if let Some(code) = progress.exit {
                let rest = job
                    .look(docker, container, output.len() as u64)
                    .await
                    .unwrap();
                output.extend(rest.output);
                return (output, code);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("the job never ended");
    }

    #[tokio::test]
    async fn real_docker_runs_a_job_once_reads_it_back_and_stops_it() {
        if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
            return;
        }
        let docker = Docker::default();
        let container = container(&docker).await;
        let name = &container.0;

        // Input on stdin, output read back, the exit code kept.
        let job = Job::named("1-echo");
        let argv = [
            "sh",
            "-c",
            "read line; echo \"got $line\"; echo \"$SECRET\" >&2; exit 3",
        ];
        job.start(&docker, name, &argv, &[("SECRET", "shh")], b"hello\n")
            .await
            .unwrap();
        // Asked again, as after a restart: it does not run twice.
        job.start(&docker, name, &argv, &[], b"again\n")
            .await
            .unwrap();
        let (output, code) = until_over(&job, &docker, name).await;
        assert_eq!(String::from_utf8_lossy(&output), "got hello\n");
        assert_eq!(code, 3);
        assert_eq!(job.errors(&docker, name).await.trim(), "shh");

        // A long job and what it started are stopped together.
        let long = Job::named("2-long");
        long.start(
            &docker,
            name,
            &["sh", "-c", "sleep 600 & sleep 600; echo done"],
            &[],
            b"",
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        stop_all(&docker, name).await.unwrap();
        let (output, code) = until_over(&long, &docker, name).await;
        assert!(output.is_empty());
        assert_ne!(code, 0);
        let left = docker
            .exec(
                name,
                &["sh", "-c", "ps -o pid,args | grep '[s]leep 600' || true"],
                &[],
                LOOK,
            )
            .await
            .unwrap();
        assert!(
            left.stdout.is_empty(),
            "left running: {}",
            String::from_utf8_lossy(&left.stdout)
        );
    }
}
