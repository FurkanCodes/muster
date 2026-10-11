//! The messages between the daemon and its clients (`ARCHITECTURE.md` §6).
//!
//! Versioning rules (`ARC-09`):
//! - every change to these types bumps [`PROTOCOL_VERSION`];
//! - a new optional field carries `#[serde(default)]`;
//! - a new variant is sent only to a peer whose agreed version knows it;
//! - removing or renaming anything also raises [`MIN_PROTOCOL_VERSION`].
//!
//! Until the first release the protocol may change in ways older peers
//! cannot read, but each such change still bumps both versions, so a daemon
//! left running by an older build and a newer client say they do not match
//! instead of misreading each other. Version 2 added restarting and removing
//! sessions, projects' own checkouts as workspaces, and removing projects.
//!
//! Frames are encoded with named fields, so a field can be added to any
//! message, including [`Hello`], without breaking older peers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::entities::{Issue, Project, Session, Workspace};
use crate::ids::{ProjectId, SessionId, WorkspaceId};
use crate::session::SessionStatus;

/// The newest protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 2;
/// The oldest protocol this build still speaks.
pub const MIN_PROTOCOL_VERSION: u32 = 2;

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
    /// So a client can tell the person which process to stop; 0 from
    /// daemons older than protocol 2.
    #[serde(default)]
    pub daemon_pid: u32,
}

/// The first protocol version there ever was. A hello offering every
/// version from here is answered with a welcome by any daemon this build
/// can still talk to, and also by older ones, whose welcome carries their
/// process id too.
pub const FIRST_PROTOCOL_VERSION: u32 = 1;

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
    /// as it arrives. The stream follows the session to a new program, with
    /// a reset. While attached, the session counts as watched: nothing that
    /// happens in it is news.
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
    /// How the session's program wants its keys and pastes encoded. Sent
    /// after the replay and whenever it changes.
    Modes {
        session: SessionId,
        modes: TerminalModes,
    },
    /// A terminal stream ended.
    Detached {
        session: SessionId,
        reason: DetachReason,
    },
}

/// Input modes a program sets on its terminal, which decide how a client
/// turns key presses and pastes into bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TerminalModes {
    /// The kitty keyboard protocol flags in effect; 0 means legacy keys.
    pub kitty_flags: u8,
    /// Pastes are wrapped in `ESC [200~` … `ESC [201~`.
    pub bracketed_paste: bool,
    /// Cursor keys send `ESC O A` rather than `ESC [ A` (DECCKM).
    pub application_cursor: bool,
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
    /// Start tracking a git repository. Its own checkout becomes the
    /// project's first workspace.
    AddProject {
        root: PathBuf,
        /// Defaults to the repository's directory name.
        name: Option<String>,
    },
    /// Stop tracking a project. Its repository is not touched. Refused while
    /// it has worktrees (remove them first) or running sessions.
    RemoveProject { project: ProjectId },
    /// Create a git worktree for a project: on `branch` if it exists,
    /// otherwise on a new branch of that name (default `muster/<random>`)
    /// from the project's current commit.
    CreateWorktree {
        project: ProjectId,
        branch: Option<String>,
    },
    /// Remove a worktree's directory. Its branch is kept. A worktree with
    /// uncommitted changes is removed only with `force`; one with running
    /// sessions never is, nor a project's own checkout.
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
    /// Start an ended session's program again, in the same session: Claude
    /// Code resumes its conversation if it had one, a shell starts afresh.
    /// Answered with [`Reply::SessionStarted`]. `size` is used when the
    /// daemon no longer knows the session's terminal size.
    RestartSession { session: SessionId, size: TermSize },
    /// Take an ended session off the list. Its history is kept.
    RemoveSession { session: SessionId },
    /// List a project's open GitHub issues, newest first.
    ListIssues { project: ProjectId },
    /// Work on a GitHub issue: a worktree on `muster/issue-<n>-<title>`
    /// (reused if it exists) and a session in it. Claude Code gets the issue
    /// as its first prompt, marked as data rather than instructions.
    StartFromIssue {
        project: ProjectId,
        number: u64,
        /// A harness id: `claude` or `shell`.
        harness: String,
        size: TermSize,
    },
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
    Issues {
        issues: Vec<Issue>,
    },
    IssueStarted {
        workspace: WorkspaceId,
        session: SessionId,
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
    /// The latest measurement, once one has been taken.
    #[serde(default)]
    pub resources: Option<ResourceReport>,
}

/// What the machine and each running session use, measured every couple of
/// seconds (`PRD-05`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceReport {
    pub host_memory_used: u64,
    pub host_memory_total: u64,
    pub sessions: Vec<SessionUsage>,
}

/// One session's use: its program and every process it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session: SessionId,
    pub memory_bytes: u64,
    /// CPU in thousandths of one core: 1500 is one and a half cores.
    pub cpu_permille: u32,
}

/// One change to the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Delta {
    ProjectUpserted(Project),
    /// A project, and with it its workspaces and their sessions, left.
    ProjectRemoved {
        id: ProjectId,
    },
    WorkspaceUpserted(Workspace),
    WorkspaceRemoved {
        id: WorkspaceId,
    },
    SessionUpserted(Session),
    /// A session was taken off the list.
    SessionRemoved {
        id: SessionId,
    },
    SessionStatus {
        session: SessionId,
        status: SessionStatus,
        unseen: bool,
    },
    /// A new measurement; replaces the last.
    Resources(ResourceReport),
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
