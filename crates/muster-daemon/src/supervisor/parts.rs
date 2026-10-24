//! The Supervisor's state in parts, each a group of what goes together:
//! the folders it keeps things in, the systems outside the daemon it
//! reaches through their traits (`ARC-03`), what the clients connected to
//! it hold, and the looks it takes at workspaces in the background.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use muster_core::SessionId;
use tokio::sync::{mpsc, watch};

use super::{changes, clashes, looking, pulls, sizing};
use crate::githost::choose::Hosts;
use crate::pty::PtyHandle;
use crate::runs::RunWork;
use crate::runs::credentials::Credentials;
use crate::trackers::Trackers;

/// Where the daemon keeps things.
#[derive(Debug)]
pub(super) struct Paths {
    pub(super) worktrees_dir: PathBuf,
    pub(super) data_dir: PathBuf,
    pub(super) hook_socket: PathBuf,
    pub(super) config_dir: PathBuf,
}

/// The systems outside the daemon, through their traits (`ARC-03`).
#[derive(Debug)]
pub(super) struct Systems {
    /// Runs' work in their containers.
    pub(super) work: Arc<dyn RunWork>,
    pub(super) trackers: Arc<dyn Trackers>,
    pub(super) credentials: Arc<dyn Credentials>,
    /// The `docker` command, for sessions in runs' containers.
    pub(super) docker: String,
    /// Each project's git host.
    pub(super) hosts: Arc<dyn Hosts>,
    /// GitHub's repositories, to clone one as a project.
    pub(super) repositories: Arc<dyn crate::repositories::Repositories>,
    /// Where the agents keep their records.
    pub(super) places: crate::spend::Places,
    /// Where plugins' secrets are kept.
    pub(super) secrets: Arc<dyn crate::secrets::Secrets>,
}

/// What the connected clients hold, and where what they let go of reports.
#[derive(Debug)]
pub(super) struct Clients {
    /// Each session's current terminal, followed by its clients across
    /// relaunches.
    pub(super) terminals: HashMap<SessionId, watch::Sender<Option<PtyHandle>>>,
    /// How many clients watch each watched session.
    pub(super) watchers: HashMap<SessionId, usize>,
    /// Where dropped watch guards report.
    pub(super) unwatched: mpsc::UnboundedSender<(SessionId, u64)>,
    /// How many clients have the person's keys in each session's terminal.
    pub(super) typing: HashMap<SessionId, usize>,
    /// Where dropped keys guards report.
    pub(super) keys_gone: mpsc::UnboundedSender<SessionId>,
    /// The colours of the person's terminal, as a UI last told them; every
    /// terminal answers its program's questions about its colours from
    /// them.
    pub(super) colours: watch::Sender<muster_core::colours::TerminalColours>,
    /// The clients that say whether the person is looking at them.
    pub(super) lookers: looking::Lookers,
    /// Whether the person looks at any, for the watchers that slow down
    /// while nobody does.
    pub(super) looked_at: watch::Sender<bool>,
    /// The sizes clients gave sessions' terminals.
    pub(super) sizing: sizing::Sizing,
    /// The key the last connection was given.
    pub(super) next_client: u64,
    /// Where dropped client guards report.
    pub(super) client_gone: mpsc::UnboundedSender<u64>,
}

impl Clients {
    pub(super) fn new(
        unwatched: mpsc::UnboundedSender<(SessionId, u64)>,
        keys_gone: mpsc::UnboundedSender<SessionId>,
        client_gone: mpsc::UnboundedSender<u64>,
    ) -> Self {
        Self {
            terminals: HashMap::new(),
            watchers: HashMap::new(),
            unwatched,
            typing: HashMap::new(),
            keys_gone,
            colours: watch::channel(muster_core::colours::TerminalColours::default()).0,
            lookers: looking::Lookers::default(),
            looked_at: watch::channel(false).0,
            sizing: sizing::Sizing::default(),
            next_client: 0,
            client_gone,
        }
    }
}

/// The looks taken at workspaces in the background.
#[derive(Debug, Default)]
pub(super) struct Looks {
    /// At branches' pull requests.
    pub(super) pulls: pulls::Looks,
    /// At what workspaces' work has changed.
    pub(super) changes: changes::Looks,
    /// At where the work in projects' workspaces meets.
    pub(super) clashes: clashes::Clashes,
}
