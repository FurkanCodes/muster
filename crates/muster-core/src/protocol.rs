//! The messages between the daemon and its clients (`ARCHITECTURE.md` §6).
//!
//! Versioning rules (`ARC-09`):
//! - every change to these types bumps [`PROTOCOL_VERSION`];
//! - a new optional field carries `#[serde(default)]`;
//! - a new variant is sent only to a peer whose agreed version knows it;
//! - removing or renaming anything also raises [`MIN_PROTOCOL_VERSION`];
//! - the handshake ([`Hello`], [`Welcome`], [`Incompatible`], the error for
//!   a frame before the hello) and the frame header never change shape:
//!   each version's encoding of them is frozen in `tests/frozen/`, and
//!   raising [`PROTOCOL_VERSION`] adds that version's (see
//!   `tests/frozen_protocol.rs`).
//!
//! Until the first release the protocol may change in ways older peers
//! cannot read, but each such change still bumps both versions, so a daemon
//! left running by an older build and a newer client say they do not match
//! instead of misreading each other. Version 2 added restarting and removing
//! sessions; version 3 projects' own checkouts as workspaces, removing
//! projects, mouse modes, and watching that follows the client's focus;
//! version 4 the wait on a harness asking before it starts; version 5 runs;
//! version 6 the person's request for changes on a run; version 7 runs'
//! containers as workspaces, and taking a run over; version 8 handing a
//! person's branch to the autopilot; version 9 the checks by hand; version
//! 10 a review and a draft pull request by hand; version 11 a ticket's
//! text, and another agent on a ticket; version 12 closing a ticket; version
//! 13 more harnesses, and a wait their own display reports (`WaitReason::Action`);
//! version 14 tickets by key from more trackers; version 15 the settings a
//! person changes on the Settings screen, and whether a listing of tickets
//! has only theirs; version 16 when each session came to its status, and
//! what it is doing now.
//!
//! Since the first release, a change keeps older peers working where it
//! can: version 17 (from 16) is where a project's tickets are chosen on the
//! Settings screen (`SettingChange::TicketsFrom`, sent only to a daemon of
//! 17 or later), and the settings saying where they are written; version
//! 18 the pull request from each workspace's branch
//! (`Workspace::pull_request`, an optional field older clients pass over);
//! version 19 what each session's agent has used (`Session::spend`, the
//! same); version 20 whether a project's branches start with `muster/`
//! (`SettingChange::MusterPrefix`, sent only to a daemon of 20 or later, and
//! `ProjectSettings::muster_prefix`); version 21 where a listing's tickets
//! are, so projects sharing them are offered together
//! (`Reply::Issues::source`); version 22 how the daemon is doing inside,
//! for `muster status` (`Command::Health`, sent only to a daemon of 22 or
//! later); version 23 the notch: whether the person looks at any client
//! (`ClientFrame::Looking`, sent only to a daemon of 23 or later;
//! `Snapshot::looked_at`, and `Delta::LookedAt`, sent only to a client of
//! 23 or later), showing a session in the client looked at last
//! (`Command::Reveal`, `ServerFrame::Reveal`), and Muster's own settings
//! (`Snapshot::preferences`, `Delta::Preferences`,
//! `Command::ChangePreference`); version 24 what a workspace's work has
//! changed (`Workspace::changes`, an optional field older clients pass
//! over; `Command::WorkspaceChanges` and `Command::FileDiff`, sent only to
//! a daemon of 24 or later); version 25 each workspace's timeline of turns
//! and putting its files back from one (`Workspace::timeline`, the same;
//! `Command::Timeline`, `Command::EntryDiff`, `Command::UndoPlan` and
//! `Command::Undo`, sent only to a daemon of 25 or later); version 26 where
//! the work in a project's workspaces meets (`Workspace::clashes`, a field
//! older clients pass over); version 27 tidying a branch's commits
//! (`Command::TidyCommits`, sent only to a daemon of 27 or later, and
//! `Session::tidy`, a field older clients pass over); version 28 stacked
//! work (`Workspace::stacked_on`, the same; `CreateWorktree::on` and
//! `StartFromIssue::on`, and `Command::MoveOnto`, sent only to a daemon of
//! 28 or later; a timeline's entries of `EntryKind::Moved` reach an older
//! client as `EntryKind::Between`, [`Reply::for_version`]); version 29 the
//! coding agents the person added in Muster's own settings
//! (`Preferences::agents`, and why those settings do not read,
//! `Preferences::problem`, fields older clients pass over), and reading
//! those settings again (`Command::ReadPreferences`, sent only to a daemon
//! of 29 or later); version 30 why a session has its status
//! (`Command::ExplainSession`, sent only to a daemon of 30 or later,
//! answered with `Reply::Explanation`); version 31 a
//! session's terminal as in a real one: the colours of the person's
//! terminal, which programs in sessions are told when they ask
//! (`ClientFrame::Colours`, sent only to a daemon of 31 or later), and
//! which session has the person's keys, for programs that asked to be
//! told as their terminal gains and loses focus (`ClientFrame::Keys`, the
//! same), and the size of a session's terminal in pixels
//! (`ClientFrame::Resize::pixels`, a field older daemons pass over);
//! version 32 the width from which a session sits beside the list
//! (`Preferences::side_by_side_from`, a field older clients pass over;
//! `PreferenceChange::SideBySideFrom`, sent only to a daemon of 32 or
//! later); version 33 where a replay's output was written while the client
//! was attached (`ServerFrame::Replay::live_from`, a field older clients
//! pass over), so a program's copy there reaches the clipboard; version 34
//! chats named by their first prompt and renamed by the person
//! (`Command::RenameSession`, sent only to a daemon of 34 or later);
//! version 35 the UI's look, whose colours it draws in and how many lines a
//! session's line takes (`Preferences::palette` and
//! `Preferences::session_lines`, fields older clients pass over;
//! `PreferenceChange::Palette` and `PreferenceChange::SessionLines`, sent
//! only to a daemon of 35 or later); version 36 where the UI's panels are
//! and whether its list is folded away (`Preferences::panels` and
//! `Preferences::list_folded`, fields older clients pass over;
//! `PreferenceChange::Panels` and `PreferenceChange::ListFolded`, sent only
//! to a daemon of 36 or later); version 37 chats named by an agent
//! (`Preferences::naming` and `AddedAgent::asks`, fields older clients pass
//! over; `PreferenceChange::NamesByAgent` and `PreferenceChange::NamedBy`,
//! and `Command::ListModels`, answered with `Reply::Models`, sent only to a
//! daemon of 37 or later); version 38 the person's own keys
//! (`Preferences::keys`, a field older clients pass over;
//! `PreferenceChange::Keys` and `PreferenceChange::AllKeysBack`, sent only
//! to a daemon of 38 or later); version 39 notifications of the Mac's own
//! (`Preferences::notifications` and `Snapshot::notifier`, fields older
//! clients pass over; `ClientFrame::Notifier`, `PreferenceChange::
//! Notifications`, sent only to a daemon of 39 or later; `Delta::Notifier`,
//! sent only to a client of 39 or later); version 40 which notifications
//! and their sounds (`Preferences::telling`, a field older clients pass
//! over; `PreferenceChange::Tell`, sent only to a daemon of 40 or later);
//! version 41 plugins (`Command::Plugin`, sent only to a daemon of 41 or
//! later, answered with `Reply::Plugins` and `Reply::PluginPreview`;
//! `Snapshot::plugins`, a field older clients pass over; and
//! `Delta::PluginsChanged`, sent only to a client of 41 or later); version
//! 42 tickets through plugins: a project's tracker is a ticket plugin's id
//! with what its `[tickets]` says for it (`TicketsFrom::tracker` and
//! `TicketsFrom::fields`, with Jira's and Linear's keys written beside
//! them as older peers read them, sent only to a daemon of 42 or later; a
//! project's settings naming anyone else's plugin reach an older client
//! without it, [`Reply::for_version`]), and a plugin's log
//! (`PluginCommand::Log`, sent only to a daemon of 42 or later, answered
//! with `Reply::PluginLog`); version
//! 43 plugins' panels (`PluginCommand::OpenPanel`, sent only to a daemon
//! of 43 or later; `Session::beside`, a field older clients pass over; an
//! older client is sent no panel's session, and a layout and keys without
//! a panel's, [`Snapshot::for_version`] and [`Delta::for_version`]);
//! version 44 plugins' hooks (`PluginDetails::hooks`, a field older
//! clients pass over, and a preview with hooks refused to them,
//! `plugins::shown_to`; `PluginCommand::TryHooks`, sent only to a daemon
//! of 44 or later, answered with `Reply::HooksTried`); version 45 where
//! each coding agent is installed and its version (`Command::Agents`, sent
//! only to a daemon of 45 or later, answered with `Reply::Agents`); version
//! 46 the screen a replay that starts over ends on, drawn whole
//! (`ServerFrame::Replay::screen`, a field older clients pass over);
//! version 47 a Rust project's worktrees building into one folder
//! (`Preferences::shared_builds`, a field older clients pass over;
//! `PreferenceChange::SharedBuilds`, sent only to a daemon of 47 or
//! later); version 48 the ticket a worktree was made for
//! (`Workspace::ticket`, a field older clients pass over); version 49
//! cloning a project from GitHub (`Command::ListRepositories` and
//! `Command::CloneProject`, sent only to a daemon of 49 or later, answered
//! with `Reply::Repositories` and `Reply::ProjectAdded`).
//!
//! Frames are encoded with named fields, so a field can be added to any
//! message, including [`Hello`], without breaking older peers.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub mod agents;
mod older;
pub mod plugins;
pub mod repositories;
mod terminal;

pub use terminal::{
    DetachReason, MouseEncoding, MouseTracking, PixelSize, TermSize, TerminalModes,
};

use crate::changes::{ChangePart, Changes, FileDiff};
use crate::colours::TerminalColours;
use crate::entities::{Issue, Project, Session, Workspace};
use crate::ids::{ProjectId, RunId, SessionId, WorkspaceId};
use crate::session::SessionStatus;
use crate::settings::{PreferenceChange, Preferences, ProjectSettings, SettingChange};
use crate::tickets::TicketKey;
use crate::timeline::{Timeline, UndoPlan};

/// The newest protocol this build speaks.
pub const PROTOCOL_VERSION: u32 = 49;
/// The oldest protocol this build still speaks.
pub const MIN_PROTOCOL_VERSION: u32 = 16;

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
        /// Its size in pixels, where the person's terminal says its own:
        /// for programs that size pictures or a cursor by it. From protocol
        /// 31; older daemons pass it over.
        #[serde(default)]
        pixels: Option<PixelSize>,
    },
    /// Whether the person is looking at this client's window: a UI says
    /// so as it connects and whenever its window gains or loses focus. A
    /// client that never says is not looked at. Sent only to a daemon of
    /// 23 or later.
    Looking { looking: bool },
    /// The colours of the person's terminal, as the UI found them as it
    /// started: a program in a session asking its terminal for its colours
    /// (OSC 10, 11 and 4) is told these, the last a UI said. Sent only to a
    /// daemon of 31 or later, and only with a colour in it.
    Colours(TerminalColours),
    /// Which session's terminal is in focus in this client, if any: the
    /// one shown, beside the list or filling the window, in a window the
    /// person looks at, whether or not it has the keys. A program that
    /// asked to be told its terminal's focus (`CSI ? 1004 h`) is sent
    /// `CSI I` as its session comes into focus in any client and `CSI O`
    /// as it leaves it in every one. Sent whenever it changes, only to a
    /// daemon of 31 or later.
    Keys { session: Option<SessionId> },
    /// Muster's helper on a Mac can show notifications of the Mac's own,
    /// the person having allowed them, or no longer can. Sent only to a
    /// daemon of 39 or later.
    Notifier { ready: bool },
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
        /// Protocol 33; an older daemon says nothing, read as `None`.
        #[serde(default)]
        live_from: Option<u64>,
        /// With `reset`, the program's screen as it stands at the end of
        /// `data`, drawn whole: the main screen or the alternate one, every
        /// cell, the cursor, the colours being drawn in and the input modes,
        /// as escape sequences. `data`, cut from the middle of the output
        /// once the daemon no longer holds its start, cannot draw it on its
        /// own: a client draws this after it, `data` then being history for
        /// its scrollback. It takes no place in the output. Protocol 46; an
        /// older daemon says nothing, read as `None`.
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
        /// The workspace whose branch a new branch is made on top of,
        /// rather than on what the checkout has. Sent only to a daemon of
        /// 28 or later.
        #[serde(default)]
        on: Option<WorkspaceId>,
    },
    /// Remove a worktree's directory. Its branch is kept. A worktree with
    /// uncommitted changes is removed only with `force`; one with running
    /// sessions never is, nor a project's own checkout.
    RemoveWorktree { workspace: WorkspaceId, force: bool },
    /// Start a session in a workspace.
    StartSession {
        workspace: WorkspaceId,
        /// A harness id: `shell`, `claude`, or an agent the person added
        /// ([`Preferences::agents`]).
        harness: String,
        title: Option<String>,
        size: TermSize,
        /// A first prompt, for harnesses that take one.
        #[serde(default)]
        prompt: Option<String>,
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
    /// Move a workspace's branch onto the newest commit of the branch it
    /// is stacked on, or of its base once that has merged, or of its base
    /// where it is not stacked, as the person asked (`PRD-14`): its own
    /// commits replayed there, and nothing changed where one would
    /// conflict; refused while an agent works there. Answered with
    /// [`Reply::Moved`]. Sent only to a daemon of 28 or later.
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
    /// Sent only to a daemon of 34 or later.
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
    /// What a workspace's work has changed: what is not committed, and
    /// the branch's commits since it left the branch it is to merge into,
    /// with their files; answered with [`Reply::Changes`]. Sent only to a
    /// daemon of 24 or later.
    WorkspaceChanges { workspace: WorkspaceId },
    /// One changed file's diff, in one part of a workspace's change;
    /// answered with [`Reply::FileDiff`]. Sent only to a daemon of 24 or
    /// later.
    FileDiff {
        workspace: WorkspaceId,
        part: ChangePart,
        path: String,
    },
    /// A workspace's timeline: each agent's turn there, what changed
    /// between them, and each undoing, newest first; answered with
    /// [`Reply::Timeline`]. Sent only to a daemon of 25 or later.
    Timeline { workspace: WorkspaceId },
    /// What one entry of a workspace's timeline changed, as a diff;
    /// answered with [`Reply::FileDiff`]. Sent only to a daemon of 25 or
    /// later.
    EntryDiff { workspace: WorkspaceId, number: u64 },
    /// What putting the files back as they were before an entry would do;
    /// answered with [`Reply::UndoPlan`]. Sent only to a daemon of 25 or
    /// later.
    UndoPlan { workspace: WorkspaceId, number: u64 },
    /// Put a workspace's files back as they were before an entry, as the
    /// person asked, and with `move_branch` its branch too where it can
    /// go (`PRD-14`); refused while an agent works there. Answered with
    /// [`Reply::Undone`]. Sent only to a daemon of 25 or later.
    Undo {
        workspace: WorkspaceId,
        number: u64,
        #[serde(default)]
        move_branch: bool,
    },
    /// How the daemon is doing inside, answered with [`Reply::Health`].
    /// Sent only to a daemon of protocol 22 or later.
    Health,
    /// Why a session has its status: what its machine holds and the
    /// signals it last received; answered with [`Reply::Explanation`].
    /// Sent only to a daemon of 30 or later.
    ExplainSession { session: SessionId },
    /// Show a session in the Muster window the person looked at last
    /// ([`ServerFrame::Reveal`]); refused when no window is open. Sent
    /// only to a daemon of 23 or later.
    Reveal { session: SessionId },
    /// Change one of Muster's own settings, in the person's own file;
    /// answered with [`Reply::Done`], every client told by
    /// [`Delta::Preferences`]. Sent only to a daemon of 23 or later.
    ChangePreference(PreferenceChange),
    /// Read Muster's own settings again, after the person edited them by
    /// hand: the agents they added among them. Answered with
    /// [`Reply::Done`], every client told by [`Delta::Preferences`]. Sent
    /// only to a daemon of 29 or later.
    ReadPreferences,
    /// The models the coding agent `agent` (a harness's id) offers for
    /// naming chats: those it lists, where it can, else those Muster
    /// suggests for it. Answered with [`Reply::Models`]. Sent only to a
    /// daemon of 37 or later.
    ListModels { agent: String },
    /// Where each coding agent Muster can start is installed, on the
    /// daemon's own `PATH`, and the version it says it is: Muster's own and
    /// those the person added, each program run with `--version`, never
    /// anything else. Answered with [`Reply::Agents`]. Sent only to a
    /// daemon of 45 or later ([`agents::AGENTS_FOUND_PROTOCOL`]).
    Agents,
    /// The repositories the person can reach on GitHub, through their own
    /// `gh`, to clone one as a project. Answered with
    /// [`Reply::Repositories`]. Sent only to a daemon of 49 or later
    /// ([`repositories::REPOSITORIES_PROTOCOL`]).
    ListRepositories,
    /// Clone `repository` (`owner/name`, or its github.com address) into a
    /// folder of its name in `into`, through the person's own `gh`, and add
    /// it as a project. Answered with [`Reply::ProjectAdded`] once cloned.
    /// Sent only to a daemon of 49 or later.
    CloneProject { repository: String, into: PathBuf },
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
        /// The workspace whose branch a worktree made for the issue is
        /// made on top of; a worktree found as it is stays as it is. Sent
        /// only to a daemon of 28 or later.
        #[serde(default)]
        on: Option<WorkspaceId>,
    },
    /// Ask of the daemon's plugins: list them, preview, install, update,
    /// turn on or off, or remove one (`ARCHITECTURE.md` §7a), or open one's
    /// panel. Sent only to a daemon of 41 or later; `OpenPanel` only to one
    /// of 43 or later.
    Plugin(plugins::PluginCommand),
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
        /// own issues (GitHub's, GitLab's), and from a daemon before
        /// protocol 21. Projects with the same are offered together.
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
    /// The models an agent offers, as it listed them or as Muster
    /// suggests. From protocol 37.
    Models(Vec<String>),
    /// Where each coding agent is installed, and its version. From
    /// protocol 45.
    Agents(Vec<agents::AgentFound>),
    /// The repositories the person can reach on GitHub, the most recently
    /// pushed first. From protocol 49.
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
    /// Every plugin. From protocol 41.
    Plugins(plugins::PluginList),
    /// What installing, linking or updating a plugin would do. From
    /// protocol 41.
    PluginPreview(Box<plugins::PluginPreview>),
    /// The end of a plugin's log. From protocol 42.
    PluginLog {
        id: String,
        log: String,
    },
    /// What a plugin's hooks did with a sample event. From protocol 44.
    HooksTried(plugins::HooksTried),
    /// The command was carried out and has nothing to report.
    Done,
}

/// What the daemon holds and has under way: counts that should fall back
/// when the work and the clients behind them end (`ARC-13`, `ARC-14`), so
/// one that only grows is a leak to look into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Health {
    /// Every task on the daemon's runtime.
    pub tasks: u64,
    /// Short-lived work under way, such as a call to a ticket source; a
    /// client's request for this is one.
    pub work: u64,
    /// Clients connected, this one included.
    pub clients: u64,
    /// Terminals held: running sessions, and ended ones whose output is
    /// still readable.
    pub terminals: u64,
}

/// The daemon's whole state, as a client needs it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Snapshot {
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    #[serde(default)]
    pub runs: Vec<crate::Run>,
    /// The latest measurement, once one has been taken.
    #[serde(default)]
    pub resources: Option<ResourceReport>,
    /// Whether the person is looking at any client's window
    /// ([`ClientFrame::Looking`]). From protocol 23.
    #[serde(default)]
    pub looked_at: bool,
    /// Muster's own settings. From protocol 23.
    #[serde(default)]
    pub preferences: Preferences,
    /// Whether Muster's helper on a Mac shows notifications of the Mac's
    /// own now ([`ClientFrame::Notifier`]): the UIs then send their
    /// terminals none. From protocol 39.
    #[serde(default)]
    pub notifier: bool,
    /// Every plugin. From protocol 41.
    #[serde(default)]
    pub plugins: Vec<plugins::PluginInfo>,
}

/// What the machine and each running session use, measured every couple of
/// seconds (`PRD-05`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ResourceReport {
    pub host_memory_used: u64,
    pub host_memory_total: u64,
    pub sessions: Vec<SessionUsage>,
}

/// One session's use: its program and every process it started.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SessionUsage {
    pub session: SessionId,
    pub memory_bytes: u64,
    /// CPU in thousandths of one core: 1500 is one and a half cores.
    pub cpu_permille: u32,
}

/// One change to the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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
        /// When it came to this status.
        since: crate::Timestamp,
    },
    /// What a session is doing now changed; `None` once it is doing
    /// nothing to tell.
    SessionActivity {
        session: SessionId,
        activity: Option<crate::session::Activity>,
    },
    /// A new measurement; replaces the last.
    Resources(ResourceReport),
    /// A run was picked up or changed.
    RunUpserted(crate::Run),
    /// The person began or stopped looking at every client's window.
    /// Sent only to a client of 23 or later.
    LookedAt {
        looked_at: bool,
    },
    /// Muster's own settings changed. Sent only to a client of 23 or later.
    Preferences(Preferences),
    /// Muster's helper shows notifications now, or no longer does. Sent
    /// only to a client of 39 or later.
    Notifier {
        ready: bool,
    },
    /// The plugins changed: each, as it is now. Sent only to a client of
    /// 41 or later.
    PluginsChanged(Vec<plugins::PluginInfo>),
}

impl Reply {
    /// The reply as a client that agreed to `version` can read it: a
    /// timeline's entries of kinds it does not know are told as edits
    /// between turns, and a project's tickets from a plugin not Muster's
    /// own are left out of its settings, as in words they are still said
    /// (`ARC-09`).
    #[must_use]
    pub fn for_version(self, version: u32) -> Self {
        match self {
            Self::ProjectSettings(mut settings)
                if version < crate::plugins::TICKET_PLUGINS_PROTOCOL =>
            {
                if settings
                    .tickets
                    .as_ref()
                    .is_some_and(|from| crate::plugins::own::manifest(&from.tracker).is_none())
                {
                    settings.tickets = None;
                }
                Self::ProjectSettings(settings)
            }
            Self::Timeline(mut timeline) if version < crate::stacks::STACKS_PROTOCOL => {
                for entry in &mut timeline.entries {
                    if matches!(entry.kind, crate::timeline::EntryKind::Moved { .. }) {
                        entry.kind = crate::timeline::EntryKind::Between;
                    }
                }
                Self::Timeline(timeline)
            }
            other => other,
        }
    }
}

impl Delta {
    /// The first protocol that knows this delta: a client that agreed to
    /// an older one is never sent it (`ARC-09`).
    pub fn since(&self) -> u32 {
        match self {
            Self::LookedAt { .. } | Self::Preferences(_) => crate::settings::NOTCH_PROTOCOL,
            Self::Notifier { .. } => crate::settings::NOTIFIER_PROTOCOL,
            Self::PluginsChanged(_) => crate::plugins::PLUGINS_PROTOCOL,
            _ => FIRST_PROTOCOL_VERSION,
        }
    }
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
