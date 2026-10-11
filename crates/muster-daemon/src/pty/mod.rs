//! Hands-on sessions' terminals (`ARCHITECTURE.md` §5).
//!
//! Each session's terminal is owned by one task, its actor. A reader thread
//! feeds it output, which it appends to the ring and broadcasts in one step,
//! so an attaching client gets the replay and the live stream with no gap and
//! no overlap. Keystrokes go straight to the actor, never through the
//! Supervisor. The actor outlives the process, so the last output of an
//! exited session can still be read, until the Supervisor releases it.
//!
//! The child is started by portable-pty, which makes it a session leader with
//! the terminal as its controlling terminal and closes every inherited
//! descriptor above stderr before `exec`. The actor also answers the
//! questions programs ask their terminal (see `queries`).

mod cursor;
mod progress;
mod queries;
mod ring;

#[cfg(test)]
mod tests;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use muster_core::protocol::TermSize;
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Instant;

use self::cursor::CursorTracker;
use self::progress::ProgressScanner;
use self::queries::{QueryScanner, Reply};
use self::ring::{RING_CAPACITY, Replay, Ring};

/// After asking a session to stop, how long before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(3);
/// After the process exits, how long to keep reading output. A background
/// job it started can hold the terminal open indefinitely.
const DRAIN_GRACE: Duration = Duration::from_secs(2);
/// The most output sent as one chunk.
const MAX_CHUNK: usize = 8 * 1024;

/// What to run in a terminal.
#[derive(Clone, Debug)]
pub(crate) struct SpawnSpec {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    /// Variables to set, over the daemon's own environment.
    pub(crate) env: Vec<(String, String)>,
    /// Variables of the daemon's environment the child must not see.
    pub(crate) env_remove: Vec<String>,
    pub(crate) size: TermSize,
}

/// What a terminal reports to whoever started it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PtyEvent {
    /// The progress sequence turned busy or idle.
    Progress { busy: bool },
    /// The process exited and all its output has been read.
    Exited { success: bool, requested: bool },
}

/// A piece of output and its position.
#[derive(Clone, Debug)]
pub(crate) struct Chunk {
    pub(crate) seq: u64,
    pub(crate) data: Arc<[u8]>,
}

/// What an attaching client receives: the replay, then live output until the
/// process exits. `live` is `None` once it has.
#[derive(Debug)]
pub(crate) struct Attachment {
    pub(crate) replay: Replay,
    pub(crate) live: Option<broadcast::Receiver<Chunk>>,
}

#[derive(Debug)]
enum Command {
    Attach {
        from_seq: Option<u64>,
        reply: oneshot::Sender<Attachment>,
    },
    Input(Vec<u8>),
    Resize(TermSize),
    Stop,
}

/// A cheap, cloneable way to reach a terminal's actor.
#[derive(Clone, Debug)]
pub(crate) struct PtyHandle {
    commands: mpsc::Sender<Command>,
}

impl PtyHandle {
    /// The replay from `from_seq` and the live stream, or `None` once the
    /// terminal has been released.
    pub(crate) async fn attach(&self, from_seq: Option<u64>) -> Option<Attachment> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(Command::Attach { from_seq, reply })
            .await
            .ok()?;
        answer.await.ok()
    }

    /// Sends keystrokes. `false` means the terminal has been released.
    pub(crate) async fn input(&self, data: Vec<u8>) -> bool {
        self.commands.send(Command::Input(data)).await.is_ok()
    }

    pub(crate) async fn resize(&self, size: TermSize) -> bool {
        self.commands.send(Command::Resize(size)).await.is_ok()
    }

    /// Hangs up the terminal, then kills its process group if it is still
    /// running after a grace period.
    pub(crate) async fn stop(&self) {
        let _ = self.commands.send(Command::Stop).await;
    }
}

/// Starts `spec` in a new terminal. `events` receives what it reports; the
/// terminal is released when every handle to it has been dropped.
pub(crate) fn spawn(
    spec: SpawnSpec,
    events: impl Fn(PtyEvent) + Send + 'static,
) -> std::io::Result<PtyHandle> {
    let pair = native_pty_system()
        .openpty(pty_size(spec.size))
        .map_err(std::io::Error::other)?;
    let mut command = CommandBuilder::new(&spec.program);
    command.args(&spec.args);
    command.cwd(&spec.cwd);
    for key in &spec.env_remove {
        command.env_remove(key);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(std::io::Error::other)?;
    // The child holds the terminal's other end; keeping a copy would stop
    // the reader from seeing end-of-file when the child exits.
    drop(pair.slave);

    let process_group = child
        .process_id()
        .and_then(|pid| i32::try_from(pid).ok())
        .map(Pid::from_raw);
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(std::io::Error::other)?;
    let writer = pair.master.take_writer().map_err(std::io::Error::other)?;

    let (output, output_queue) = mpsc::channel(64);
    std::thread::Builder::new()
        .name("muster-pty-read".to_owned())
        .spawn(move || read_output(reader, &output))?;

    let (input, input_queue) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name("muster-pty-write".to_owned())
        .spawn(move || write_input(writer, &input_queue))?;

    let (exit, exited) = oneshot::channel();
    std::thread::Builder::new()
        .name("muster-pty-wait".to_owned())
        .spawn(move || {
            let success = child.wait().is_ok_and(|status| status.success());
            let _ = exit.send(success);
        })?;

    let (commands, queue) = mpsc::channel(256);
    let actor = Actor {
        master: pair.master,
        process_group,
        ring: Ring::new(RING_CAPACITY),
        live: Some(broadcast::channel(256).0),
        input: Some(input),
        progress: ProgressScanner::default(),
        queries: QueryScanner::default(),
        cursor: None,
        size: spec.size,
        stopping: false,
        kill_at: None,
    };
    tokio::spawn(actor.run(queue, output_queue, exited, events));
    Ok(PtyHandle { commands })
}

fn pty_size(size: TermSize) -> PtySize {
    PtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn read_output(mut reader: Box<dyn Read + Send>, output: &mpsc::Sender<Vec<u8>>) {
    let mut buffer = vec![0; 16 * 1024];
    loop {
        match reader.read(&mut buffer) {
            // macOS reports a closed terminal as an error rather than EOF.
            Ok(0) | Err(_) => return,
            Ok(read) => {
                if output.blocking_send(buffer[..read].to_vec()).is_err() {
                    return;
                }
            }
        }
    }
}

fn write_input(mut writer: Box<dyn Write + Send>, input: &std::sync::mpsc::Receiver<Vec<u8>>) {
    while let Ok(data) = input.recv() {
        if writer
            .write_all(&data)
            .and_then(|()| writer.flush())
            .is_err()
        {
            return;
        }
    }
}

struct Actor {
    master: Box<dyn MasterPty + Send>,
    process_group: Option<Pid>,
    ring: Ring,
    /// Dropped once the process has exited, which ends live streams.
    live: Option<broadcast::Sender<Chunk>>,
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    progress: ProgressScanner,
    queries: QueryScanner,
    /// A screen for cursor-position answers, built at the first question.
    cursor: Option<CursorTracker>,
    size: TermSize,
    stopping: bool,
    kill_at: Option<Instant>,
}

impl Actor {
    async fn run(
        mut self,
        mut commands: mpsc::Receiver<Command>,
        mut output: mpsc::Receiver<Vec<u8>>,
        mut exited: oneshot::Receiver<bool>,
        events: impl Fn(PtyEvent),
    ) {
        let (mut output_open, mut exit_status) = (true, None);
        let mut drain_until: Option<Instant> = None;
        loop {
            let kill_at = self.kill_at;
            tokio::select! {
                chunk = output.recv(), if output_open => match chunk {
                    Some(first) => self.on_output(first, &mut output, &events),
                    None => output_open = false,
                },
                status = &mut exited, if exit_status.is_none() => {
                    exit_status = Some(status.unwrap_or(false));
                    drain_until = Some(Instant::now() + DRAIN_GRACE);
                }
                () = tokio::time::sleep_until(drain_until.unwrap_or_else(Instant::now)), if output_open && drain_until.is_some() => {
                    // Keep what already arrived, then stop reading.
                    output.close();
                    while let Ok(chunk) = output.try_recv() {
                        self.on_output(chunk, &mut output, &events);
                    }
                    output_open = false;
                }
                command = commands.recv() => match command {
                    Some(command) => self.on_command(command),
                    // Released: nobody can attach any more.
                    None => break,
                },
                () = tokio::time::sleep_until(kill_at.unwrap_or_else(Instant::now)), if kill_at.is_some() => {
                    self.signal(Signal::SIGKILL);
                    self.kill_at = None;
                }
            }
            // Announce the exit only once every byte is in the ring.
            if let (Some(success), false, Some(_)) = (exit_status, output_open, &self.live) {
                self.live = None;
                self.input = None;
                self.kill_at = None;
                events(PtyEvent::Exited {
                    success,
                    requested: self.stopping,
                });
            }
        }
        if self.live.is_some() {
            // Released while still running.
            self.signal(Signal::SIGKILL);
        }
    }

    fn on_output(
        &mut self,
        first: Vec<u8>,
        output: &mut mpsc::Receiver<Vec<u8>>,
        events: &impl Fn(PtyEvent),
    ) {
        let mut data = first;
        while data.len() < MAX_CHUNK {
            match output.try_recv() {
                Ok(more) => data.extend_from_slice(&more),
                Err(_) => break,
            }
        }
        for busy in self.progress.scan(&data) {
            events(PtyEvent::Progress { busy });
        }
        self.answer_queries(&data);
        let seq = self.ring.push(&data);
        if let Some(live) = &self.live {
            // No subscribers is fine; the ring holds it.
            let _ = live.send(Chunk {
                seq,
                data: data.into(),
            });
        }
    }

    /// Answers what `data` asks the terminal, in the order it asked. Runs
    /// before `data` joins the ring, so a new cursor screen is built from
    /// the output before it.
    fn answer_queries(&mut self, data: &[u8]) {
        let mut fed = 0;
        for reply in self.queries.scan(data).replies {
            let bytes = match reply {
                Reply::Bytes(bytes) => bytes,
                Reply::CursorPosition { at } => {
                    let (size, ring) = (self.size, &self.ring);
                    let tracker = self.cursor.get_or_insert_with(|| {
                        CursorTracker::new(size.cols, size.rows, &ring.replay_from(None).data)
                    });
                    tracker.feed(&data[fed..at]);
                    fed = at;
                    tracker.report()
                }
            };
            if let Some(input) = &self.input {
                let _ = input.send(bytes);
            }
        }
        if let Some(tracker) = &mut self.cursor {
            tracker.feed(&data[fed..]);
        }
    }

    fn on_command(&mut self, command: Command) {
        match command {
            Command::Attach { from_seq, reply } => {
                let _ = reply.send(Attachment {
                    replay: self.ring.replay_from(from_seq),
                    live: self.live.as_ref().map(broadcast::Sender::subscribe),
                });
            }
            Command::Input(data) => {
                if let Some(input) = &self.input {
                    let _ = input.send(data);
                }
            }
            Command::Resize(size) => {
                if self.live.is_some() {
                    let _ = self.master.resize(pty_size(size));
                    self.size = size;
                    if let Some(tracker) = &mut self.cursor {
                        tracker.resize(size.cols, size.rows);
                    }
                }
            }
            Command::Stop => {
                if self.live.is_some() && !self.stopping {
                    self.stopping = true;
                    self.signal(Signal::SIGHUP);
                    self.kill_at = Some(Instant::now() + STOP_GRACE);
                }
            }
        }
    }

    fn signal(&self, signal: Signal) {
        if let Some(group) = self.process_group {
            let _ = killpg(group, signal);
        }
    }
}

impl std::fmt::Debug for Actor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Actor")
            .field("process_group", &self.process_group)
            .field("end", &self.ring.end())
            .field("stopping", &self.stopping)
            .finish_non_exhaustive()
    }
}
