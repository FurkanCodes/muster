//! Work that runs in a run's container on its own, outliving the docker
//! command that started it and the daemon (`ARC-04`): a headless agent's
//! turn, or the checks. Each job has a name, `<generation>-<role>`, and its
//! files under the state directory: what it was given (`.in`), its runner's
//! and its own process ids (`.runner`, `.pid`), what it wrote (`.out`), its
//! errors (`.err`) and its exit code (`.exit`, written last). It runs in a
//! session of its own, so stopping it reaches whatever it started. The
//! daemon reads the output back from an offset, again from the start after
//! a restart, and knows a job is over when its exit code is there, or lost
//! when its runner is gone without one (the container stopped under it).
//!
//! A process id is trusted only if that process carries the job's name in
//! its environment (`MUSTER_JOB`), since a restarted container reuses ids.
//! Stopping work leaves a mark of the generation it stopped below, which a
//! job starting late checks once its id is written, so work overtaken while
//! it was being started never runs on.

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

/// What a look at a job found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Progress {
    /// Output past the offset asked from.
    pub(crate) output: Vec<u8>,
    pub(crate) state: JobState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JobState {
    Running,
    /// It ended with this code; all its output is in.
    Ended(i32),
    /// Its runner is gone without an exit code: the container stopped (or
    /// restarted) under it, and it will never end.
    Lost,
}

/// Whether the process whose id is in the file `$1` carries job `$2`.
const OURS: &str = r#"ours() {
  [ -s "$1" ] || return 1
  tr '\0' '\n' < "/proc/$(cat "$1")/environ" 2>/dev/null | grep -qx "MUSTER_JOB=$2"
}
"#;

/// Starts a job: `$1` is its files' stem; the rest is the command, run in
/// `/work` in a session of its own with the job's input on stdin. Docker
/// gives it `MUSTER_JOB`, its name.
const RUN: &str = r#"stem="$1"; shift
echo $$ > "$stem.runner"
cd /work 2>/dev/null || cd /
setsid "$@" < "$stem.in" > "$stem.out" 2> "$stem.err" &
child=$!
echo "$child" > "$stem.pid"
name="${stem##*/}"; generation="${name%%-*}"
before="$(cat /tmp/muster/stop-before 2>/dev/null || echo 0)"
if [ "$generation" -lt "$before" ] 2>/dev/null; then
  kill -TERM -- "-$child" 2>/dev/null || kill -TERM "$child" 2>/dev/null
fi
wait "$child"
echo $? > "$stem.exit.part" && mv "$stem.exit.part" "$stem.exit"
"#;

/// The job's state on its first line, then its output from byte `$2`: `$1`
/// is its files' stem. The exit code is read before the output, so output
/// read after it is all there is.
const LOOK_AT: &str = r#"stem="$1"; job="${stem##*/}"
if [ -e "$stem.exit" ]; then
  echo "ended $(cat "$stem.exit")"
elif [ ! -e "$stem.runner" ] || ours "$stem.runner" "$job"; then
  echo running
elif [ -e "$stem.exit" ]; then
  echo "ended $(cat "$stem.exit")"
else
  echo lost
fi
tail -c "+$2" "$stem.out" 2>/dev/null || true
"#;

/// Stops every job that has not ended and was asked for under a
/// generation before `$1` (its name starts with it): its whole process
/// group. The mark goes first, so a job starting meanwhile sees it.
const STOP: &str = r#"before="$1"
mkdir -p /tmp/muster/out
current="$(cat /tmp/muster/stop-before 2>/dev/null || echo 0)"
if [ "$before" -gt "$current" ] 2>/dev/null; then
  echo "$before" > /tmp/muster/stop-before.part && mv /tmp/muster/stop-before.part /tmp/muster/stop-before
fi
for pid in /tmp/muster/out/*.pid; do
  [ -e "$pid" ] || continue
  stem="${pid%.pid}"
  [ -e "$stem.exit" ] && continue
  name="${stem##*/}"; generation="${name%%-*}"
  case "$generation" in ''|*[!0-9]*) continue ;; esac
  [ "$generation" -lt "$before" ] || continue
  ours "$pid" "$name" || continue
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
    /// process id says so. `base_cost` is what the run had cost before it,
    /// kept with it so that a daemon following it again after a restart
    /// adds its cost to the same sum.
    pub(crate) async fn start(
        &self,
        docker: &Docker,
        container: &str,
        argv: &[&str],
        env: &[(&str, &str)],
        input: &[u8],
        base_cost: u64,
    ) -> Result<(), RuntimeError> {
        if self.started(docker, container).await? {
            return Ok(());
        }
        let stem = self.stem();
        let base_cost = base_cost.to_string();
        let written = docker
            .exec_with(
                container,
                &[
                    "sh",
                    "-c",
                    "mkdir -p \"${1%/*}\" && printf '%s' \"$2\" > \"$1.base\" && cat > \"$1.in\"",
                    "sh",
                    &stem,
                    &base_cost,
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
        let mut env = env.to_vec();
        env.push(("MUSTER_JOB", &self.name));
        docker.exec_detached(container, &command, &env).await
    }

    /// The job's state and its output from byte `offset` on, in one look.
    pub(crate) async fn look(
        &self,
        docker: &Docker,
        container: &str,
        offset: u64,
    ) -> Result<Progress, RuntimeError> {
        let script = format!("{OURS}{LOOK_AT}");
        let start = (offset + 1).to_string();
        let output = docker
            .exec(
                container,
                &["sh", "-c", &script, "sh", &self.stem(), &start],
                &[],
                LOOK,
            )
            .await?;
        let (first, output) = match output.stdout.iter().position(|byte| *byte == b'\n') {
            Some(at) => (
                String::from_utf8_lossy(&output.stdout[..at]).into_owned(),
                output.stdout[at + 1..].to_vec(),
            ),
            None => (
                String::from_utf8_lossy(&output.stdout).into_owned(),
                Vec::new(),
            ),
        };
        let state = match first.trim() {
            "lost" => JobState::Lost,
            ended if ended.starts_with("ended ") => ended["ended ".len()..]
                .trim()
                .parse()
                .map_or(JobState::Running, JobState::Ended),
            _ => JobState::Running,
        };
        Ok(Progress { output, state })
    }

    /// What the run had cost before the job, as it was started with.
    pub(crate) async fn base_cost(&self, docker: &Docker, container: &str) -> Option<u64> {
        let file = format!("{}.base", self.stem());
        let output = docker
            .exec(container, &["cat", &file], &[], LOOK)
            .await
            .ok()?;
        String::from_utf8_lossy(&output.stdout).trim().parse().ok()
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

/// Stops every job in the container that has not ended and was asked for
/// under a generation before `before`, and keeps any asked for meanwhile
/// from running on.
pub(crate) async fn stop_before(
    docker: &Docker,
    container: &str,
    before: u64,
) -> Result<(), RuntimeError> {
    let before = before.to_string();
    let script = format!("{OURS}{STOP}");
    docker
        .exec(container, &["sh", "-c", &script, "sh", &before], &[], LOOK)
        .await
        .map(drop)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::runs::provision::tests::test_image;

    /// A test container, removed when dropped.
    pub(crate) struct Container(pub(crate) String);

    impl Drop for Container {
        fn drop(&mut self) {
            let _ = std::process::Command::new("docker")
                .args(["rm", "--force", &self.0])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    pub(crate) async fn container(docker: &Docker) -> Container {
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
        let code = crate::waiting::found("the job to end", async || {
            let progress = job
                .look(docker, container, output.len() as u64)
                .await
                .unwrap();
            output.extend(progress.output);
            match progress.state {
                JobState::Ended(code) => Some(code),
                JobState::Lost => panic!("the job was lost"),
                JobState::Running => None,
            }
        })
        .await;
        (output, code)
    }

    /// The sleeps of `seconds` running in the container, not the shells
    /// that started them.
    async fn sleeping(docker: &Docker, container: &str, seconds: &str) -> usize {
        let script = format!("ps -o args | grep -x 'sleep {seconds}' || true");
        let found = docker
            .exec(container, &["sh", "-c", &script], &[], LOOK)
            .await
            .unwrap();
        String::from_utf8_lossy(&found.stdout).lines().count()
    }

    #[tokio::test]
    async fn real_docker_runs_a_job_once_reads_it_back_and_stops_it() {
        if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
            return;
        }
        let docker = Docker::default();
        let container = container(&docker).await;
        let name = &container.0;

        // Input on stdin, output read back, the exit code and the cost it
        // started from kept.
        let job = Job::named("1-echo");
        let argv = [
            "sh",
            "-c",
            "read line; echo \"got $line\"; echo \"$SECRET\" >&2; exit 3",
        ];
        job.start(
            &docker,
            name,
            &argv,
            &[("SECRET", "shh")],
            b"hello\n",
            250_000,
        )
        .await
        .unwrap();
        // Asked again, as after a restart: it does not run twice.
        job.start(&docker, name, &argv, &[], b"again\n", 0)
            .await
            .unwrap();
        let (output, code) = until_over(&job, &docker, name).await;
        assert_eq!(String::from_utf8_lossy(&output), "got hello\n");
        assert_eq!(code, 3);
        assert_eq!(job.errors(&docker, name).await.trim(), "shh");
        assert_eq!(job.base_cost(&docker, name).await, Some(250_000));

        // A long job and what it started are stopped together; work asked
        // for under a later generation is not.
        let long = Job::named("2-long");
        let sleeps = ["sh", "-c", "sleep 600 & sleep 600; echo done"];
        long.start(&docker, name, &sleeps, &[], b"", 0)
            .await
            .unwrap();
        let newer = Job::named("3-newer");
        let naps = ["sh", "-c", "sleep 500 & sleep 500; echo done"];
        newer
            .start(&docker, name, &naps, &[], b"", 0)
            .await
            .unwrap();
        crate::waiting::until("both jobs' sleeps to start", async || {
            sleeping(&docker, name, "600").await == 2 && sleeping(&docker, name, "500").await == 2
        })
        .await;
        stop_before(&docker, name, 3).await.unwrap();
        let (output, code) = until_over(&long, &docker, name).await;
        assert!(output.is_empty());
        assert_ne!(code, 0);
        assert_eq!(sleeping(&docker, name, "600").await, 0, "all of it stopped");
        assert_eq!(sleeping(&docker, name, "500").await, 2);
        assert_eq!(
            newer.look(&docker, name, 0).await.unwrap().state,
            JobState::Running
        );

        // Work of a stopped generation that starts late (its task was on
        // its way meanwhile) does not run on.
        let late = Job::named("2-late");
        let dozes = ["sh", "-c", "sleep 400"];
        late.start(&docker, name, &dozes, &[], b"", 0)
            .await
            .unwrap();
        let (_, code) = until_over(&late, &docker, name).await;
        assert_ne!(code, 0);
        assert_eq!(sleeping(&docker, name, "400").await, 0);
    }

    #[tokio::test]
    async fn real_docker_tells_a_job_lost_with_its_container_and_a_stale_pid_from_its_own() {
        if std::env::var("MUSTER_DOCKER_TESTS").as_deref() != Ok("1") {
            return;
        }
        let docker = Docker::default();
        let container = container(&docker).await;
        let name = &container.0;
        let job = Job::named("1-long");
        job.start(&docker, name, &["sleep", "600"], &[], b"", 0)
            .await
            .unwrap();
        crate::waiting::until("its sleep to start", async || {
            sleeping(&docker, name, "600").await == 1
        })
        .await;

        // The container restarts under it: its files stay, its processes go,
        // and their ids may be anyone's now.
        let restarted = std::process::Command::new("docker")
            .args(["restart", "--time", "1", name])
            .output()
            .unwrap();
        assert!(restarted.status.success());
        let state = job.look(&docker, name, 0).await.unwrap().state;
        assert_eq!(state, JobState::Lost);
        assert!(job.started(&docker, name).await.unwrap(), "it did start");

        // A process that took its old id is not stopped as its.
        let pid = String::from_utf8(
            docker
                .exec(name, &["cat", "/tmp/muster/out/1-long.pid"], &[], LOOK)
                .await
                .unwrap()
                .stdout,
        )
        .unwrap();
        let stranger = docker
            .exec(
                name,
                &["sh", "-c", "sleep 300 >/dev/null 2>&1 & echo $!"],
                &[],
                LOOK,
            )
            .await
            .unwrap();
        let stranger = String::from_utf8_lossy(&stranger.stdout).trim().to_owned();
        docker
            .exec(
                name,
                &[
                    "sh",
                    "-c",
                    &format!("echo {stranger} > /tmp/muster/out/1-long.pid"),
                ],
                &[],
                LOOK,
            )
            .await
            .unwrap();
        stop_before(&docker, name, 5).await.unwrap();
        assert_eq!(
            sleeping(&docker, name, "300").await,
            1,
            "{pid} was not ours"
        );

        // Gone altogether, it is an error of docker's, not the job's.
        let name = name.clone();
        drop(container);
        let gone = job.look(&docker, &name, 0).await.unwrap_err();
        assert!(matches!(gone, RuntimeError::NoContainer(_)), "{gone:?}");
    }
}
