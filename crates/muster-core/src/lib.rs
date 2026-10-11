//! Types shared by every Muster crate: identifiers and the entities the daemon
//! stores and sends to clients.
//!
//! This crate does no IO (`ARC-01`).

pub mod ids;
pub mod session;

pub use ids::{ParseIdError, ProjectId, RunId, SessionId, WorkspaceId};
pub use session::{SessionStatus, WaitReason};
