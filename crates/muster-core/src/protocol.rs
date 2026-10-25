//! The messages between the daemon and its clients (`ARCHITECTURE.md` §6).
//!
//! Versioning rules (`ARC-09`):
//! - every change to these types bumps [`PROTOCOL_VERSION`];
//! - a new optional field carries `#[serde(default)]`;
//! - a new variant is sent only to a peer whose agreed version knows it:
//!   a client's frame says its version in [`ClientFrame::since`], which
//!   `muster-client` checks once for every client; the first delta that
//!   needs it adds a `Delta::since` the daemon checks as it sends;
//! - removing or renaming anything also raises [`MIN_PROTOCOL_VERSION`];
//! - the handshake ([`Hello`], [`Welcome`], [`Incompatible`], the error for
//!   a frame before the hello) and the frame header never change shape:
//!   each version's encoding of them is frozen in `tests/frozen/`, and
//!   raising [`PROTOCOL_VERSION`] adds that version's (see
//!   `tests/frozen_protocol.rs`).
//!
//! What each version added is in git's history, not here.
//!
//! Frames are encoded with named fields, so a field can be added to any
//! message, including [`Hello`], without breaking older peers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod agents;
pub mod plugins;
pub mod repositories;
mod state;
mod terminal;

pub use state::{Delta, Health, ResourceReport, SessionUsage, Snapshot};
pub use terminal::{
    DetachReason, MouseEncoding, MouseTracking, PixelSize, TermSize, TerminalModes,
};

use crate::changes::{ChangePart, Changes, FileDiff};
use crate::colours::TerminalColours;
use crate::entities::Issue;
use crate::ids::{ProjectId, RunId, SessionId, SpaceId, WorkspaceId};
use crate::settings::{PreferenceChange, ProjectSettings, SettingChange};
use crate::tickets::TicketKey;
use crate::timeline::{Timeline, UndoPlan};

/// The newest protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 58;
/// The oldest protocol this build still speaks: 58's, where everything is
/// in one space and `AddToSpace` and `RemoveFromSpace` gave way to
/// `MoveToSpace` (`ARC-33`), which an older client could not send. Older
/// clients and daemons are told to update.
pub const MIN_PROTOCOL_VERSION: u32 = 58;

/// The first frame a client sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Welcome {
    /// The protocol both sides will speak.
    pub version: u32,
    pub daemon_pid: u32,
    pub daemon_version: String,
}

/// The daemon and the client share no protocol version. The connection closes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
        /// Whether the person is looking; a client whose window is out of
        /// focus attaches without watching (see [`ClientFrame::Watch`]).
        #[serde(default = "watching_by_default")]
        watching: bool,
    },
    /// Stop streaming a session's terminal.
    Detach { session: SessionId },
    /// Whether the person is looking at an attached session: a client whose
    /// window lost focus stops watching, so what happens meanwhile is news,
    /// and starts again when it is back. Attaching starts watching.
    Watch { session: SessionId, watching: bool },
    /// Keystrokes or pasted text for a session's terminal.
    Input {
        session: SessionId,
        #[serde(with = "serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
        data: Vec<u8>,
    },
    /// The client's view of a session's terminal changed size.
    Resize {
        session: SessionId,
        size: TermSize,
        /// Its size in pixels, where the person's terminal says its own: for
        /// programs that size pictures or a cursor by it.
        #[serde(default)]
        pixels: Option<PixelSize>,
    },
    /// Whether the person is looking at this client's window: a UI says so as
    /// it connects and whenever its window gains or loses focus. A client that
    /// never says is not looked at.
    Looking { looking: bool },
    /// The colours of the person's terminal, as the UI found them as it
    /// started: a program in a session asking its terminal for its colours
    /// (OSC 10, 11 and 4) is told these, the last a UI said. Sent only with
    /// a colour in it.
    Colours(TerminalColours),
    /// Which session's terminal is in focus in this client, if any: the
    /// one shown, beside the list or filling the window, in a window the
    /// person looks at, whether or not it has the keys. A program that
    /// asked to be told its terminal's focus (`CSI ? 1004 h`) is sent
    /// `CSI I` as its session comes into focus in any client and `CSI O`
    /// as it leaves it in every one. Sent whenever it changes.
    Keys { session: Option<SessionId> },
    /// Muster's helper on a Mac can show notifications of the Mac's own, the
    /// person having allowed them, or no longer can.
    Notifier { ready: bool },
}

impl ClientFrame {
    /// The first protocol whose daemon reads this frame (`ARC-09`), which
    /// `muster-client` checks before it sends one: a frame added after
    /// [`MIN_PROTOCOL_VERSION`] says its version here.
    #[must_use]
    pub fn since(&self) -> u32 {
        // Every frame is as old as the oldest protocol spoken.
        MIN_PROTOCOL_VERSION
    }
}

fn watching_by_default() -> bool {
    true
}

/// Everything the daemon sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
    /// bytes it last saw are no longer held. A stream starts again with one
    /// when its client fell behind it, and when a new program starts in the
    /// session.
    Replay {
        session: SessionId,
        seq: u64,
        #[serde(with = "serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
        data: Vec<u8>,
        reset: bool,
        /// Where the output written while the client was attached begins,
        /// in a replay that starts the stream again: from there it is live
        /// output the client was not sent as it came, so what the program
        /// does in it (a copy) counts as in [`ServerFrame::Output`].
        /// `None` in the replay that starts a stream: it is all history.
        #[serde(default)]
        live_from: Option<u64>,
        /// With `reset`, the program's screen as it stands at the end of
        /// `data`, drawn whole: the main screen or the alternate one, every
        /// cell, the cursor, the colours being drawn in and the input modes,
        /// as escape sequences. `data`, cut from the middle of the output
        /// once the daemon no longer holds its start, cannot draw it on its
        /// own: a client draws this after it, `data` then being history for
        /// its scrollback. It takes no place in the output.
        #[serde(default, with = "serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "Array<number> | null"))]
        screen: Option<Vec<u8>>,
    },
    /// Terminal output that follows the replay; `seq` is the position of its
    /// first byte in everything the session has written.
    Output {
        session: SessionId,
        seq: u64,
        #[serde(with = "serde_bytes")]
        #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
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
    /// Show this session: the person asked for it elsewhere, in the notch.
    /// Sent only to a client that said whether it is looked at
    /// ([`ClientFrame::Looking`]), the one looked at last.
    Reveal {
        session: SessionId,
    },
}

/// Something a client asks the daemon to do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub enum Command {
    /// Check the daemon is answering.
    Ping,
    /// Start tracking a git repository. Its own checkout becomes the
    /// project's first workspace.
    AddProject {
        root: PathBuf,
        /// Defaults to the repository's directory name.
        name: Option<String>,
        /// The space it joins, the one the client shows; `None` for none.
        #[serde(default)]
        space: Option<SpaceId>,
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
        /// The workspace whose branch a new branch is made on top of, rather
        /// than on what the checkout has.
        #[serde(default)]
        on: Option<WorkspaceId>,
        /// With no `branch`, a new branch named from these words (a prompt's:
        /// `muster/fix-the-login`), `-2`, `-3`… where that is taken, never an
        /// existing branch.
        #[serde(default)]
        named_for: Option<String>,
    },
    /// Remove a worktree's directory. Its branch is kept. A worktree with
    /// uncommitted changes is removed only with `force`; one with running
    /// sessions never is, nor a project's own checkout.
    RemoveWorktree { workspace: WorkspaceId, force: bool },
    /// Start a session in a workspace, or a free terminal in none
    /// (`ARC-33`): an agent always starts in a workspace.
    StartSession {
        workspace: Option<WorkspaceId>,
        /// A harness id: `shell`, `claude`, or an agent the person added
        /// ([`crate::settings::Preferences::agents`]).
        harness: String,
        title: Option<String>,
        size: TermSize,
        /// A first prompt, for harnesses that take one.
        #[serde(default)]
        prompt: Option<String>,
        /// The agent's session asking, where an agent runs the command line
        /// (`PRD-01`): the daemon refuses past the person's limits and marks
        /// what it starts.
        #[serde(default)]
        started_by: Option<SessionId>,
        /// The space it joins, the one the client shows; `None` for none.
        #[serde(default)]
        space: Option<SpaceId>,
    },
    /// Stop a running session.
    StopSession { session: SessionId },
    /// Run the project's checks in a workspace on the host, as a session of
    /// their own; answered with [`Reply::SessionStarted`]. Starting that
    /// session again runs them again.
    CheckWorkspace {
        workspace: WorkspaceId,
        size: TermSize,
    },
    /// Have Claude Code review a workspace's change against the branch it
    /// is to merge into, in plan mode (it changes nothing), as a session;
    /// answered with [`Reply::SessionStarted`].
    ReviewWorkspace {
        workspace: WorkspaceId,
        size: TermSize,
    },
    /// Have the agent `harness` tidy a workspace's branch's commits not
    /// pushed yet, the files left as they are, as a session; answered with
    /// [`Reply::SessionStarted`]. Refused while an agent works there.
    TidyCommits {
        workspace: WorkspaceId,
        harness: String,
        size: TermSize,
    },
    /// Move a workspace's branch onto the newest commit of the branch it is
    /// stacked on, or of its base once that has merged, or of its base where it
    /// is not stacked, as the person asked (`PRD-14`): its own commits replayed
    /// there, and nothing changed where one would conflict; refused while an
    /// agent works there. Answered with [`Reply::Moved`].
    MoveOnto { workspace: WorkspaceId },
    /// Push a workspace's branch, as committed, with the person's own git,
    /// and open a draft pull request from it, or find the one open;
    /// answered with [`Reply::PullRequestOpened`].
    OpenPullRequest { workspace: WorkspaceId },
    /// Start an ended session's program again, in the same session: Claude
    /// Code resumes its conversation if it had one, a shell starts afresh.
    /// Answered with [`Reply::SessionStarted`]. `size` is used when the
    /// daemon no longer knows the session's terminal size.
    RestartSession { session: SessionId, size: TermSize },
    /// Take an ended session off the list. Its history is kept.
    RemoveSession { session: SessionId },
    /// Give a session the name `title` (`muster_core::names::given`).
    RenameSession { session: SessionId, title: String },
    /// List a project's open tickets, newest first, from its tracker.
    ListIssues { project: ProjectId },
    /// One of a project's tickets with its text, answered with
    /// [`Reply::IssueViewed`].
    ViewIssue { project: ProjectId, key: TicketKey },
    /// Close one of a project's tickets in its tracker, as the person
    /// asked: the one change Muster makes in a tracker. Answered with
    /// [`Reply::Done`].
    CloseIssue { project: ProjectId, key: TicketKey },
    /// The project's settings that the person can change, as they stand;
    /// answered with [`Reply::ProjectSettings`].
    ProjectSettings { project: ProjectId },
    /// What a workspace's work has changed: what is not committed, and the
    /// branch's commits since it left the branch it is to merge into, with
    /// their files; answered with [`Reply::Changes`].
    WorkspaceChanges { workspace: WorkspaceId },
    /// One changed file's diff, in one part of a workspace's change; answered
    /// with [`Reply::FileDiff`].
    FileDiff {
        workspace: WorkspaceId,
        part: ChangePart,
        path: String,
    },
    /// A workspace's timeline: each agent's turn there, what changed between
    /// them, and each undoing, newest first; answered with [`Reply::Timeline`].
    Timeline { workspace: WorkspaceId },
    /// What one entry of a workspace's timeline changed, as a diff; answered
    /// with [`Reply::FileDiff`].
    EntryDiff { workspace: WorkspaceId, number: u64 },
    /// What putting the files back as they were before an entry would do;
    /// answered with [`Reply::UndoPlan`].
    UndoPlan { workspace: WorkspaceId, number: u64 },
    /// Put a workspace's files back as they were before an entry, as the person
    /// asked, and with `move_branch` its branch too where it can go (`PRD-14`);
    /// refused while an agent works there. Answered with [`Reply::Undone`].
    Undo {
        workspace: WorkspaceId,
        number: u64,
        #[serde(default)]
        move_branch: bool,
    },
    /// How the daemon is doing inside, answered with [`Reply::Health`].
    Health,
    /// Why a session has its status: what its machine holds and the signals it
    /// last received; answered with [`Reply::Explanation`].
    ExplainSession { session: SessionId },
    /// A session's screen as it stands, or the answer its agent's last turn
    /// ended with, as text; answered with [`Reply::SessionText`].
    ReadSession { session: SessionId, what: Reading },
    /// Show a session in the Muster window the person looked at last
    /// ([`ServerFrame::Reveal`]); refused when no window is open.
    Reveal { session: SessionId },
    /// Show `to` instead of `from` wherever the person is watching `from` now:
    /// a terminal tab of the command line attached to it, or a window showing
    /// it, is sent [`ServerFrame::Reveal`] for `to`. So a session started from
    /// inside a session's terminal (`muster claude` typed in a workspace's
    /// shell) takes the place the person typed in. Answered with
    /// [`Reply::Done`], or refused when nothing shows `from`.
    Switch { from: SessionId, to: SessionId },
    /// Make `workspace` (a worktree) the one ticket `key` is worked on in, as
    /// the person asked; `None` makes it no ticket's. Its agents are then the
    /// ticket's, and starting work on the ticket goes there. Refused for a
    /// project's own checkout, and where another worktree of the project has
    /// the ticket. Answered with [`Reply::Done`].
    AssignTicket {
        workspace: WorkspaceId,
        key: Option<TicketKey>,
    },
    /// Run coding agent `harness` in the foreground of terminal `session`, as
    /// `muster exec` asks from inside it (`muster init`'s functions): with the
    /// terminal's hooks and a session id the daemon chooses, and `prompt` to
    /// start on, heard from now as the terminal's agent (`ARC-28`). Answered
    /// with [`Reply::AgentCommand`], which the asker runs.
    ExecAgent {
        session: SessionId,
        harness: String,
        #[serde(default)]
        prompt: Option<String>,
    },
    /// Change one of Muster's own settings, in the person's own file; answered
    /// with [`Reply::Done`], every client told by [`Delta::Preferences`].
    ChangePreference(PreferenceChange),
    /// Read Muster's own settings again, after the person edited them by hand:
    /// the agents they added among them. Answered with [`Reply::Done`], every
    /// client told by [`Delta::Preferences`].
    ReadPreferences,
    /// The models the coding agent `agent` (a harness's id) offers for naming
    /// chats: those it lists, where it can, else those Muster suggests for it.
    /// Answered with [`Reply::Models`].
    ListModels { agent: String },
    /// Where each coding agent Muster can start is installed, on the daemon's
    /// own `PATH`, and the version it says it is: Muster's own and those the
    /// person added, each program run with `--version`, never anything else.
    /// Answered with [`Reply::Agents`].
    Agents,
    /// The repositories the person can reach on GitHub, through their own `gh`,
    /// to clone one as a project. Answered with [`Reply::Repositories`].
    ListRepositories,
    /// Clone `repository` (`owner/name`, or its github.com address) into a
    /// folder of its name in `into`, through the person's own `gh`, and add it
    /// as a project. Answered with [`Reply::ProjectAdded`] once cloned.
    CloneProject {
        repository: String,
        into: PathBuf,
        /// The space it joins, the one the client shows; `None` for none.
        #[serde(default)]
        space: Option<SpaceId>,
    },
    /// Change one of the project's settings, in the person's own overrides
    /// of its `muster.toml`, never in the repository (`ARC-05`); refused if
    /// the settings would not read with it. Answered with
    /// [`Reply::ProjectSettings`] as they stand after.
    ChangeSetting {
        project: ProjectId,
        change: SettingChange,
    },
    /// Hand a GitHub issue to the autopilot: a run that builds, checks,
    /// opens a draft pull request and has it reviewed before asking the
    /// person (`PRODUCT.md` §5). Needs the project's `muster.toml` checks.
    /// With `from`, the run takes over the work on that workspace's
    /// branch, as committed, and starts at the checks (`PRD-08`).
    StartRun {
        project: ProjectId,
        number: u64,
        #[serde(default)]
        from: Option<WorkspaceId>,
    },
    /// Stop a run for good; its container goes, its record stays.
    CancelRun { run: RunId },
    /// Send a run that needs the person back to work, with a fresh budget
    /// and, if given, notes for its agent.
    RetryRun {
        run: RunId,
        #[serde(default)]
        notes: Option<String>,
    },
    /// Drive a run's building agent by hand: Muster stops its own work and
    /// starts the agent interactively in the run's container, resuming its
    /// conversation. Answered with [`Reply::RunTakenOver`].
    TakeOverRun { run: RunId },
    /// Give a run back after driving it: the sessions in its container
    /// stop and its work goes to the checks.
    HandBackRun { run: RunId },
    /// Send a run that is ready for the person back to its building agent
    /// with what to change; its pull request goes back to draft.
    RequestChanges { run: RunId, comments: String },
    /// A run's timeline, answered with [`Reply::RunHistory`].
    RunHistory { run: RunId },
    /// Work on a ticket: a worktree on `muster/issue-<key>-<title>`
    /// (reused if it exists) and a session in it. An agent gets the ticket
    /// as its first prompt, marked as data rather than instructions.
    StartFromIssue {
        project: ProjectId,
        key: TicketKey,
        /// A harness id, such as `claude` or `shell`.
        harness: String,
        size: TermSize,
        /// Another agent on the issue, in a worktree of its own, rather
        /// than in the issue's worktree as it is.
        #[serde(default)]
        another: bool,
        /// The workspace whose branch a worktree made for the issue is made on
        /// top of; a worktree found as it is stays as it is.
        #[serde(default)]
        on: Option<WorkspaceId>,
        /// What of the ticket the agent is given: its text and the comments
        /// the person chose; none for its text alone.
        #[serde(default)]
        brief: Option<crate::TicketBrief>,
    },
    /// Ask of the daemon's plugins: list them, preview, install, update,
    /// turn on or off, or remove one (`ARCHITECTURE.md` §7a), or open one's
    /// panel.
    Plugin(plugins::PluginCommand),
    /// Make a space; answered with [`Reply::SpaceCreated`].
    CreateSpace { name: String },
    /// Rename a space.
    RenameSpace { space: SpaceId, name: String },
    /// Remove a space; refused while it holds anything (`ARC-33`).
    DeleteSpace { space: SpaceId },
    /// Move a project or a free terminal to a space, out of the one it was
    /// in; to its own space does nothing.
    MoveToSpace {
        space: SpaceId,
        member: crate::spaces::Member,
    },
    /// Where a terminal's shell is now; answered with [`Reply::Place`].
    ReadPlace { session: SessionId },
    /// File a free terminal under `workspace`, where its shell is in that
    /// workspace's folder: the same shell runs on (`ARC-33`).
    FileSession {
        session: SessionId,
        workspace: WorkspaceId,
    },
    /// Add the folder a free terminal's shell is in as a project, and file
    /// the terminal under its checkout; answered with
    /// [`Reply::ProjectAdded`].
    AddProjectHere {
        session: SessionId,
        space: Option<SpaceId>,
    },
    /// Start a terminal under the same name in `workspace` where `session`
    /// is shown, and close `session`: what ran in it ends. Refused while an
    /// agent runs in it. Answered with [`Reply::SessionStarted`].
    RestartIn {
        session: SessionId,
        workspace: WorkspaceId,
    },
}

/// A successful answer to a [`Command`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
    SpaceCreated {
        id: SpaceId,
    },
    /// Where a terminal's shell is now.
    Place {
        /// Its folder; `None` where it cannot be read.
        folder: Option<PathBuf>,
        /// The workspace whose folder holds it, if one does.
        workspace: Option<WorkspaceId>,
        /// What runs in front in it, as the person would name it.
        front: Option<String>,
    },
    Issues {
        issues: Vec<Issue>,
        /// Where they came from, for the person: `GitHub issues`, `Jira
        /// (acme.atlassian.net)`.
        #[serde(default)]
        tracker: String,
        /// Only those assigned to the person signed in (`[tickets] mine`).
        #[serde(default)]
        mine: bool,
        /// Where they are, the same for every project that lists the same
        /// tickets from a tracker that is no repository's own: a Jira
        /// site's query, a Linear team's issues. `None` for a repository's
        /// own issues (GitHub's, GitLab's). Projects with the same are offered
        /// together.
        #[serde(default)]
        source: Option<String>,
    },
    IssueStarted {
        workspace: WorkspaceId,
        session: SessionId,
    },
    /// An issue, with its text: anyone can write it, so it is data, never
    /// instructions (`PRD-07`).
    IssueViewed {
        issue: Issue,
        body: String,
        /// What was said on it after, oldest first: none from a tracker
        /// that gives none.
        #[serde(default)]
        comments: Vec<crate::IssueComment>,
    },
    RunStarted {
        id: RunId,
    },
    RunHistory {
        transitions: Vec<crate::RunTransition>,
    },
    /// A pull request from a workspace's branch, opened or found open.
    PullRequestOpened {
        pr: crate::PullRequest,
    },
    /// A project's settings that the person can change.
    ProjectSettings(ProjectSettings),
    /// How the daemon is doing inside.
    Health(Health),
    /// Why a session has its status.
    Explanation(Box<crate::explain::Explanation>),
    /// The models an agent offers, as it listed them or as Muster suggests.
    Models(Vec<String>),
    /// Where each coding agent is installed, and its version.
    Agents(Vec<agents::AgentFound>),
    /// The repositories the person can reach on GitHub, the most recently
    /// pushed first.
    Repositories(Vec<repositories::HostedRepository>),
    /// What a workspace's work has changed.
    Changes(Changes),
    /// One changed file's diff.
    FileDiff(FileDiff),
    /// A workspace's timeline.
    Timeline(Timeline),
    /// What an undo would do.
    UndoPlan(UndoPlan),
    /// The files are back; `entry` is the undoing's own entry, from which
    /// it can be undone in turn.
    Undone {
        entry: u64,
        #[serde(default)]
        moved_branch: bool,
    },
    /// The branch moved onto `onto`, its `commits` replayed there; what
    /// was there is entry `entry` of its timeline.
    Moved {
        onto: String,
        commits: u64,
        entry: u64,
    },
    /// The run is the person's; `session` is its agent, in its container.
    RunTakenOver {
        workspace: WorkspaceId,
        session: SessionId,
    },
    /// Every plugin.
    Plugins(plugins::PluginList),
    /// What installing, linking or updating a plugin would do.
    PluginPreview(Box<plugins::PluginPreview>),
    /// The end of a plugin's log.
    PluginLog {
        id: String,
        log: String,
    },
    /// What a plugin's hooks did with a sample event.
    HooksTried(plugins::HooksTried),
    /// What a session shows or its agent last answered, as text; `None` where
    /// there is none: a terminal let go of, no turn ended yet, or an agent
    /// whose hooks do not tell its answer.
    SessionText {
        text: Option<String>,
    },
    /// The command line to run an agent in a terminal with
    /// ([`Command::ExecAgent`]): its program, arguments and the variables to
    /// add.
    AgentCommand {
        program: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// The command was carried out and has nothing to report.
    Done,
}

/// What of a session [`Command::ReadSession`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum Reading {
    /// Its screen as it stands, at its program's size, each line's
    /// trailing blanks and the blank lines under the last taken off.
    Screen,
    /// The answer its agent's last turn ended with, as its hooks told it:
    /// kept while the daemon runs, never stored.
    LastReply,
}

/// A failure the client can act on: `code` is for code, `message` for people.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
