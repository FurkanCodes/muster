//! The messages between the daemon and its clients (`ARCHITECTURE.md` §6).
//!
//! Versioning rules (`ARC-09`):
//! - every change to these types bumps [`PROTOCOL_VERSION`];
//! - a new optional field carries `#[serde(default)]`;
//! - a new variant is sent only to a peer whose agreed version knows it;
//! - removing or renaming anything also raises [`MIN_PROTOCOL_VERSION`].
//!
//! Protocol 1 has not been released; until the first release it changes
//! without a version bump.
//!
//! Frames are encoded with named fields, so a field can be added to any
//! message, including [`Hello`], without breaking older peers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::entities::{Project, Session, Workspace};
use crate::ids::{ProjectId, SessionId, WorkspaceId};
use crate::session::SessionStatus;

/// The newest protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 1;
/// The oldest protocol this build still speaks.
pub const MIN_PROTOCOL_VERSION: u32 = 1;

/// The first frame a client sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hello {
    pub min_version: u32,
    pub max_version: u32,
    /// Who is connecting, for the daemon's log, such as `muster-tui 0.1.0`.
    pub client: String,
    /// Optional features the client supports. None are defined yet.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl Hello {
    /// A hello for this build's protocol range.
    pub fn current(client: impl Into<String>) -> Self {
        Self {
            min_version: MIN_PROTOCOL_VERSION,
            max_version: PROTOCOL_VERSION,
            client: client.into(),
            capabilities: Vec::new(),
        }
    }
}

/// The daemon accepted the hello.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Welcome {
    /// The protocol both sides will speak.
    pub version: u32,
    pub daemon_pid: u32,
    pub daemon_version: String,
}

/// The daemon and the client share no protocol version. The connection closes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Incompatible {
    pub min_version: u32,
    pub max_version: u32,
    pub daemon_version: String,
}

/// The newest version both `hello` and this build speak, if any.
pub fn negotiate(hello: &Hello) -> Option<u32> {
    let version = hello.max_version.min(PROTOCOL_VERSION);
    (version >= hello.min_version.max(MIN_PROTOCOL_VERSION)).then_some(version)
}

/// Everything a client sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClientFrame {
    /// Must come first, once.
    Hello(Hello),
    /// Ask for a [`Snapshot`] followed by every [`Delta`] after it.
    Subscribe,
    /// Ask the daemon to do something; answered by a [`ServerFrame::Reply`]
    /// with the same `req_id`.
    Command { req_id: u64, command: Command },
    /// Stream a session's terminal: a [`ServerFrame::Replay`] from `from_seq`
    /// (or everything the daemon still holds), then [`ServerFrame::Output`]
    /// as it arrives. Attaching counts as looking at the session.
    Attach {
        session: SessionId,
        from_seq: Option<u64>,
    },
    /// Stop streaming a session's terminal.
    Detach { session: SessionId },
    /// Keystrokes or pasted text for a session's terminal.
    Input {
        session: SessionId,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    /// The client's view of a session's terminal changed size.
    Resize { session: SessionId, size: TermSize },
}

/// Everything the daemon sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerFrame {
    Welcome(Welcome),
    Incompatible(Incompatible),
    /// The whole state. Sent after `Subscribe`, and again whenever the client
    /// fell too far behind to be sent the deltas it missed.
    Snapshot(Snapshot),
    /// One change after the last snapshot.
    Delta(Delta),
    Reply {
        req_id: u64,
        outcome: Result<Reply, ErrorBody>,
    },
    /// A problem with the connection itself. The daemon closes it after this.
    Error(ErrorBody),
    /// The start of an attached terminal stream: the bytes from `seq` on.
    /// With `reset`, the client must clear its screen first, because the
    /// bytes it last saw are no longer held.
    Replay {
        session: SessionId,
        seq: u64,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
        reset: bool,
    },
    /// Terminal output that follows the replay; `seq` is the position of its
    /// first byte in everything the session has written.
    Output {
        session: SessionId,
        seq: u64,
        #[serde(with = "serde_bytes")]
        data: Vec<u8>,
    },
    /// A terminal stream ended.
    Detached {
        session: SessionId,
        reason: DetachReason,
    },
}

/// Why a terminal stream ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetachReason {
    /// The session's process exited; everything it wrote has been sent.
    Exited,
    /// The session is unknown, or its terminal is no longer held.
    NotRunning,
}

/// A terminal's size in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TermSize {
    pub cols: u16,
    pub rows: u16,
}

impl Default for TermSize {
    fn default() -> Self {
        Self { cols: 80, rows: 24 }
    }
}

/// Something a client asks the daemon to do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    /// Check the daemon is answering.
    Ping,
    /// Start tracking a git repository.
    AddProject {
        root: PathBuf,
        /// Defaults to the repository's directory name.
        name: Option<String>,
    },
    /// Create a git worktree for a project: on `branch` if it exists,
    /// otherwise on a new branch of that name (default `muster/<random>`)
    /// from the project's current commit.
    CreateWorktree {
        project: ProjectId,
        branch: Option<String>,
    },
    /// Remove a worktree's directory. Its branch is kept. A worktree with
    /// uncommitted changes is removed only with `force`; one with running
    /// sessions never is.
    RemoveWorktree { workspace: WorkspaceId, force: bool },
    /// Start a session in a workspace.
    StartSession {
        workspace: WorkspaceId,
        /// A harness id: `shell` or `claude`.
        harness: String,
        title: Option<String>,
        size: TermSize,
        /// A first prompt, for harnesses that take one.
        #[serde(default)]
        prompt: Option<String>,
    },
    /// Stop a running session.
    StopSession { session: SessionId },
}

/// A successful answer to a [`Command`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Reply {
    Pong,
    ProjectAdded {
        id: ProjectId,
    },
    WorktreeCreated {
        id: WorkspaceId,
    },
    SessionStarted {
        id: SessionId,
    },
    /// The command was carried out and has nothing to report.
    Done,
}

/// The daemon's whole state, as a client needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
}

/// One change to the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delta {
    ProjectUpserted(Project),
    WorkspaceUpserted(Workspace),
    WorkspaceRemoved {
        id: WorkspaceId,
    },
    SessionUpserted(Session),
    SessionStatus {
        session: SessionId,
        status: SessionStatus,
        unseen: bool,
    },
}

/// A failure the client can act on: `code` is for code, `message` for people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[error("{message} ({code:?})")]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

/// Machine-readable error kinds (`ARC-07`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// The request is malformed or names something invalid.
    BadRequest,
    /// The request names something that does not exist.
    NotFound,
    /// The request clashes with existing state.
    Conflict,
    /// Something else must come first, such as the hello.
    OutOfOrder,
    /// The daemon failed; the message says how.
    Internal,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello(min_version: u32, max_version: u32) -> Hello {
        Hello {
            min_version,
            max_version,
            client: "test".to_owned(),
            capabilities: Vec::new(),
        }
    }

    #[test]
    fn negotiation_picks_the_newest_shared_version() {
        assert_eq!(negotiate(&Hello::current("t")), Some(PROTOCOL_VERSION));
        assert_eq!(negotiate(&hello(1, 99)), Some(PROTOCOL_VERSION));
    }

    #[test]
    fn negotiation_fails_without_overlap() {
        assert_eq!(
            negotiate(&hello(PROTOCOL_VERSION + 1, PROTOCOL_VERSION + 5)),
            None
        );
        assert_eq!(negotiate(&hello(0, MIN_PROTOCOL_VERSION - 1)), None);
    }
}
