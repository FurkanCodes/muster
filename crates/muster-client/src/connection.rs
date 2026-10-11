//! A client's connection to the daemon.

use std::collections::VecDeque;
use std::io;
use std::path::Path;

use muster_core::protocol::{
    ClientFrame, Command, ErrorBody, Hello, Incompatible, Reply, ServerFrame, Snapshot, Welcome,
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
        "the daemon ({}) speaks protocol {}..={}, which this client does not; restart the daemon",
        .0.daemon_version, .0.min_version, .0.max_version
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
/// daemon pushes meanwhile wait for [`next_event`](Self::next_event).
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

    /// The next snapshot or delta the daemon pushed, or `None` once the
    /// daemon has closed the connection.
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
            ServerFrame::Snapshot(_) | ServerFrame::Delta(_) => {
                self.pending.push_back(frame);
                Ok(())
            }
            other => Err(ClientError::Unexpected(format!("{other:?}"))),
        }
    }
}
