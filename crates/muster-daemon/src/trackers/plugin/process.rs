//! A ticket plugin's process: started at the first request, said `hello`
//! to, then asked by the ticket protocol. Bounded every way (`ARC-13`,
//! `ARC-14`): at most eight requests wait on it, each with a deadline; a
//! line it answers is at most 4 MiB; the end of its stderr is kept as its
//! log. It stops after five minutes unused, and with the daemon. A crash,
//! or a line that is no reply, answers the requests waiting with why, and
//! the next request starts it again; one that lets a deadline pass is taken
//! as hung and stopped the same way.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::process::{ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use muster_core::ticket_protocol::{self, Call, MOST_LINE, PROTOCOL, PluginError, Reply};
use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin};
use tokio::sync::{Semaphore, mpsc, oneshot};

use super::streams::{Lines, Log};
use super::{Environment, PluginSpec};
use crate::service::ServiceError;
use crate::tasks::{self, Owned};

/// The most requests waiting on one plugin at once.
pub(crate) const MOST_IN_FLIGHT: usize = 8;

/// How long a plugin told to stop has to end, before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(1);

/// How long a plugin may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Limits {
    /// To answer `hello`, once started.
    pub(crate) hello: Duration,
    /// To answer any other request.
    pub(crate) request: Duration,
    /// Unused, before it is stopped.
    pub(crate) idle: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            hello: Duration::from_secs(5),
            request: Duration::from_secs(30),
            idle: Duration::from_mins(5),
        }
    }
}

/// One plugin's process, when it runs, and its log.
#[derive(Debug)]
pub(crate) struct Process {
    spec: PluginSpec,
    limits: Limits,
    running: tokio::sync::Mutex<Option<Running>>,
    in_flight: Semaphore,
    log: Arc<Mutex<Log>>,
}

/// A process started, and the task that speaks to it, which owns it.
#[derive(Debug)]
struct Running {
    requests: mpsc::Sender<Outgoing>,
    stop: oneshot::Sender<End>,
    driver: Owned,
}

impl Running {
    /// Tells it to stop, and waits until it has. A driver that does not
    /// listen, stuck writing to a plugin that reads nothing, is ended as
    /// it is dropped, killing the plugin.
    async fn stop(self, end: End) {
        let _ = self.stop.send(end);
        let _ = tokio::time::timeout(STOP_GRACE * 2, self.driver).await;
    }
}

/// A request on its way to the plugin.
#[derive(Debug)]
struct Outgoing {
    call: Call,
    answer: oneshot::Sender<Answer>,
}

/// What became of a request.
#[derive(Debug)]
enum Answer {
    Replied(Result<Value, PluginError>),
    /// The plugin ended, why, before the request was written: its next
    /// process may answer it.
    Unsent(String),
    /// The plugin ended, why, with the request unanswered.
    Ended(String),
}

/// What asking came to.
enum Asked {
    Answered(Result<Value, ServiceError>),
    Unsent(String),
    TimedOut,
}

impl Process {
    /// The process of the plugin `spec` describes, keeping what it says on
    /// stderr in `log`, which hides its secrets.
    pub(crate) fn new(spec: PluginSpec, limits: Limits, log: Arc<Mutex<Log>>) -> Self {
        lock(&log).hide(spec.secrets());
        Self {
            spec,
            limits,
            running: tokio::sync::Mutex::new(None),
            in_flight: Semaphore::new(MOST_IN_FLIGHT),
            log,
        }
    }

    pub(crate) fn spec(&self) -> &PluginSpec {
        &self.spec
    }

    /// The end of what it said on stderr, across its restarts.
    #[cfg(test)]
    pub(crate) fn log(&self) -> String {
        lock(&self.log).text()
    }

    /// The plugin as messages name it.
    fn who(&self) -> String {
        format!("the {} plugin", self.spec.name)
    }

    /// What the plugin answers `call` with, starting it if it is not
    /// running.
    pub(crate) async fn call(&self, call: Call) -> Result<Value, ServiceError> {
        let _turn = self
            .in_flight
            .acquire()
            .await
            .map_err(|_| ServiceError::Unavailable(format!("{} was stopped", self.who())))?;
        let mut why = String::new();
        // A request the plugin ended before reading goes to its next
        // process, once.
        for _ in 0..2 {
            let requests = self.requests().await?;
            match self.ask(&requests, call.clone(), self.limits.request).await {
                Asked::Answered(outcome) => return outcome,
                Asked::Unsent(said) => why = said,
                Asked::TimedOut => {
                    self.stop_hung(&requests).await;
                    return Err(self.too_slow(self.limits.request));
                }
            }
        }
        Err(ServiceError::Unavailable(why))
    }

    /// Stops it, if it runs, waiting until it has: as the daemon stops.
    pub(crate) async fn stop(&self) {
        if let Some(running) = self.running.lock().await.take() {
            running.stop(End::Told).await;
        }
    }

    /// Where requests go: the process running, or one started and said
    /// `hello` to. Those asking meanwhile wait for it.
    async fn requests(&self) -> Result<mpsc::Sender<Outgoing>, ServiceError> {
        let mut running = self.running.lock().await;
        if let Some(alive) = running
            .as_ref()
            .filter(|running| !running.requests.is_closed())
        {
            return Ok(alive.requests.clone());
        }
        if let Some(ended) = running.take() {
            ended.stop(End::Told).await;
        }
        let started = self.start()?;
        let hello = Call::Hello {
            muster: env!("CARGO_PKG_VERSION").to_owned(),
            protocol: PROTOCOL,
        };
        let spoken = match self.ask(&started.requests, hello, self.limits.hello).await {
            Asked::Answered(Ok(value)) => ticket_protocol::hello(value)
                .map_err(|why| ServiceError::Unreadable(format!("{}: {why}", self.who()))),
            Asked::Answered(Err(error)) => Err(error),
            Asked::Unsent(why) => Err(ServiceError::Unavailable(why)),
            Asked::TimedOut => Err(self.too_slow(self.limits.hello)),
        };
        let refused = match spoken {
            Ok(PROTOCOL) => None,
            Ok(other) => Some(ServiceError::Unavailable(format!(
                "{} speaks version {other} of the ticket protocol; this Muster speaks {PROTOCOL}",
                self.who()
            ))),
            Err(error) => Some(error),
        };
        if let Some(error) = refused {
            started.stop(End::Hung).await;
            return Err(error);
        }
        tracing::info!(plugin = %self.spec.id, "a ticket plugin started");
        let requests = started.requests.clone();
        *running = Some(started);
        Ok(requests)
    }

    /// Starts the plugin's program, in its folder if it has one, with what
    /// of the environment it is given: Muster's own what sessions get
    /// (`harness::scrub`), the person's `PATH` among it, so their `gh` is
    /// found; anyone else's the basics of that and what it names.
    fn start(&self) -> Result<Running, ServiceError> {
        let mut command = tokio::process::Command::new(&self.spec.program);
        command
            .args(&self.spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(folder) = &self.spec.folder {
            command.current_dir(folder);
        }
        match &self.spec.environment {
            Environment::Sessions => {
                let keys = std::env::vars_os().filter_map(|(key, _)| key.into_string().ok());
                for key in crate::harness::scrub::scrubbed(keys) {
                    command.env_remove(key);
                }
            }
            Environment::Only { variables, secrets } => {
                command.env_clear();
                command.envs(super::basics(std::env::vars_os()));
                for (name, value) in variables.iter().chain(secrets) {
                    command.env(name, value.value());
                }
            }
        }
        let child = command.spawn().map_err(|error| {
            ServiceError::Unavailable(format!(
                "could not start {} ({}): {error}",
                self.who(),
                self.spec.program.display()
            ))
        })?;
        let (requests, queue) = mpsc::channel(MOST_IN_FLIGHT);
        let (stop, told) = oneshot::channel();
        let driver = tasks::owned(drive(
            child,
            queue,
            told,
            Driving {
                id: self.spec.id.clone(),
                who: self.who(),
                log: self.log.clone(),
                idle: self.limits.idle,
            },
        ));
        Ok(Running {
            requests,
            stop,
            driver,
        })
    }

    /// Sends `call` and waits for its answer, at most `limit`.
    async fn ask(&self, requests: &mpsc::Sender<Outgoing>, call: Call, limit: Duration) -> Asked {
        let (answer, answered) = oneshot::channel();
        let sent = async {
            if requests.send(Outgoing { call, answer }).await.is_err() {
                return None;
            }
            Some(answered.await)
        };
        match tokio::time::timeout(limit, sent).await {
            Err(_) => Asked::TimedOut,
            Ok(None) => Asked::Unsent(format!("{} stopped", self.who())),
            // Its driver was ended, the plugin killed, as another request
            // found it hung.
            Ok(Some(Err(_))) => Asked::Answered(Err(ServiceError::Unavailable(format!(
                "{} was stopped: it let a request's deadline pass",
                self.who()
            )))),
            Ok(Some(Ok(Answer::Replied(outcome)))) => {
                Asked::Answered(outcome.map_err(|error| super::refusal(&self.spec, error)))
            }
            Ok(Some(Ok(Answer::Unsent(why)))) => Asked::Unsent(why),
            Ok(Some(Ok(Answer::Ended(why)))) => {
                Asked::Answered(Err(ServiceError::Unavailable(why)))
            }
        }
    }

    /// Stops the process `requests` went to, if it still runs, as hung.
    async fn stop_hung(&self, requests: &mpsc::Sender<Outgoing>) {
        let mut running = self.running.lock().await;
        if running
            .as_ref()
            .is_some_and(|running| running.requests.same_channel(requests))
            && let Some(hung) = running.take()
        {
            tracing::warn!(plugin = %self.spec.id, "a ticket plugin let a request's deadline pass; stopped it");
            hung.stop(End::Hung).await;
        }
    }

    fn too_slow(&self, limit: Duration) -> ServiceError {
        ServiceError::Unavailable(format!(
            "{} did not answer within {} s",
            self.who(),
            limit.as_secs_f32()
        ))
    }
}

fn lock(log: &Mutex<Log>) -> std::sync::MutexGuard<'_, Log> {
    log.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Why a plugin's process ended.
#[derive(Debug)]
enum End {
    /// Told to: the daemon stops, or the process was found ended.
    Told,
    /// It let a request's deadline pass, or never said `hello` right.
    Hung,
    /// Unused for so long.
    Idle,
    /// Nothing can ask it any more.
    Dropped,
    /// It closed its stdout, or ended.
    Closed,
    /// It wrote a line that is no reply.
    Garbage(String),
    /// It wrote a line longer than a reply may be.
    TooLong,
    /// Its pipes failed.
    Io(std::io::Error),
}

impl End {
    /// Why, for those waiting on it, with the last thing it said where it
    /// ended on its own.
    fn message(&self, who: &str, status: Option<ExitStatus>, said: Option<String>) -> String {
        let mut message = match self {
            Self::Told | Self::Idle | Self::Dropped => return format!("{who} was stopped"),
            Self::Hung => return format!("{who} was stopped: it let a request's deadline pass"),
            Self::Closed => match status {
                Some(status) => format!("{who} ended ({status})"),
                None => format!("{who} closed its output"),
            },
            Self::Garbage(why) => {
                format!("{who} answered something that is not the ticket protocol: {why}")
            }
            Self::TooLong => format!("{who} answered more than {} MiB at once", MOST_LINE >> 20),
            Self::Io(error) => format!("lost touch with {who}: {error}"),
        };
        if let Some(said) = said {
            let _ = write!(message, "; it said: {said}");
        }
        message
    }
}

/// What the driver needs to know besides its pipes.
struct Driving {
    id: String,
    who: String,
    log: Arc<Mutex<Log>>,
    idle: Duration,
}

/// Speaks to the plugin until it ends or is told to stop: writes each
/// request with an id, hands each reply to whoever waits for it, and keeps
/// its stderr. Then answers whoever still waits, with why.
async fn drive(
    mut child: Child,
    mut requests: mpsc::Receiver<Outgoing>,
    mut told: oneshot::Receiver<End>,
    driving: Driving,
) {
    let (Some(mut stdin), Some(stdout), Some(mut stderr)) =
        (child.stdin.take(), child.stdout.take(), child.stderr.take())
    else {
        return;
    };
    let mut lines = Lines::new(stdout, MOST_LINE);
    let mut pending: HashMap<u64, oneshot::Sender<Answer>> = HashMap::new();
    let mut next_id: u64 = 1;
    let mut said = vec![0; 4096];
    let mut stderr_open = true;
    let unused = tokio::time::sleep(driving.idle);
    tokio::pin!(unused);
    let end = loop {
        tokio::select! {
            end = &mut told => break end.unwrap_or(End::Dropped),
            outgoing = requests.recv() => {
                let Some(Outgoing { call, answer }) = outgoing else {
                    break End::Dropped;
                };
                if let Err(error) = write(&mut stdin, next_id, &call).await {
                    let _ = answer.send(Answer::Unsent(format!("lost touch with {}: {error}", driving.who)));
                    break End::Io(error);
                }
                pending.insert(next_id, answer);
                next_id += 1;
                unused.as_mut().reset(tokio::time::Instant::now() + driving.idle);
            }
            line = lines.next() => match line {
                Ok(Some(line)) if line.trim_ascii().is_empty() => {}
                Ok(Some(line)) => match ticket_protocol::read_reply(&line) {
                    Ok(Reply { id: Some(id), outcome }) => {
                        if let Some(answer) = pending.remove(&id) {
                            let _ = answer.send(Answer::Replied(outcome));
                        }
                        unused.as_mut().reset(tokio::time::Instant::now() + driving.idle);
                    }
                    Ok(Reply { id: None, outcome }) => {
                        let why = outcome.err().map_or_else(String::new, |error| error.message);
                        break End::Garbage(format!("it could not read a request: {why}"));
                    }
                    Err(why) => break End::Garbage(why),
                },
                Ok(None) => break End::Closed,
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => break End::TooLong,
                Err(error) => break End::Io(error),
            },
            read = stderr.read(&mut said), if stderr_open => match read {
                Ok(0) | Err(_) => stderr_open = false,
                Ok(count) => lock(&driving.log).push(&said[..count]),
            },
            () = &mut unused, if pending.is_empty() => break End::Idle,
        }
    };
    requests.close();
    // Its stdin ends, so a plugin told to stop can finish on its own.
    drop(stdin);
    let status = if matches!(end, End::Hung) {
        None
    } else {
        tokio::time::timeout(STOP_GRACE, child.wait())
            .await
            .ok()
            .and_then(Result::ok)
    };
    if status.is_none() {
        let _ = child.kill().await;
    }
    if stderr_open {
        // The rest of what it said as it ended.
        let mut rest = Vec::new();
        let _ = tokio::time::timeout(
            Duration::from_millis(100),
            (&mut stderr)
                .take(super::streams::MOST_LOG as u64)
                .read_to_end(&mut rest),
        )
        .await;
        lock(&driving.log).push(&rest);
    }
    let on_its_own = matches!(
        end,
        End::Closed | End::Garbage(_) | End::TooLong | End::Io(_)
    );
    let said = on_its_own.then(|| lock(&driving.log).last_line()).flatten();
    let why = end.message(&driving.who, status, said);
    if on_its_own {
        tracing::warn!(plugin = %driving.id, %why, "a ticket plugin ended");
    } else {
        tracing::info!(plugin = %driving.id, ?end, "a ticket plugin stopped");
    }
    while let Ok(Outgoing { answer, .. }) = requests.try_recv() {
        let _ = answer.send(Answer::Unsent(why.clone()));
    }
    for (_, answer) in pending {
        let _ = answer.send(Answer::Ended(why.clone()));
    }
}

/// Writes request `id`, `call`, as a line.
async fn write(stdin: &mut ChildStdin, id: u64, call: &Call) -> std::io::Result<()> {
    stdin
        .write_all(ticket_protocol::request_line(id, call).as_bytes())
        .await?;
    stdin.flush().await
}
