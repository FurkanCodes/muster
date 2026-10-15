//! Containers, through the `docker` command (`ARCHITECTURE.md` §8).
//!
//! The command rather than the Engine API: it finds the person's runtime
//! the way they use it (the current Docker context: Docker Desktop,
//! `OrbStack`, Colima), much as `gh` brings their GitHub login, and tests
//! fake it on `PATH` like `gh` and `claude`. It runs without a shell, with
//! a time limit, and values meant for a container's processes go through
//! the command's environment (`-e NAME`), never its arguments, which other
//! processes can read.
//!
//! A container's main process does nothing (`tail -f /dev/null` under
//! Docker's init), so it lives between stages and outlives the daemon
//! (`ARC-04`); work runs in it with `exec`.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::Deserialize;
use tokio::process::Command;

/// How long a command that only asks or tells Docker something may take.
const QUICK: Duration = Duration::from_secs(60);

/// The label Muster puts on a run's container, and looks for after a
/// restart.
pub(crate) const LABEL_RUN: &str = "muster.run";

/// Why Docker could not do something.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RuntimeError {
    #[error("the docker command is not installed; install Docker Desktop, OrbStack or Colima")]
    Missing,
    #[error("Docker is not running; start it, then retry ({0})")]
    NotRunning(String),
    /// The container named is not there.
    #[error("the container is gone: {0}")]
    NoContainer(String),
    /// The container is there but stopped.
    #[error("the container is stopped: {0}")]
    Stopped(String),
    #[error("could not run docker: {0}")]
    Spawn(std::io::Error),
    #[error("`docker {args}` failed: {stderr}")]
    Failed { args: String, stderr: String },
    #[error("`docker {0}` did not finish in time")]
    TimedOut(String),
    #[error("docker answered something unexpected: {0}")]
    Unreadable(String),
}

/// What the container runtime has to offer.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "the scheduler (M3) budgets with it")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Capacity {
    pub(crate) memory_bytes: u64,
    pub(crate) cpus: u32,
}

/// A container to start.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ContainerSpec {
    pub(crate) name: String,
    pub(crate) image: String,
    pub(crate) labels: Vec<(String, String)>,
    /// The hard memory limit, which is also the run's reservation.
    pub(crate) memory_bytes: u64,
    pub(crate) millicpus: u32,
    pub(crate) mounts: Vec<Mount>,
}

/// A directory of the host's, or a named volume, in a container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Mount {
    pub(crate) source: MountSource,
    pub(crate) target: String,
    pub(crate) read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MountSource {
    Host(PathBuf),
    Volume(String),
}

/// A container Docker knows, as listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) running: bool,
}

/// What a finished `exec` printed, and how it exited.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExecOutput {
    pub(crate) code: Option<i32>,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: String,
}

impl ExecOutput {
    pub(crate) fn success(&self) -> bool {
        self.code == Some(0)
    }
}

/// The `docker` command.
#[derive(Clone, Debug)]
pub(crate) struct Docker {
    program: String,
}

impl Default for Docker {
    fn default() -> Self {
        Self {
            program: "docker".to_owned(),
        }
    }
}

impl Docker {
    /// The `docker` command at `program`, for tests.
    #[cfg(test)]
    pub(crate) fn with_program(program: &str) -> Self {
        Self {
            program: program.to_owned(),
        }
    }

    /// The runtime's memory and CPUs; fails plainly if Docker is not there
    /// or not running.
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the scheduler (M3) budgets with it")
    )]
    pub(crate) async fn capacity(&self) -> Result<Capacity, RuntimeError> {
        #[derive(Deserialize)]
        struct Info {
            #[serde(rename = "MemTotal")]
            memory: u64,
            #[serde(rename = "NCPU")]
            cpus: u32,
        }
        let output = self.quick(&["info", "--format", "{{json .}}"], &[]).await?;
        let info: Info = serde_json::from_slice(&output)
            .map_err(|error| RuntimeError::Unreadable(error.to_string()))?;
        Ok(Capacity {
            memory_bytes: info.memory,
            cpus: info.cpus,
        })
    }

    /// The containers carrying `label` with `value`, running or not.
    pub(crate) async fn find(&self, label: &str, value: &str) -> Result<Vec<Found>, RuntimeError> {
        let filter = format!("label={label}={value}");
        let output = self
            .quick(
                &[
                    "ps",
                    "--all",
                    "--no-trunc",
                    "--filter",
                    &filter,
                    "--format",
                    "{{json .}}",
                ],
                &[],
            )
            .await?;
        parse_ps(&output)
    }

    /// The containers carrying `label`, whatever its value, with that
    /// value; running or not.
    pub(crate) async fn labelled(&self, label: &str) -> Result<Vec<(Found, String)>, RuntimeError> {
        let filter = format!("label={label}");
        let output = self
            .quick(
                &[
                    "ps",
                    "--all",
                    "--no-trunc",
                    "--filter",
                    &filter,
                    "--format",
                    "{{json .}}",
                ],
                &[],
            )
            .await?;
        parse_labelled(&output, label)
    }

    /// Starts a container from `spec` and returns its id. Docker refuses a
    /// second container of the same name, so a retry after a restart finds
    /// the first one instead (see [`find`](Self::find)).
    pub(crate) async fn start(&self, spec: &ContainerSpec) -> Result<String, RuntimeError> {
        let args = run_args(spec);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self.quick(&args, &[]).await?;
        let id = String::from_utf8_lossy(&output).trim().to_owned();
        if id.is_empty() {
            return Err(RuntimeError::Unreadable("no container id".to_owned()));
        }
        Ok(id)
    }

    /// Runs `argv` in the container and waits, at most `limit`. Each
    /// `(name, value)` in `env` reaches the process through docker's own
    /// environment, so the value never appears in docker's arguments.
    pub(crate) async fn exec(
        &self,
        container: &str,
        argv: &[&str],
        env: &[(&str, &str)],
        limit: Duration,
    ) -> Result<ExecOutput, RuntimeError> {
        self.exec_with(container, argv, env, None, limit).await
    }

    /// As [`exec`](Self::exec), with `input` on the process's stdin.
    pub(crate) async fn exec_with(
        &self,
        container: &str,
        argv: &[&str],
        env: &[(&str, &str)],
        input: Option<&[u8]>,
        limit: Duration,
    ) -> Result<ExecOutput, RuntimeError> {
        let mut args = vec!["exec".to_owned()];
        if input.is_some() {
            args.push("--interactive".to_owned());
        }
        for (name, _) in env {
            args.push("--env".to_owned());
            args.push((*name).to_owned());
        }
        args.push(container.to_owned());
        args.extend(argv.iter().map(|arg| (*arg).to_owned()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self.run_with(&args, env, input, limit).await?;
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if !output.status.success()
            && let Some(error) = docker_failure(&stderr)
        {
            return Err(error);
        }
        Ok(ExecOutput {
            code: output.status.code(),
            stdout: output.stdout,
            stderr,
        })
    }

    /// Starts `argv` in the container and returns without waiting: it goes
    /// on after docker, and the daemon, have gone.
    pub(crate) async fn exec_detached(
        &self,
        container: &str,
        argv: &[&str],
        env: &[(&str, &str)],
    ) -> Result<(), RuntimeError> {
        let mut args = vec!["exec".to_owned(), "--detach".to_owned()];
        for (name, _) in env {
            args.push("--env".to_owned());
            args.push((*name).to_owned());
        }
        args.push(container.to_owned());
        args.extend(argv.iter().map(|arg| (*arg).to_owned()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        self.quick(&args, env).await.map(drop)
    }

    /// Starts a stopped container again.
    pub(crate) async fn restart(&self, container: &str) -> Result<(), RuntimeError> {
        self.quick(&["start", container], &[]).await.map(drop)
    }

    /// Whether the image `tag` is there.
    pub(crate) async fn has_image(&self, tag: &str) -> Result<bool, RuntimeError> {
        match self
            .quick(&["image", "inspect", "--format", "{{.Id}}", tag], &[])
            .await
        {
            Ok(_) => Ok(true),
            Err(RuntimeError::Failed { stderr, .. })
                if stderr.contains("No such image") || stderr.contains("No such object") =>
            {
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }

    /// Builds `tag` from `dockerfile`, in `context` if given (else with
    /// none), within `limit`; on failure, the end of docker's output says
    /// why.
    pub(crate) async fn build(
        &self,
        tag: &str,
        dockerfile: &str,
        context: Option<&std::path::Path>,
        limit: Duration,
    ) -> Result<(), RuntimeError> {
        let context = context.map(|path| path.display().to_string());
        let mut args = vec!["build", "--quiet", "--tag", tag];
        match &context {
            Some(context) => args.extend(["--file", "-", context.as_str()]),
            None => args.push("-"),
        }
        let output = self
            .run_with(&args, &[], Some(dockerfile.as_bytes()), limit)
            .await?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut tail: Vec<&str> = stderr.lines().rev().take(15).collect();
        tail.reverse();
        Err(RuntimeError::Failed {
            args: format!("build {tag}"),
            stderr: tail.join("\n"),
        })
    }

    /// Removes the container, stopping it first; one already gone is fine.
    pub(crate) async fn remove(&self, container: &str) -> Result<(), RuntimeError> {
        match self.quick(&["rm", "--force", container], &[]).await {
            Ok(_) | Err(RuntimeError::NoContainer(_)) => Ok(()),
            Err(RuntimeError::Failed { stderr, .. }) if stderr.contains("No such container") => {
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    /// A command that is quick, must succeed, and whose output is wanted.
    async fn quick(&self, args: &[&str], env: &[(&str, &str)]) -> Result<Vec<u8>, RuntimeError> {
        let output = self.run(args, env, QUICK).await?;
        if output.status.success() {
            return Ok(output.stdout);
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if let Some(error) = docker_failure(&stderr) {
            return Err(error);
        }
        Err(RuntimeError::Failed {
            args: args.join(" "),
            stderr,
        })
    }

    async fn run(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
        limit: Duration,
    ) -> Result<std::process::Output, RuntimeError> {
        self.run_with(args, env, None, limit).await
    }

    async fn run_with(
        &self,
        args: &[&str],
        env: &[(&str, &str)],
        input: Option<&[u8]>,
        limit: Duration,
    ) -> Result<std::process::Output, RuntimeError> {
        use tokio::io::AsyncWriteExt;
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (name, value) in env {
            command.env(name, value);
        }
        let work = async {
            let mut child = command.spawn()?;
            if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
                // A process that stops reading early is its own business.
                let _ = stdin.write_all(input).await;
                drop(stdin);
            }
            child.wait_with_output().await
        };
        match tokio::time::timeout(limit, work).await {
            Err(_) => Err(RuntimeError::TimedOut(
                args.first().copied().unwrap_or("").to_owned(),
            )),
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Err(RuntimeError::Missing)
            }
            Ok(Err(error)) => Err(RuntimeError::Spawn(error)),
            Ok(Ok(output)) => Ok(output),
        }
    }
}

/// `docker run`'s arguments for `spec`.
fn run_args(spec: &ContainerSpec) -> Vec<String> {
    let mut args: Vec<String> = ["run", "--detach", "--init", "--name", &spec.name]
        .map(str::to_owned)
        .to_vec();
    for (key, value) in &spec.labels {
        args.push("--label".to_owned());
        args.push(format!("{key}={value}"));
    }
    args.push("--memory".to_owned());
    args.push(spec.memory_bytes.to_string());
    // No swap beyond the limit: the reservation is what it may use.
    args.push("--memory-swap".to_owned());
    args.push(spec.memory_bytes.to_string());
    args.push("--cpus".to_owned());
    args.push(format!(
        "{}.{:03}",
        spec.millicpus / 1000,
        spec.millicpus % 1000
    ));
    for mount in &spec.mounts {
        let (kind, source) = match &mount.source {
            MountSource::Host(path) => ("bind", path.display().to_string()),
            MountSource::Volume(name) => ("volume", name.clone()),
        };
        let mut value = format!("type={kind},source={source},target={}", mount.target);
        if mount.read_only {
            value.push_str(",readonly");
        }
        args.push("--mount".to_owned());
        args.push(value);
    }
    // Whatever the image's own entrypoint, the container idles until work
    // is run in it.
    args.extend(["--entrypoint", "tail", &spec.image, "-f", "/dev/null"].map(str::to_owned));
    args
}

/// `docker ps --format '{{json .}}'`: one object per line.
fn parse_ps(output: &[u8]) -> Result<Vec<Found>, RuntimeError> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "ID")]
        id: String,
        #[serde(rename = "Names")]
        names: String,
        #[serde(rename = "State")]
        state: String,
    }
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let parsed: Line = serde_json::from_str(line)
                .map_err(|error| RuntimeError::Unreadable(format!("{error}: {line}")))?;
            Ok(Found {
                id: parsed.id,
                name: parsed.names,
                running: parsed.state == "running",
            })
        })
        .collect()
}

/// `docker ps --format '{{json .}}'` with each container's value of
/// `label`, from its `Labels` (`a=1,b=2`).
fn parse_labelled(output: &[u8], label: &str) -> Result<Vec<(Found, String)>, RuntimeError> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "Labels", default)]
        labels: String,
    }
    let found = parse_ps(output)?;
    let lines: Vec<&str> = std::str::from_utf8(output)
        .map_err(|error| RuntimeError::Unreadable(error.to_string()))?
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let prefix = format!("{label}=");
    found
        .into_iter()
        .zip(lines)
        .map(|(found, line)| {
            let parsed: Line = serde_json::from_str(line)
                .map_err(|error| RuntimeError::Unreadable(format!("{error}: {line}")))?;
            let value = parsed
                .labels
                .split(',')
                .find_map(|pair| pair.strip_prefix(&prefix))
                .unwrap_or_default()
                .to_owned();
            Ok((found, value))
        })
        .collect()
}

/// Whether docker's complaint means the runtime is not up.
fn is_not_running(stderr: &str) -> bool {
    stderr.contains("Cannot connect to the Docker daemon")
        || stderr.contains("Is the docker daemon running")
        || stderr.contains("error during connect")
}

/// A failure of docker's own, rather than of the command it ran: the
/// runtime is down, or the container is gone or stopped. Docker says so on
/// its first line (`Error response from daemon: …`, checked against Docker
/// Desktop 4).
fn docker_failure(stderr: &str) -> Option<RuntimeError> {
    if is_not_running(stderr) {
        return Some(RuntimeError::NotRunning(stderr.to_owned()));
    }
    let first = stderr.lines().next().unwrap_or_default();
    if !first.starts_with("Error response from daemon:") {
        return None;
    }
    if first.contains("No such container") {
        Some(RuntimeError::NoContainer(first.to_owned()))
    } else if first.contains("is not running") || first.contains("is paused") {
        Some(RuntimeError::Stopped(first.to_owned()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
