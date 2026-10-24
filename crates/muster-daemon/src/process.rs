//! Running a program the daemon asks something of to its end: git, gh,
//! glab, the Keychain's `security` and docker. Each runs in a process group
//! of its own, killed whole if it overstays its time, so nothing it started
//! is left behind (`ARC-13`); what it is given is written while what it
//! says is read, so neither waits on the other; and of what it says only
//! so much is kept (`ARC-14`).

use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// The most of its standard output, and of its standard error, kept: more
/// than any answer a client is sent can hold (`muster_core::frame`).
const MOST_SAID: u64 = 16 << 20;

/// How a program ended, and what it said.
#[derive(Debug)]
pub(crate) struct Ran {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl Ran {
    pub(crate) fn success(&self) -> bool {
        self.status.success()
    }

    /// What it printed, as text.
    pub(crate) fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// What it complained, as text, trimmed.
    pub(crate) fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).trim().to_owned()
    }
}

/// Why a program could not be run to its end.
#[derive(Debug, thiserror::Error)]
pub(crate) enum RunError {
    #[error("it is not installed")]
    NotFound,
    #[error("could not start it: {0}")]
    Spawn(std::io::Error),
    #[error("it did not finish within {0:?}")]
    TimedOut(Duration),
}

/// Runs `command` to its end within `limit`, with `input` on its standard
/// input (none where `None`). Past the limit it is killed with whatever it
/// started.
pub(crate) async fn run(
    mut command: Command,
    input: Option<&[u8]>,
    limit: Duration,
) -> Result<Ran, RunError> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            RunError::NotFound
        } else {
            RunError::Spawn(error)
        }
    })?;
    let group = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .map(nix::unistd::Pid::from_raw);
    let (stdin, stdout, stderr) = (child.stdin.take(), child.stdout.take(), child.stderr.take());
    let work = async {
        let write = async {
            if let (Some(input), Some(mut stdin)) = (input, stdin) {
                // A program that stops reading early is its own business.
                let _ = stdin.write_all(input).await;
            }
        };
        let ((), stdout, stderr) = tokio::join!(write, kept(stdout), kept(stderr));
        let status = child.wait().await?;
        Ok::<_, std::io::Error>(Ran {
            status,
            stdout,
            stderr,
        })
    };
    match tokio::time::timeout(limit, work).await {
        Ok(Ok(ran)) => Ok(ran),
        Ok(Err(error)) => Err(RunError::Spawn(error)),
        Err(_) => {
            if let Some(group) = group {
                let _ = nix::sys::signal::killpg(group, nix::sys::signal::Signal::SIGKILL);
            }
            Err(RunError::TimedOut(limit))
        }
    }
}

/// What `stream` says, up to [`MOST_SAID`]; the rest read and let go, so
/// the program never waits to write it.
async fn kept(stream: Option<impl AsyncRead + Unpin>) -> Vec<u8> {
    let mut said = Vec::new();
    if let Some(mut stream) = stream {
        let _ = (&mut stream).take(MOST_SAID).read_to_end(&mut said).await;
        let _ = tokio::io::copy(&mut stream, &mut tokio::io::sink()).await;
    }
    said
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", script]);
        command
    }

    #[tokio::test]
    async fn what_it_is_given_and_what_it_says_pass_both_ways_at_once() {
        // More than a pipe holds each way: written and read together, or
        // the two would wait on each other.
        let input = vec![b'x'; 1 << 20];
        let ran = run(
            sh("cat; echo done >&2"),
            Some(&input),
            Duration::from_secs(10),
        )
        .await
        .unwrap();
        assert!(ran.success());
        assert_eq!(ran.stdout.len(), input.len());
        assert_eq!(ran.stderr_text(), "done");
    }

    #[tokio::test]
    async fn one_that_overstays_is_killed_with_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid = dir.path().join("pid");
        let script = format!("sleep 30 & echo $! > '{}'; wait", pid.display());
        let error = run(sh(&script), None, Duration::from_millis(500))
            .await
            .unwrap_err();
        assert!(matches!(error, RunError::TimedOut(_)), "{error}");
        let child: i32 = std::fs::read_to_string(&pid)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let gone = || nix::sys::signal::kill(nix::unistd::Pid::from_raw(child), None).is_err();
        crate::waiting::until("what it started to be gone", async || gone()).await;
    }

    #[tokio::test]
    async fn a_program_not_there_says_so() {
        let error = run(
            Command::new("muster-no-such-program"),
            None,
            Duration::from_secs(5),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, RunError::NotFound), "{error}");
    }
}
