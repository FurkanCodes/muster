//! One run of a hook: its plugin's manifest checked against what the
//! person approved, its command filled in from its event, its program
//! started in the plugin's folder in a process group of its own, from the
//! basics of the environment sessions get and what its manifest names
//! (`ARC-23`), told the event on stdin; its stdout and stderr read
//! together and capped; stopped with its group at its deadline (`ARC-13`).
//! What it came to goes to its plugin's log, and to whoever waits.

use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use muster_core::plugins::events::EVENT_VARIABLE;
use muster_core::plugins::{EventKind, manifest};
use muster_core::protocol::plugins::{HookOutcome, HookRun};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use super::{Job, Limits, lock};
use crate::secrets::{Secrets, Token};
use crate::trackers::plugin::streams::Log;

/// How long what a hook left running may hold its pipes open once its
/// program has ended, before it is stopped with its group.
const DRAIN: Duration = Duration::from_millis(200);

/// What a run is given besides its job.
pub(super) struct Given<'a> {
    pub(super) secrets: &'a dyn Secrets,
    pub(super) limits: Limits,
    pub(super) log: &'a Mutex<Log>,
    /// Set once it is cut.
    pub(super) cut: &'a AtomicBool,
    /// Told its process group as it starts, so it can be cut.
    pub(super) started: &'a (dyn Fn(Pid) + Send + Sync),
}

/// Runs `job`, writing what it came to in its plugin's log and telling
/// whoever waits.
pub(super) async fn run(job: Job, given: &Given<'_>) {
    let began = tokio::time::Instant::now();
    let (outcome, output) = match prepare(&job, given).await {
        Ok(prepared) => execute(prepared, &job, given).await,
        Err(why) => (HookOutcome::NotRun { why }, Vec::new()),
    };
    let millis = u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX);
    let output = {
        let mut log = lock(given.log);
        let output = clean(&log, &output);
        log.push(entry(&job.hook.id, job.event.event, &outcome, millis, &output).as_bytes());
        output
    };
    match &outcome {
        HookOutcome::Exited { code: 0 } => {}
        outcome => tracing::info!(
            plugin = %job.plugin,
            hook = %job.hook.id,
            event = %job.event.event,
            outcome = %outcome.words(),
            "a hook did not end well"
        ),
    }
    if let Some(done) = job.done {
        let _ = done.send(HookRun {
            hook: job.hook.id,
            outcome,
            output,
            millis,
        });
    }
}

/// What a run's log says of it: its hook, its event and how it ended,
/// then what it said.
pub(super) fn entry(
    hook: &str,
    event: EventKind,
    outcome: &HookOutcome,
    millis: u64,
    output: &str,
) -> String {
    let took = if millis > 0 {
        format!(
            " after {:.1} s",
            Duration::from_millis(millis).as_secs_f32()
        )
    } else {
        String::new()
    };
    let mut entry = format!("[hook {hook} · {event}] {}{took}\n", outcome.words());
    if !output.is_empty() {
        entry.push_str(output);
        if !output.ends_with('\n') {
            entry.push('\n');
        }
    }
    entry
}

/// `output` as read, its plugin's secrets and anything shaped like a
/// credential hidden, without the controls a terminal would act on.
fn clean(log: &Log, output: &[u8]) -> String {
    muster_core::text::multi_line(&log.hidden(&String::from_utf8_lossy(output)))
}

/// A run ready to start.
struct Prepared {
    program: String,
    args: Vec<String>,
    variables: Vec<(String, String)>,
    secrets: Vec<Token>,
}

/// What `job` runs, or why it does not.
async fn prepare(job: &Job, given: &Given<'_>) -> Result<Prepared, String> {
    let folder = job.folder.clone();
    let hashed = tokio::task::spawn_blocking(move || hash(&folder))
        .await
        .ok()
        .flatten();
    if hashed.as_deref() != Some(job.sha256.as_str()) {
        return Err(
            "its manifest changed since you approved it: update it, or link it again".to_owned(),
        );
    }
    let context = job.event.context();
    let mut command = context
        .expand(&job.hook.command)
        .map_err(|missing| missing.to_string())?;
    if let Some(inside) = command
        .first()
        .and_then(|program| program.strip_prefix("./"))
    {
        command[0] = job.folder.join(inside).to_string_lossy().into_owned();
    }
    let mut command = command.into_iter();
    let program = command.next().unwrap_or_default();
    let mut variables: Vec<(String, String)> = job
        .env
        .iter()
        .filter_map(|name| Some((name.clone(), std::env::var(name).ok()?)))
        .collect();
    let mut secrets = Vec::new();
    for name in &job.secrets {
        if let Some(secret) = crate::secrets::plugin_secret(given.secrets, &job.plugin, name).await
        {
            variables.push((name.clone(), secret.value().to_owned()));
            secrets.push(secret);
        }
    }
    variables.extend(context.variables());
    variables.push((EVENT_VARIABLE.to_owned(), job.event.event.name().to_owned()));
    Ok(Prepared {
        program,
        args: command.collect(),
        variables,
        secrets,
    })
}

/// The SHA-256 of the manifest in `folder` as it is now, if it reads: a
/// file of its own, no larger than a manifest may be.
fn hash(folder: &Path) -> Option<String> {
    use std::io::Read as _;
    let path = folder.join(manifest::FILE_NAME);
    if std::fs::symlink_metadata(&path)
        .ok()?
        .file_type()
        .is_symlink()
    {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .ok()?
        .take(manifest::MOST_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= manifest::MOST_BYTES).then(|| crate::plugins::sha256(&bytes))
}

/// Starts `prepared` and sees it to its end: how it ended, and what it
/// said.
async fn execute(prepared: Prepared, job: &Job, given: &Given<'_>) -> (HookOutcome, Vec<u8>) {
    lock(given.log).hide(prepared.secrets);
    let mut command = tokio::process::Command::new(&prepared.program);
    command
        .args(&prepared.args)
        .current_dir(&job.folder)
        .env_clear()
        .envs(crate::trackers::plugin::basics(std::env::vars_os()))
        .envs(prepared.variables)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .kill_on_drop(true);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            let why = format!("could not start {}: {error}", prepared.program);
            return (HookOutcome::NotRun { why }, Vec::new());
        }
    };
    let group = child
        .id()
        .and_then(|pid| i32::try_from(pid).ok())
        .map(Pid::from_raw);
    if let Some(group) = group {
        (given.started)(group);
    }
    let signal = |signal: Signal| {
        if let Some(group) = group {
            let _ = killpg(group, signal);
        }
    };
    let (Some(mut stdin), Some(stdout), Some(stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        signal(Signal::SIGKILL);
        let why = "its pipes could not be opened".to_owned();
        return (HookOutcome::NotRun { why }, Vec::new());
    };
    let line = job.event.line();
    let deadline = Duration::from_secs(job.hook.timeout);
    let mut output = Output::new(given.limits.output);
    let ran = tokio::time::timeout(deadline, async {
        // A hook that reads nothing still runs: its stdin is a line, and
        // then closed.
        let _ = stdin.write_all(line.as_bytes()).await;
        drop(stdin);
        let reading = output.read(stdout, stderr);
        tokio::pin!(reading);
        let mut read = false;
        let status = loop {
            tokio::select! {
                status = child.wait() => break status,
                () = &mut reading, if !read => read = true,
            }
        };
        // What it left running may hold its pipes: a moment, then it goes.
        if !read && tokio::time::timeout(DRAIN, &mut reading).await.is_err() {
            signal(Signal::SIGKILL);
            let _ = tokio::time::timeout(DRAIN, &mut reading).await;
        }
        status
    })
    .await;
    let outcome = if let Ok(status) = ran {
        // Whatever it left running goes with it (`ARC-13`).
        signal(Signal::SIGKILL);
        ended(status.ok(), given.cut)
    } else {
        // Past its deadline: told to stop, then killed, with its group.
        signal(Signal::SIGTERM);
        if tokio::time::timeout(given.limits.stop_grace, child.wait())
            .await
            .is_err()
        {
            signal(Signal::SIGKILL);
            let _ = child.wait().await;
        }
        signal(Signal::SIGKILL);
        if given.cut.load(Ordering::SeqCst) {
            HookOutcome::Cut
        } else {
            HookOutcome::TimedOut {
                seconds: job.hook.timeout,
            }
        }
    };
    (outcome, output.into_bytes())
}

/// How a run whose program ended with `status` came to be.
fn ended(status: Option<ExitStatus>, cut: &AtomicBool) -> HookOutcome {
    if cut.load(Ordering::SeqCst) {
        return HookOutcome::Cut;
    }
    match status.and_then(|status| status.code()) {
        Some(code) => HookOutcome::Exited { code },
        None => HookOutcome::Signalled,
    }
}

/// What a run says on stdout and stderr together, kept up to its cap; the
/// rest is read and let go, so a hook that says a lot is never stopped
/// waiting to say it.
struct Output {
    bytes: Vec<u8>,
    most: usize,
    over: bool,
}

impl Output {
    fn new(most: usize) -> Self {
        Self {
            bytes: Vec::new(),
            most,
            over: false,
        }
    }

    /// Reads `stdout` and `stderr` until both close.
    async fn read(
        &mut self,
        mut stdout: impl AsyncRead + Unpin,
        mut stderr: impl AsyncRead + Unpin,
    ) {
        let (mut out, mut err) = (vec![0; 4096], vec![0; 4096]);
        let (mut out_open, mut err_open) = (true, true);
        while out_open || err_open {
            tokio::select! {
                read = stdout.read(&mut out), if out_open => match read {
                    Ok(0) | Err(_) => out_open = false,
                    Ok(count) => self.push(&out[..count]),
                },
                read = stderr.read(&mut err), if err_open => match read {
                    Ok(0) | Err(_) => err_open = false,
                    Ok(count) => self.push(&err[..count]),
                },
            }
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        let room = self.most.saturating_sub(self.bytes.len());
        if bytes.len() > room {
            self.over = true;
        }
        self.bytes
            .extend_from_slice(&bytes[..bytes.len().min(room)]);
    }

    fn into_bytes(mut self) -> Vec<u8> {
        if self.over {
            self.bytes.extend_from_slice(
                format!("\n[more than {} KiB; the rest let go]\n", self.most / 1024).as_bytes(),
            );
        }
        self.bytes
    }
}
