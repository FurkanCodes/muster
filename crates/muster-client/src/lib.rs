//! Talking to the daemon: the async framed transport over its Unix socket, and
//! a client connection that performs the handshake. Used by the TUI, the
//! `muster` command, the daemon itself (for its side of the transport) and
//! the end-to-end tests.
