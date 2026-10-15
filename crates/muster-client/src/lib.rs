//! Talking to the daemon: the async framed transport over its Unix socket, and
//! a client connection that performs the handshake. Used by the TUI, the
//! `muster` command, the daemon itself (for its side of the transport) and
//! the end-to-end tests.

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

pub mod connection;
pub mod transport;

pub use connection::{
    ClientError, Connection, FrameReceiver, FrameSender, daemon_identity, daemon_pid,
};
pub use transport::{FramedRead, FramedWrite, TransportError};
