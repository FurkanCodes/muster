//! Types shared by every Muster crate: identifiers, the entities the daemon
//! stores, the protocol, and its framing.
//!
//! This crate does no IO (`ARC-01`).

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

pub mod changes;
pub mod clashes;
pub mod config;
pub mod credentials;
pub mod entities;
pub mod explain;
pub mod frame;
pub mod hex;
pub mod html;
pub mod ids;
pub mod num;
pub mod paths;
pub mod protocol;
pub mod pulls;
pub mod remote;
pub mod run;
pub mod session;
pub mod settings;
pub mod stacks;
pub mod text;
pub mod tickets;
pub mod tidy;
pub mod time;
pub mod timeline;

pub use changes::{ChangeKind, ChangePart, ChangeSummary, Changes, FileChange, FileDiff};
pub use entities::{Issue, Project, Session, Workspace, WorkspaceKind};
pub use ids::{ParseIdError, ProjectId, RunId, SessionId, WorkspaceId};
pub use paths::Paths;
pub use pulls::{Checks, PullRequestState, PullRequestStatus, Review};
pub use run::{PullRequest, Run, RunStage, RunTransition, Ticket};
pub use session::{Activity, SessionStatus, Spend, WaitReason};
pub use tickets::TicketKey;
pub use time::Timestamp;
