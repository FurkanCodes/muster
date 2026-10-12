//! Types shared by every Muster crate: identifiers, the entities the daemon
//! stores, the protocol, and its framing.
//!
//! This crate does no IO (`ARC-01`).

pub mod config;
pub mod credentials;
pub mod entities;
pub mod frame;
pub mod html;
pub mod ids;
pub mod paths;
pub mod protocol;
pub mod remote;
pub mod run;
pub mod session;
pub mod settings;
pub mod text;
pub mod tickets;
pub mod time;

pub use entities::{Issue, Project, Session, Workspace, WorkspaceKind};
pub use ids::{ParseIdError, ProjectId, RunId, SessionId, WorkspaceId};
pub use paths::Paths;
pub use run::{PullRequest, Run, RunStage, RunTransition, Ticket};
pub use session::{Activity, SessionStatus, WaitReason};
pub use tickets::TicketKey;
pub use time::Timestamp;
