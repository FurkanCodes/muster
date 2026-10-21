//! What crosses between the app's Rust side and its webview (`ARC-24`).
//!
//! The webview draws; it never works out how the state stands. The Rust side
//! keeps the daemon's state (the model the terminal UI keeps too) and sends
//! the webview patches of whole entities with what the model derives from
//! them, so the two apps agree on what needs the person and in what order.
//!
//! Every type here is exported to TypeScript (`src/ipc/generated/desktop`)
//! by `cargo test export_bindings`; `pnpm bindings` runs it and CI checks the
//! files are current.

use std::collections::BTreeMap;

use muster_core::protocol::plugins::PluginInfo;
use muster_core::protocol::{DetachReason, ErrorCode, ResourceReport, TerminalModes};
use muster_core::settings::Preferences;
use muster_core::{Project, ProjectId, Session, SessionId, Timestamp, Workspace, WorkspaceId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Pushed, in order, on the channel the webview opens with `link_open`.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "desktop/")]
pub enum StateMessage {
    /// The link to the daemon changed.
    Link { link: LinkState },
    /// Entities that changed, those that went, and what the model now
    /// derives. With `reset`, everything the webview holds is replaced: the
    /// first patch after connecting, and after falling behind.
    Patch { patch: Box<Patch> },
    /// The person picked a session elsewhere (the notch, a notification):
    /// open it with the keys.
    Reveal { session: SessionId },
}

/// How the link to the daemon stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "desktop/")]
pub enum LinkState {
    /// Looking for the daemon, or starting it.
    Connecting,
    /// Connected and subscribed.
    Up {
        /// The daemon's version, as it says it.
        version: String,
        /// The protocol both agreed on.
        protocol: u32,
    },
    /// Out of reach; trying again every half second.
    Down {
        /// Why, in words the person can act on.
        reason: String,
        /// Since when, by the wall clock.
        since: Timestamp,
    },
}

/// A change to the state, as whole entities.
#[derive(Clone, Debug, Default, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub struct Patch {
    /// Replace everything held with this patch's contents.
    pub reset: bool,
    pub projects: Vec<Project>,
    pub workspaces: Vec<Workspace>,
    pub sessions: Vec<Session>,
    pub gone: Gone,
    /// Muster's own settings, when they changed.
    pub preferences: Option<Preferences>,
    /// Every plugin, when any changed.
    pub plugins: Option<Vec<PluginInfo>>,
    /// The latest measurement, when a new one came.
    pub resources: Option<ResourceReport>,
    /// Whether Muster's helper shows the Mac's notifications, when it changed.
    pub notifier: Option<bool>,
    /// What the shared model derives, after this patch.
    pub derived: Derived,
}

/// Entities that left the state.
#[derive(Clone, Debug, Default, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub struct Gone {
    pub projects: Vec<ProjectId>,
    pub workspaces: Vec<WorkspaceId>,
    pub sessions: Vec<SessionId>,
}

/// What the model works out from the state, so the webview never does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub struct Derived {
    /// Sessions waiting on the person: asking first, then news not seen,
    /// oldest first within each (the terminal UI's *Needs you*).
    pub needs_you: Vec<SessionId>,
    /// The sessions list: each project, its workspaces, their sessions, in
    /// the order the terminal UI lists them.
    pub rows: Vec<Row>,
    /// The flock, as the title bar counts it.
    pub counts: Counts,
    /// What each session asking the person shows last: its screen's last
    /// four lines with anything on them, oldest first, control characters
    /// taken out. Only while the window is seen, and only for sessions
    /// asking (status `needs_you`) whose screen has come.
    pub questions: BTreeMap<SessionId, Vec<String>>,
}

/// One line of the sessions list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[ts(export, export_to = "desktop/")]
pub enum Row {
    Project(ProjectId),
    Workspace(WorkspaceId),
    Session(SessionId),
}

/// How many agents work, wait on the person and are done, as the terminal
/// UI's top bar counts them (`muster_model::Counts`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, TS)]
#[ts(export, export_to = "desktop/")]
pub struct Counts {
    pub working: u32,
    pub waiting: u32,
    pub done: u32,
}

/// A terminal stream's frames, on the channel `attach` opens, as raw bytes:
/// `[kind: u8][seq: u64 little-endian][payload]`. Output never goes
/// through JSON (`ARC-25`).
pub mod frame {
    /// Output that follows; the payload is the program's bytes.
    pub const OUTPUT: u8 = 0;
    /// The stream (re)starts from `seq`; the payload is the bytes held.
    pub const REPLAY: u8 = 1;
    /// As [`REPLAY`], but the screen must be cleared first.
    pub const REPLAY_RESET: u8 = 2;
    /// A [`super::TermEvent`] as JSON; `seq` is 0.
    pub const EVENT: u8 = 9;

    /// The length of a frame's header: its kind and its `seq`.
    pub const HEADER: usize = 9;

    /// One frame, as the webview reads it (`src/ipc/frames.ts`).
    pub fn encode(kind: u8, seq: u64, payload: &[u8]) -> Vec<u8> {
        let mut frame = Vec::with_capacity(HEADER + payload.len());
        frame.push(kind);
        frame.extend_from_slice(&seq.to_le_bytes());
        frame.extend_from_slice(payload);
        frame
    }
}

/// What happens to a stream besides its bytes, sent as a [`frame::EVENT`].
#[derive(Clone, Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "desktop/")]
pub enum TermEvent {
    /// How the program wants its keys and pastes encoded.
    Modes { modes: TerminalModes },
    /// The stream ended: the program exited and its output is all sent
    /// (`exited`; a new program in the session streams on, from a reset),
    /// or the session holds no terminal (`not_running`; the link attaches
    /// again should it run again).
    Ended { reason: DetachReason },
}

/// Why a command from the webview failed.
#[derive(Clone, Debug, Serialize, TS, thiserror::Error)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "desktop/")]
pub enum BridgeError {
    /// The daemon is out of reach.
    #[error("Muster's daemon is out of reach")]
    Unreachable,
    /// The daemon refused, with its own words and code.
    #[error("{message}")]
    Refused { code: ErrorCode, message: String },
    /// Something the webview sent did not make sense here.
    #[error("{message}")]
    Invalid { message: String },
}
