//! A client's connection to the daemon.

use std::collections::VecDeque;
use std::io;
use std::path::Path;

use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, Command, ErrorBody, FIRST_PROTOCOL_VERSION, Hello, Incompatible,
    MIN_PROTOCOL_VERSION, PROTOCOL_VERSION, Reply, ServerFrame, Snapshot, TermSize, Welcome,
};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

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
}

/// An open, greeted connection. Requests are answered in turn; frames the
/// daemon pushes meanwhile (state changes and terminal output) wait for
/// [`next_event`](Self::next_event).
#[derive(Debug)]
pub struct Connection {
    reader: FramedRead<OwnedReadHalf>,
    writer: FramedWrite<OwnedWriteHalf>,
    welcome: Welcome,
    pending: VecDeque<ServerFrame>,
    next_req_id: u64,
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
        let (mut reader, mut writer) = (FramedRead::new(read), FramedWrite::new(write));

        writer
            .send(&ClientFrame::Hello(Hello::current(client)))
            .await?;
        let welcome = match reader.next::<ServerFrame>().await? {
            Some(ServerFrame::Welcome(welcome)) => welcome,
            Some(ServerFrame::Incompatible(incompatible)) => {
                return Err(ClientError::Incompatible(incompatible));
            }
            Some(ServerFrame::Error(error)) => return Err(ClientError::Daemon(error)),
            Some(other) => return Err(ClientError::Unexpected(format!("{other:?}"))),
            None => return Err(ClientError::Closed),
        };
        Ok(Self {
            reader,
            writer,
            welcome,
            pending: VecDeque::new(),
            next_req_id: 1,
        })
    }

    /// What the daemon said when it accepted the hello.
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    /// Subscribes to the daemon's state and returns the first snapshot.
    /// Later deltas, and fresh snapshots, arrive through
    /// [`next_event`](Self::next_event).
    pub async fn subscribe(&mut self) -> Result<Snapshot, ClientError> {
        self.writer.send(&ClientFrame::Subscribe).await?;
        loop {
            match self.receive().await? {
                ServerFrame::Snapshot(snapshot) => return Ok(snapshot),
                other => self.hold(other)?,
            }
        }
    }

    /// Sends a command and waits for its answer.
    pub async fn command(&mut self, command: Command) -> Result<Reply, ClientError> {
        let req_id = self.next_req_id;
        self.next_req_id += 1;
        self.writer
            .send(&ClientFrame::Command { req_id, command })
            .await?;
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
        self.send(ClientFrame::Resize { session, size }).await
    }

    async fn send(&mut self, frame: ClientFrame) -> Result<(), ClientError> {
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
    writer.send(&ClientFrame::Hello(hello)).await?;
    match reader.next::<ServerFrame>().await? {
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

/// The sending half of a split [`Connection`].
#[derive(Debug)]
pub struct FrameSender {
    writer: FramedWrite<OwnedWriteHalf>,
}

impl FrameSender {
    /// Sends one frame. The caller numbers its commands and matches the
    /// replies the receiving half delivers.
    pub async fn send(&mut self, frame: &ClientFrame) -> Result<(), ClientError> {
        Ok(self.writer.send(frame).await?)
    }
}

/// The receiving half of a split [`Connection`].
#[derive(Debug)]
pub struct FrameReceiver {
    reader: FramedRead<OwnedReadHalf>,
    pending: VecDeque<ServerFrame>,
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
}

impl Connection {
    /// Splits the connection so frames can be sent while others are being
    /// received, as an interactive client needs. Frames held for
    /// [`next_event`](Self::next_event) come first from the receiver.
    pub fn into_split(self) -> (FrameSender, FrameReceiver, Welcome) {
        (
            FrameSender {
                writer: self.writer,
            },
            FrameReceiver {
                reader: self.reader,
                pending: self.pending,
            },
            self.welcome,
        )
    }
}
