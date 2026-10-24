//! A client's connection to the daemon.

use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::time::Duration;

use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, Command, ErrorBody, FIRST_PROTOCOL_VERSION, Hello, Incompatible,
    MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, Reply, ServerFrame, Snapshot, TermSize, Welcome,
};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::UnixStream;

use crate::transport::{FramedRead, FramedWrite, TransportError};

/// Why talking to the daemon failed.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("could not reach the daemon at {path}: {source}")]
    Unreachable { path: String, source: io::Error },
    #[error(transparent)]
    Transport(#[from] TransportError),
    #[error(
        "the running daemon (muster {}, protocol {}..={}) is from another build than this muster \
         (protocol {}..={}); stop it with `muster daemon stop` so this build's daemon can take over",
        .0.daemon_version, .0.min_version, .0.max_version, MIN_PROTOCOL_VERSION, PROTOCOL_VERSION
    )]
    Incompatible(Incompatible),
    #[error("the daemon refused: {0}")]
    Daemon(ErrorBody),
    #[error("the daemon closed the connection")]
    Closed,
    #[error("the daemon sent something unexpected: {0}")]
    Unexpected(String),
    /// The program carrying the connection (`ssh` to another machine)
    /// could not, said in its words.
    #[error("{0}")]
    Carrier(String),
    /// A frame the daemon's protocol does not have (`ClientFrame::since`):
    /// it was never sent.
    #[error(
        "the running daemon (protocol {daemon}) is older than this asks for (protocol {needs}): \
         `muster reload` moves it onto the muster installed now"
    )]
    DaemonTooOld { needs: u32, daemon: u32 },
    /// A daemon that works answers a hello, and sends its state, at once:
    /// one that does neither within [`ANSWER_WITHIN`] is stuck.
    #[error("the daemon did not {0} within {secs} s; it may be stuck (`muster daemon stop` ends it)", secs = ANSWER_WITHIN.as_secs())]
    Silent(&'static str),
}

/// How long a daemon has to answer a hello, or to send its state: long
/// enough for one reached over `ssh` on a slow link.
pub const ANSWER_WITHIN: Duration = Duration::from_secs(30);

/// `step`, or [`ClientError::Silent`] saying what the daemon did not do if
/// it takes longer than [`ANSWER_WITHIN`].
async fn within<T>(
    what: &'static str,
    step: impl Future<Output = Result<T, ClientError>>,
) -> Result<T, ClientError> {
    tokio::time::timeout(ANSWER_WITHIN, step)
        .await
        .unwrap_or(Err(ClientError::Silent(what)))
}

/// The stream a connection reads the daemon from: its socket, or a
/// program carrying it, such as `ssh` to another machine.
type Reader = Box<dyn AsyncRead + Send + Unpin>;
/// The stream a connection writes to the daemon.
type Writer = Box<dyn AsyncWrite + Send + Unpin>;
/// What a connection keeps alive as long as it is open: the program
/// carrying it, where one does.
type Held = Box<dyn std::any::Any + Send>;

/// An open, greeted connection. Requests are answered in turn; frames the
/// daemon pushes meanwhile (state changes and terminal output) wait for
/// [`next_event`](Self::next_event).
pub struct Connection {
    reader: FramedRead<Reader>,
    writer: FramedWrite<Writer>,
    welcome: Welcome,
    pending: VecDeque<ServerFrame>,
    next_req_id: u64,
    held: Option<Held>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("welcome", &self.welcome)
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl Connection {
    /// Connects to the daemon's socket and says hello as `client`.
    pub async fn connect(socket: &Path, client: &str) -> Result<Self, ClientError> {
        let stream =
            UnixStream::connect(socket)
                .await
                .map_err(|source| ClientError::Unreachable {
                    path: socket.display().to_string(),
                    source,
                })?;
        let (read, write) = stream.into_split();
        Self::over(read, write, client).await
    }

    /// Says hello as `client` over any stream that carries the daemon's
    /// frames both ways, such as a program's standard input and output.
    pub async fn over(
        reader: impl AsyncRead + Send + Unpin + 'static,
        writer: impl AsyncWrite + Send + Unpin + 'static,
        client: &str,
    ) -> Result<Self, ClientError> {
        let reader: Reader = Box::new(reader);
        let writer: Writer = Box::new(writer);
        let (mut reader, mut writer) = (FramedRead::new(reader), FramedWrite::new(writer));
        let welcome = within("answer its hello", async {
            writer
                .send(&ClientFrame::Hello(Hello::current(client)))
                .await?;
            match reader.next::<ServerFrame>().await? {
                Some(ServerFrame::Welcome(welcome)) => Ok(welcome),
                Some(ServerFrame::Incompatible(incompatible)) => {
                    Err(ClientError::Incompatible(incompatible))
                }
                Some(ServerFrame::Error(error)) => Err(ClientError::Daemon(error)),
                Some(other) => Err(ClientError::Unexpected(format!("{other:?}"))),
                None => Err(ClientError::Closed),
            }
        })
        .await?;
        Ok(Self {
            reader,
            writer,
            welcome,
            pending: VecDeque::new(),
            next_req_id: 1,
            held: None,
        })
    }

    /// Keeps `held` alive as long as the connection is open: the program
    /// carrying it, ended as the connection is dropped.
    #[must_use]
    pub fn holding(mut self, held: impl std::any::Any + Send) -> Self {
        self.held = Some(Box::new(held));
        self
    }

    /// What the daemon said when it accepted the hello.
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// Subscribes to the daemon's state and returns the first snapshot.
    /// Later deltas, and fresh snapshots, arrive through
    /// [`next_event`](Self::next_event).
    pub async fn subscribe(&mut self) -> Result<Snapshot, ClientError> {
        within("send its state", async {
            self.send(ClientFrame::Subscribe).await?;
            loop {
                match self.receive().await? {
                    ServerFrame::Snapshot(snapshot) => return Ok(snapshot),
                    other => self.hold(other)?,
                }
            }
        })
        .await
    }

    /// Sends a command and waits for its answer, which may take as long as
    /// the work does (a clone, a wait on an agent): no time limit here.
    pub async fn command(&mut self, command: Command) -> Result<Reply, ClientError> {
        let req_id = self.next_req_id;
        self.next_req_id += 1;
        self.send(ClientFrame::Command { req_id, command }).await?;
        loop {
            match self.receive().await? {
                ServerFrame::Reply {
                    req_id: answered,
                    outcome,
                } if answered == req_id => return outcome.map_err(ClientError::Daemon),
                other => self.hold(other)?,
            }
        }
    }

    /// Streams a session's terminal from `from_seq` (or everything the
    /// daemon holds): a `Replay`, then `Output` frames, then `Detached`, all
    /// through [`next_event`](Self::next_event).
    pub async fn attach(
        &mut self,
        session: SessionId,
        from_seq: Option<u64>,
    ) -> Result<(), ClientError> {
        self.send(ClientFrame::Attach {
            session,
            from_seq,
            watching: true,
        })
        .await
    }

    /// As [`attach`](Self::attach), for a person who is not looking: the
    /// stream comes, but what happens in the session stays news.
    pub async fn attach_unwatched(
        &mut self,
        session: SessionId,
        from_seq: Option<u64>,
    ) -> Result<(), ClientError> {
        self.send(ClientFrame::Attach {
            session,
            from_seq,
            watching: false,
        })
        .await
    }

    pub async fn detach(&mut self, session: SessionId) -> Result<(), ClientError> {
        self.send(ClientFrame::Detach { session }).await
    }

    /// Starts or stops watching an attached session.
    pub async fn watch(&mut self, session: SessionId, watching: bool) -> Result<(), ClientError> {
        self.send(ClientFrame::Watch { session, watching }).await
    }

    /// Says whether the person is looking at this client's window: what it
    /// watches is watched only while they do.
    pub async fn looking(&mut self, looking: bool) -> Result<(), ClientError> {
        self.send(ClientFrame::Looking { looking }).await
    }

    /// Types into a session's terminal.
    pub async fn input(&mut self, session: SessionId, data: &[u8]) -> Result<(), ClientError> {
        self.send(ClientFrame::Input {
            session,
            data: data.to_vec(),
        })
        .await
    }

    pub async fn resize(&mut self, session: SessionId, size: TermSize) -> Result<(), ClientError> {
        self.resize_in_pixels(session, size, None).await
    }

    /// As [`resize`](Self::resize), with the size in pixels too.
    pub async fn resize_in_pixels(
        &mut self,
        session: SessionId,
        size: TermSize,
        pixels: Option<muster_core::protocol::PixelSize>,
    ) -> Result<(), ClientError> {
        self.send(ClientFrame::Resize {
            session,
            size,
            pixels,
        })
        .await
    }

    /// Tells the colours of the person's terminal, which programs in
    /// sessions asking their terminal's are told.
    pub async fn colours(
        &mut self,
        colours: muster_core::colours::TerminalColours,
    ) -> Result<(), ClientError> {
        self.send(ClientFrame::Colours(colours)).await
    }

    /// Says which session's terminal has the person's keys, if any.
    pub async fn keys(&mut self, session: Option<SessionId>) -> Result<(), ClientError> {
        self.send(ClientFrame::Keys { session }).await
    }

    async fn send(&mut self, frame: ClientFrame) -> Result<(), ClientError> {
        sendable(&frame, self.welcome.version)?;
        Ok(self.writer.send(&frame).await?)
    }

    /// The next frame the daemon pushed (a snapshot, a delta, or terminal
    /// output), or `None` once the daemon has closed the connection.
    pub async fn next_event(&mut self) -> Result<Option<ServerFrame>, ClientError> {
        if let Some(frame) = self.pending.pop_front() {
            return Ok(Some(frame));
        }
        match self.reader.next::<ServerFrame>().await? {
            Some(ServerFrame::Error(error)) => Err(ClientError::Daemon(error)),
            frame => Ok(frame),
        }
    }

    async fn receive(&mut self) -> Result<ServerFrame, ClientError> {
        match self.reader.next::<ServerFrame>().await? {
            Some(ServerFrame::Error(error)) => Err(ClientError::Daemon(error)),
            Some(frame) => Ok(frame),
            None => Err(ClientError::Closed),
        }
    }

    /// Keeps a pushed frame for `next_event`.
    fn hold(&mut self, frame: ServerFrame) -> Result<(), ClientError> {
        match frame {
            ServerFrame::Snapshot(_)
            | ServerFrame::Delta(_)
            | ServerFrame::Replay { .. }
            | ServerFrame::Output { .. }
            | ServerFrame::Modes { .. }
            | ServerFrame::Detached { .. } => {
                self.pending.push_back(frame);
                Ok(())
            }
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }
}

/// The process id of the daemon answering on `socket`, whichever protocol
/// it speaks, so it can be stopped even when this build cannot talk to it.
pub async fn daemon_pid(socket: &Path, client: &str) -> Result<u32, ClientError> {
    daemon_identity(socket, client).await.map(|(pid, _)| pid)
}

/// The process id and version of the daemon answering on `socket`,
/// whichever protocol it speaks.
pub async fn daemon_identity(socket: &Path, client: &str) -> Result<(u32, String), ClientError> {
    let stream = UnixStream::connect(socket)
        .await
        .map_err(|source| ClientError::Unreachable {
            path: socket.display().to_string(),
            source,
        })?;
    let (read, write) = stream.into_split();
    let (mut reader, mut writer) = (FramedRead::new(read), FramedWrite::new(write));
    let hello = Hello {
        min_version: FIRST_PROTOCOL_VERSION,
        ..Hello::current(client)
    };
    within("answer its hello", async {
        writer.send(&ClientFrame::Hello(hello)).await?;
        identity(reader.next::<ServerFrame>().await?)
    })
    .await
}

/// The process id and version a daemon's answer to a hello names.
fn identity(answer: Option<ServerFrame>) -> Result<(u32, String), ClientError> {
    match answer {
        Some(ServerFrame::Welcome(welcome)) => Ok((welcome.daemon_pid, welcome.daemon_version)),
        Some(ServerFrame::Incompatible(incompatible)) if incompatible.daemon_pid != 0 => {
            Ok((incompatible.daemon_pid, incompatible.daemon_version))
        }
        Some(ServerFrame::Incompatible(incompatible)) => {
            Err(ClientError::Incompatible(incompatible))
        }
        Some(ServerFrame::Error(error)) => Err(ClientError::Daemon(error)),
        Some(other) => Err(ClientError::Unexpected(format!("{other:?}"))),
        None => Err(ClientError::Closed),
    }
}

/// The sending half of a split [`Connection`], which keeps what the
/// connection held alive.
pub struct FrameSender {
    writer: FramedWrite<Writer>,
    /// The protocol agreed with the daemon.
    version: u32,
    _held: Option<Held>,
}

impl std::fmt::Debug for FrameSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameSender").finish_non_exhaustive()
    }
}

impl FrameSender {
    /// Sends one frame. The caller numbers its commands and matches the
    /// replies the receiving half delivers.
    pub async fn send(&mut self, frame: &ClientFrame) -> Result<(), ClientError> {
        sendable(frame, self.version)?;
        Ok(self.writer.send(frame).await?)
    }

    /// Sends `frames` in order until they end or the connection does; the
    /// receiving half notices a connection gone.
    pub async fn send_all(mut self, mut frames: tokio::sync::mpsc::UnboundedReceiver<ClientFrame>) {
        while let Some(frame) = frames.recv().await {
            if self.send(&frame).await.is_err() {
                return;
            }
        }
    }
}

/// Refuses `frame` where the daemon's protocol, `version`, does not have
/// it (`ARC-09`): the one check every client makes, as all send through
/// here.
fn sendable(frame: &ClientFrame, version: u32) -> Result<(), ClientError> {
    let needs = frame.since();
    if needs > version {
        return Err(ClientError::DaemonTooOld {
            needs,
            daemon: version,
        });
    }
    Ok(())
}

/// The receiving half of a split [`Connection`].
pub struct FrameReceiver {
    reader: FramedRead<Reader>,
    pending: VecDeque<ServerFrame>,
}

impl std::fmt::Debug for FrameReceiver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameReceiver")
            .field("pending", &self.pending.len())
            .finish_non_exhaustive()
    }
}

impl FrameReceiver {
    /// The next frame from the daemon, replies included, or `None` once it
    /// has closed the connection. A connection-level error frame is returned
    /// as an error.
    pub async fn next(&mut self) -> Result<Option<ServerFrame>, ClientError> {
        if let Some(frame) = self.pending.pop_front() {
            return Ok(Some(frame));
        }
        match self.reader.next::<ServerFrame>().await? {
            Some(ServerFrame::Error(error)) => Err(ClientError::Daemon(error)),
            frame => Ok(frame),
        }
    }

    /// Hands `take` each frame from the daemon, waiting for it, until it
    /// says it takes no more (`false`) or the connection ends: why it
    /// ended, or `None` where `take` stopped it.
    pub async fn forward<F, Took>(mut self, mut take: F) -> Option<String>
    where
        F: FnMut(ServerFrame) -> Took,
        Took: Future<Output = bool>,
    {
        loop {
            match self.next().await {
                Ok(Some(frame)) => {
                    if !take(frame).await {
                        return None;
                    }
                }
                Ok(None) => return Some("the daemon closed the connection".to_owned()),
                Err(error) => return Some(error.to_string()),
            }
        }
    }
}

impl Connection {
    /// Splits the connection so frames can be sent while others are being
    /// received, as an interactive client needs. Frames held for
    /// [`next_event`](Self::next_event) come first from the receiver.
    pub fn into_split(self) -> (FrameSender, FrameReceiver, Welcome) {
        (
            FrameSender {
                writer: self.writer,
                version: self.welcome.version,
                _held: self.held,
            },
            FrameReceiver {
                reader: self.reader,
                pending: self.pending,
            },
            self.welcome,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_the_daemon_s_protocol_lacks_is_never_sent_arc_09() {
        let exec = ClientFrame::Command {
            req_id: 1,
            command: Command::ExecAgent {
                session: SessionId::new(),
                harness: "claude".to_owned(),
                prompt: None,
            },
        };
        assert!(sendable(&exec, 54).is_ok());
        let refused = sendable(&exec, 53).unwrap_err();
        assert!(
            matches!(
                refused,
                ClientError::DaemonTooOld {
                    needs: 54,
                    daemon: 53
                }
            ),
            "{refused}"
        );
        assert!(refused.to_string().contains("muster reload"));
        assert!(sendable(&ClientFrame::Subscribe, MIN_PROTOCOL_VERSION).is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn a_daemon_that_never_answers_its_hello_is_said_to_be_stuck() {
        // Keeps the daemon's ends open, so it is silent, not gone.
        let (_daemon_reads, client_writes) = tokio::io::duplex(1024);
        let (_daemon_writes, client_reads) = tokio::io::duplex(1024);
        let error = Connection::over(client_reads, client_writes, "test")
            .await
            .unwrap_err();
        assert!(
            matches!(error, ClientError::Silent("answer its hello")),
            "{error}"
        );
        assert!(error.to_string().contains("within 30 s"), "{error}");
    }
}
