//! Containers, through the `docker` command (`ARCHITECTURE.md` §8).
//!
//! The command rather than the Engine API: it finds the person's runtime
//! the way they use it (the current Docker context: Docker Desktop,
//! OrbStack, Colima), much as `gh` brings their GitHub login, and tests
//! fake it on `PATH` like `gh` and `claude`. It runs without a shell, with
//! a time limit, and values meant for a container's processes go through
//! the command's environment (`-e NAME`), never its arguments, which other
//! processes can read.
//!
//! A container's main process does nothing (`tail -f /dev/null` under
//! Docker's init), so it lives between stages and outlives the daemon
//! (`ARC-04`); work runs in it with `exec`.

// Wired into runs by a later M2 step; until then only its tests use it.
#![cfg_attr(not(test), allow(dead_code))]

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
    #[error("Docker is not running: {0}")]
    NotRunning(String),
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
    /// The runtime's memory and CPUs; fails plainly if Docker is not there
    /// or not running.
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
        let mut args = vec!["exec".to_owned()];
        for (name, _) in env {
            args.push("--env".to_owned());
            args.push((*name).to_owned());
        }
        args.push(container.to_owned());
        args.extend(argv.iter().map(|arg| (*arg).to_owned()));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let output = self.run(&args, env, limit).await?;
        Ok(ExecOutput {
            code: output.status.code(),
            stdout: output.stdout,
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }

    /// Removes the container, stopping it first; one already gone is fine.
    pub(crate) async fn remove(&self, container: &str) -> Result<(), RuntimeError> {
        match self.quick(&["rm", "--force", container], &[]).await {
            Ok(_) => Ok(()),
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
        if is_not_running(&stderr) {
            return Err(RuntimeError::NotRunning(stderr));
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
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (name, value) in env {
            command.env(name, value);
        }
        match tokio::time::timeout(limit, command.output()).await {
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

/// Whether docker's complaint means the runtime is not up.
fn is_not_running(stderr: &str) -> bool {
    stderr.contains("Cannot connect to the Docker daemon")
        || stderr.contains("Is the docker daemon running")
        || stderr.contains("error during connect")
}

#[cfg(test)]
mod tests;
