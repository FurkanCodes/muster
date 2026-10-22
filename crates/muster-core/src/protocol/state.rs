//! The state a client is sent: the whole of it once ([`Snapshot`]), then
//! each change to it ([`Delta`]), and how the daemon is doing inside.

use serde::{Deserialize, Serialize};

use super::{FIRST_PROTOCOL_VERSION, plugins};
use crate::entities::{Project, Session, Workspace};
use crate::ids::{ProjectId, SessionId, WorkspaceId};
use crate::session::SessionStatus;
use crate::settings::Preferences;

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
    /// ([`super::ClientFrame::Looking`]). From protocol 23.
    #[serde(default)]
    pub looked_at: bool,
    /// Muster's own settings. From protocol 23.
    #[serde(default)]
    pub preferences: Preferences,
    /// Whether Muster's helper on a Mac shows notifications of the Mac's
    /// own now ([`super::ClientFrame::Notifier`]): the UIs then send their
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
