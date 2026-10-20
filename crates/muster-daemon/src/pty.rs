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
//! questions programs ask their terminal (see `queries`), those about its
//! colours from the person's terminal's, as a UI last told them (see
//! `colours`).
//!
//! An actor can hand its terminal over, for the daemon's restart in place
//! (`handoff`): it stops its reader between reads, so no byte is taken from
//! the terminal that the ring does not hold, and gives up the terminal with
//! its ring, input modes and size; the program runs on, untouched. The next
//! daemon adopts it into a new actor.

mod colours;
mod cursor;
mod osc;
mod queries;
mod ring;

#[cfg(test)]
mod props;
#[cfg(test)]
mod tests;

use std::fs::File;
use std::io::{PipeReader, PipeWriter, Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use muster_core::colours::TerminalColours;
use muster_core::protocol::{PixelSize, TermSize, TerminalModes};
use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, oneshot, watch};
use tokio::time::Instant;

use self::cursor::CursorTracker;
use self::osc::{OscScanner, Report};
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
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PtyEvent {
    /// The progress sequence turned busy or idle.
    Progress { busy: bool },
    /// The program set its window title, as it may to say how it is doing.
    Title(String),
    /// The process exited and all its output has been read; `end` is the
    /// position after its last byte and `size` the terminal's last size.
    Exited {
        success: bool,
        requested: bool,
        end: u64,
        size: TermSize,
    },
}

/// A piece of output, its position, and the input modes in effect after it.
#[derive(Clone, Debug)]
pub(crate) struct Chunk {
    pub(crate) seq: u64,
    pub(crate) data: Arc<[u8]>,
    pub(crate) modes: TerminalModes,
}

/// What an attaching client receives: the replay, then live output until the
/// process exits. `live` is `None` once it has.
#[derive(Debug)]
pub(crate) struct Attachment {
    pub(crate) replay: Replay,
    /// The input modes in effect at the end of the replay.
    pub(crate) modes: TerminalModes,
    pub(crate) live: Option<broadcast::Receiver<Chunk>>,
}

/// A running terminal as carried to the daemon's next version.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Carried {
    /// The program's process id, which leads its process group.
    pub(crate) pid: Option<u32>,
    /// The sequence number of the ring's first byte.
    pub(crate) seq: u64,
    #[serde(with = "serde_bytes")]
    pub(crate) ring: Vec<u8>,
    pub(crate) size: TermSize,
    pub(crate) modes: queries::CarriedModes,
    /// It was asked to stop and is within its grace.
    pub(crate) stopping: bool,
}

/// A terminal handed over: the terminal itself and what goes with it.
#[derive(Debug)]
pub(crate) struct Handed {
    pub(crate) master: OwnedFd,
    pub(crate) terminal: Carried,
}

#[derive(Debug)]
enum Command {
    Attach {
        from_seq: Option<u64>,
        reply: oneshot::Sender<Attachment>,
    },
    Input(Vec<u8>),
    Resize(TermSize, Option<PixelSize>),
    /// The person's keys came into the terminal, or left it.
    Focus(bool),
    Stop,
    ResumeIfSuspended,
    /// Give the terminal up, with what goes with it, if its program still
    /// runs; the actor ends, leaving the program running.
    HandOver(oneshot::Sender<Option<Handed>>),
    /// The process group in the terminal's foreground, which has the keys:
    /// a shell's, or the program it runs now.
    Foreground(oneshot::Sender<Option<u32>>),
}

/// A cheap, cloneable way to reach a terminal's actor.
#[derive(Clone, Debug)]
pub(crate) struct PtyHandle {
    commands: mpsc::Sender<Command>,
    pid: Option<u32>,
}

impl PtyHandle {
    /// The program's process id, the root of what it uses.
    pub(crate) fn pid(&self) -> Option<u32> {
        self.pid
    }

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

    /// Sizes the terminal, in pixels too where a client knows them, for
    /// programs that size pictures or a cursor by them.
    pub(crate) async fn resize(&self, size: TermSize, pixels: Option<PixelSize>) -> bool {
        self.commands
            .send(Command::Resize(size, pixels))
            .await
            .is_ok()
    }

    /// Says whether the person's keys are in the terminal: a program that
    /// asked to be told (`CSI ? 1004 h`) is sent `CSI I` as they come and
    /// `CSI O` as they go.
    pub(crate) async fn focus(&self, focused: bool) {
        let _ = self.commands.send(Command::Focus(focused)).await;
    }

    /// Resizes without waiting, for a terminal just started, whose queue
    /// has room; `false` if it had none or the terminal is gone.
    pub(crate) fn resize_now(&self, size: TermSize, pixels: Option<PixelSize>) -> bool {
        self.commands
            .try_send(Command::Resize(size, pixels))
            .is_ok()
    }

    /// Hangs up the terminal, then kills its process group if it is still
    /// running after a grace period.
    pub(crate) async fn stop(&self) {
        let _ = self.commands.send(Command::Stop).await;
    }

    /// Continues a program that suspended itself. Claude Code takes Ctrl+Z
    /// as "suspend": it hands the terminal back (bracketed paste and its
    /// keyboard modes off) and waits for SIGCONT. In a terminal of its own
    /// with no shell above it the system ignores the stop, so it never truly
    /// stops, and nothing would ever continue it. Sent only when paste mode
    /// went off while the program kept running; a continue it did not need
    /// only makes it redraw.
    pub(crate) async fn resume_if_suspended(&self) {
        let _ = self.commands.send(Command::ResumeIfSuspended).await;
    }

    /// The process group in the terminal's foreground: a shell's while it
    /// waits at its prompt, the program it runs while one runs; `None` once
    /// it has ended, or the terminal is gone.
    pub(crate) async fn foreground(&self) -> Option<u32> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::Foreground(reply)).await.ok()?;
        answer.await.ok().flatten()
    }

    /// Hands the terminal over, for the daemon's next version: `None` if
    /// its program has ended, or the terminal is gone.
    pub(crate) async fn hand_over(&self) -> Option<Handed> {
        let (reply, answer) = oneshot::channel();
        self.commands.send(Command::HandOver(reply)).await.ok()?;
        answer.await.ok().flatten()
    }
}

/// The colours of the person's terminal, as a UI last told them, which a
/// program asking its terminal for its colours is told.
pub(crate) type Colours = watch::Receiver<TerminalColours>;

/// Starts `spec` in a new terminal whose first byte of output is numbered
/// `first_seq`. `events` receives what it reports; the terminal is released
/// when every handle to it has been dropped.
pub(crate) fn spawn(
    spec: &SpawnSpec,
    first_seq: u64,
    colours: Colours,
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

    let pid = child.process_id();
    // The terminal as a descriptor of Muster's own, which a handoff can
    // carry; portable-pty's copy closes with it.
    let master = pair
        .master
        .as_raw_fd()
        .ok_or_else(|| std::io::Error::other("the terminal has no descriptor"))
        .and_then(crate::handoff::fds::duplicate)?;
    drop(pair.master);
    let (exit, exited) = oneshot::channel();
    std::thread::Builder::new()
        .name("muster-pty-wait".to_owned())
        .spawn(move || {
            let success = child.wait().is_ok_and(|status| status.success());
            let _ = exit.send(success);
        })?;
    start(
        Starting {
            master,
            pid,
            exited,
            ring: Ring::starting_at(RING_CAPACITY, first_seq),
            queries: QueryScanner::default(),
            colours,
            size: spec.size,
            stopping: false,
        },
        events,
    )
}

/// Takes over a terminal handed over by the daemon's previous version, its
/// program still running and still this process's child.
pub(crate) fn adopt(
    master: OwnedFd,
    carried: Carried,
    colours: Colours,
    events: impl Fn(PtyEvent) + Send + 'static,
) -> std::io::Result<PtyHandle> {
    let mut ring = Ring::starting_at(RING_CAPACITY, carried.seq);
    ring.push(&carried.ring);
    let pid = carried.pid;
    let (exit, exited) = oneshot::channel();
    std::thread::Builder::new()
        .name("muster-pty-wait".to_owned())
        .spawn(move || {
            let _ = exit.send(wait_for(pid));
        })?;
    start(
        Starting {
            master,
            pid,
            exited,
            ring,
            queries: QueryScanner::adopt(carried.modes),
            colours,
            size: carried.size,
            stopping: carried.stopping,
        },
        events,
    )
}

/// Waits for the child `pid` to exit: whether it exited cleanly.
fn wait_for(pid: Option<u32>) -> bool {
    use nix::errno::Errno;
    use nix::sys::wait::{WaitStatus, waitpid};
    let Some(pid) = pid.and_then(|pid| i32::try_from(pid).ok()) else {
        return false;
    };
    loop {
        match waitpid(Pid::from_raw(pid), None) {
            Ok(WaitStatus::Exited(_, code)) => return code == 0,
            Ok(WaitStatus::Signaled(..)) => return false,
            Err(Errno::EINTR) | Ok(_) => {}
            // Reaped already, as it was exiting during the handoff.
            Err(_) => return false,
        }
    }
}

/// A terminal about to get its actor: new, or taken over from the
/// daemon's previous version.
struct Starting {
    master: OwnedFd,
    pid: Option<u32>,
    /// Whether its program exited cleanly, once it has.
    exited: oneshot::Receiver<bool>,
    ring: Ring,
    queries: QueryScanner,
    colours: Colours,
    size: TermSize,
    /// Its program was being stopped when it was handed over.
    stopping: bool,
}

/// Starts the actor for a terminal and its threads.
fn start(
    Starting {
        master,
        pid,
        exited,
        ring,
        queries,
        colours,
        size,
        stopping,
    }: Starting,
    events: impl Fn(PtyEvent) + Send + 'static,
) -> std::io::Result<PtyHandle> {
    let process_group = pid
        .and_then(|pid| i32::try_from(pid).ok())
        .map(Pid::from_raw);
    let reader = File::from(master.try_clone()?);
    let writer = File::from(master.try_clone()?);

    let (output, output_queue) = mpsc::channel(64);
    let (stop, stop_reading) = std::io::pipe()?;
    let (read_done, reader_done) = oneshot::channel();
    std::thread::Builder::new()
        .name("muster-pty-read".to_owned())
        .spawn(move || {
            read_output(reader, &output, &stop);
            let _ = read_done.send(());
        })?;

    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: what the person types and pastes; the writer must not drop keys, and the actor must not wait on it"
    )]
    let (input, input_queue) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::Builder::new()
        .name("muster-pty-write".to_owned())
        .spawn(move || write_input(writer, &input_queue))?;

    let (commands, queue) = mpsc::channel(256);
    let actor = Actor {
        master: Some(master),
        process_group,
        ring,
        live: Some(broadcast::channel(256).0),
        input: Some(input),
        osc: OscScanner::default(),
        queries,
        colours,
        cursor: None,
        focused: None,
        size,
        stopping,
        kill_at: stopping.then(|| Instant::now() + STOP_GRACE),
        stop_reading: Some(stop_reading),
        reader_done: Some(reader_done),
    };
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-13: the actor runs until every PtyHandle to it is dropped or it hands its terminal over"
    )]
    tokio::spawn(actor.run(queue, output_queue, exited, events));
    Ok(PtyHandle { commands, pid })
}

/// The terminal's size as the system keeps it; pixels 0 where unknown, as
/// before any client said them.
fn winsize(size: TermSize, pixels: Option<PixelSize>) -> rustix::termios::Winsize {
    rustix::termios::Winsize {
        ws_row: size.rows.max(1),
        ws_col: size.cols.max(1),
        ws_xpixel: pixels.map_or(0, |pixels| pixels.width),
        ws_ypixel: pixels.map_or(0, |pixels| pixels.height),
    }
}

fn pty_size(size: TermSize) -> PtySize {
    PtySize {
        rows: size.rows.max(1),
        cols: size.cols.max(1),
        pixel_width: 0,
        pixel_height: 0,
    }
}

/// Reads output until the terminal closes, or until `stop` can be read,
/// which its writer's end being closed makes so: it is looked at between
/// reads, so a byte read is always sent on, and a handoff loses none. The
/// reader sleeps until one of them wakes it, never on a timer.
fn read_output(mut reader: File, output: &mpsc::Sender<Vec<u8>>, stop: &PipeReader) {
    let mut buffer = vec![0; 16 * 1024];
    loop {
        match readable(&reader, stop) {
            Ok(Ready::Output) => {}
            Ok(Ready::Stop) => return,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return,
        }
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

/// What woke the reader.
#[derive(Debug, PartialEq, Eq)]
enum Ready {
    Output,
    Stop,
}

/// Waits until the terminal has output or the reader is to stop, which
/// comes first when both are so. With `select`, as macOS's `poll` does not
/// work on terminals.
fn readable(reader: &File, stop: &PipeReader) -> nix::Result<Ready> {
    use nix::sys::select::{FdSet, select};
    let mut fds = FdSet::new();
    fds.insert(reader.as_fd());
    fds.insert(stop.as_fd());
    select(None, &mut fds, None, None, None)?;
    if fds.contains(stop.as_fd()) {
        Ok(Ready::Stop)
    } else {
        Ok(Ready::Output)
    }
}

fn write_input(mut writer: File, input: &std::sync::mpsc::Receiver<Vec<u8>>) {
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
    /// The terminal; taken by a handoff.
    master: Option<OwnedFd>,
    process_group: Option<Pid>,
    ring: Ring,
    /// Dropped once the process has exited, which ends live streams.
    live: Option<broadcast::Sender<Chunk>>,
    input: Option<std::sync::mpsc::Sender<Vec<u8>>>,
    osc: OscScanner,
    queries: QueryScanner,
    /// The person's terminal's colours, for questions about its own.
    colours: Colours,
    /// A screen for cursor-position answers, built at the first question.
    cursor: Option<CursorTracker>,
    /// Whether the person's keys are in the terminal, once told.
    focused: Option<bool>,
    size: TermSize,
    stopping: bool,
    kill_at: Option<Instant>,
    /// Closed to stop the reader, for a handoff; it also stops once the
    /// actor is gone.
    stop_reading: Option<PipeWriter>,
    /// Told when the reader has stopped.
    reader_done: Option<oneshot::Receiver<()>>,
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
                    Some(Command::HandOver(reply)) => {
                        let handed = if exit_status.is_none() && self.live.is_some() {
                            for chunk in self.stop_reader(&mut output).await {
                                self.on_output(chunk, &mut output, &events);
                            }
                            // What the reader read before it stopped.
                            while let Ok(chunk) = output.try_recv() {
                                self.on_output(chunk, &mut output, &events);
                            }
                            self.give_up()
                        } else {
                            None
                        };
                        let gone = handed.is_some();
                        let _ = reply.send(handed);
                        if gone {
                            // The program runs on, the next daemon's now.
                            return;
                        }
                    }
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
                    end: self.ring.end(),
                    size: self.size,
                });
            }
        }
        if self.live.is_some() {
            // Released while still running.
            self.signal(Signal::SIGKILL);
        }
    }

    /// Stops the reader between reads, for a handoff; returns what it read
    /// meanwhile. Its output is taken as it waits: a reader waiting on a
    /// full queue would otherwise never look to see it should stop, and the
    /// handoff would wait for ever. Once its last send goes through it
    /// stops, so what is taken is at most a queue's worth.
    async fn stop_reader(&mut self, output: &mut mpsc::Receiver<Vec<u8>>) -> Vec<Vec<u8>> {
        drop(self.stop_reading.take());
        let mut taken = Vec::new();
        let Some(mut done) = self.reader_done.take() else {
            return taken;
        };
        loop {
            tokio::select! {
                _ = &mut done => return taken,
                chunk = output.recv() => match chunk {
                    Some(chunk) => taken.push(chunk),
                    // The reader has gone, so it has stopped.
                    None => return taken,
                },
            }
        }
    }

    /// Gives up the terminal with its ring, modes and size.
    fn give_up(&mut self) -> Option<Handed> {
        let master = self.master.take()?;
        let replay = self.ring.replay_from(None);
        Some(Handed {
            master,
            terminal: Carried {
                pid: self
                    .process_group
                    .and_then(|group| u32::try_from(group.as_raw()).ok()),
                seq: replay.seq,
                ring: replay.data,
                size: self.size,
                modes: self.queries.carry(),
                stopping: self.stopping,
            },
        })
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
        // Every change is reported, in order: a harness's titles are steps
        // ("Working", then "Action Required"), not only a latest state.
        for report in self.osc.scan(&data) {
            events(match report {
                Report::Progress { busy } => PtyEvent::Progress { busy },
                Report::Title(title) => PtyEvent::Title(title),
            });
        }
        self.answer_queries(&data);
        let seq = self.ring.push(&data);
        if let Some(live) = &self.live {
            // No subscribers is fine; the ring holds it.
            let _ = live.send(Chunk {
                seq,
                data: data.into(),
                modes: self.queries.modes(),
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
                Reply::Colour { colour, ending } => {
                    // One no UI has said goes unanswered, as a terminal
                    // that does not know it would.
                    let known = colours::answer(colour, ending, &self.colours.borrow());
                    let Some(bytes) = known else {
                        continue;
                    };
                    bytes
                }
                Reply::PixelSize { cell } => {
                    let Some(bytes) = self.pixel_size(cell) else {
                        continue;
                    };
                    bytes
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

    /// The answer to a question about the terminal's size in pixels, or a
    /// cell's (`CSI 4 ; height ; width t`, `CSI 6 ; …`), from the size the
    /// system keeps, which a handoff carries too; none where no client
    /// gave pixels.
    fn pixel_size(&self, cell: bool) -> Option<Vec<u8>> {
        let size = rustix::termios::tcgetwinsize(self.master.as_ref()?).ok()?;
        let (width, height) = (size.ws_xpixel, size.ws_ypixel);
        if width == 0 || height == 0 {
            return None;
        }
        let answer = if cell {
            let (cell_width, cell_height) =
                (width / size.ws_col.max(1), height / size.ws_row.max(1));
            format!("\x1b[6;{cell_height};{cell_width}t")
        } else {
            format!("\x1b[4;{height};{width}t")
        };
        Some(answer.into_bytes())
    }

    fn on_command(&mut self, command: Command) {
        match command {
            Command::Attach { from_seq, reply } => {
                let _ = reply.send(Attachment {
                    replay: self.ring.replay_from(from_seq),
                    modes: self.queries.modes(),
                    live: self.live.as_ref().map(broadcast::Sender::subscribe),
                });
            }
            Command::Input(data) => {
                if let Some(input) = &self.input {
                    let _ = input.send(data);
                }
            }
            Command::Resize(size, pixels) => {
                if self.live.is_some() {
                    if let Some(master) = &self.master {
                        let _ = rustix::termios::tcsetwinsize(master, winsize(size, pixels));
                    }
                    self.size = size;
                    if let Some(tracker) = &mut self.cursor {
                        tracker.resize(size.cols, size.rows);
                    }
                }
            }
            Command::Focus(focused) => {
                // Told only as it changes, and only to a program that
                // asked, as a terminal does.
                if self.focused != Some(focused) {
                    self.focused = Some(focused);
                    if let (true, Some(input)) = (self.queries.focus_events(), &self.input) {
                        let report = if focused { b"\x1b[I" } else { b"\x1b[O" };
                        let _ = input.send(report.to_vec());
                    }
                }
            }
            Command::ResumeIfSuspended => {
                let suspended = self.queries.paste_used() && !self.queries.modes().bracketed_paste;
                if self.live.is_some() && !self.stopping && suspended {
                    self.signal(Signal::SIGCONT);
                }
            }
            // Handled in `run`, where it can wait for the reader.
            Command::HandOver(_) => {}
            Command::Foreground(reply) => {
                let group = self
                    .master
                    .as_ref()
                    .filter(|_| self.live.is_some())
                    .and_then(|master| rustix::termios::tcgetpgrp(master).ok())
                    .and_then(|group| u32::try_from(group.as_raw_nonzero().get()).ok());
                let _ = reply.send(group);
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
