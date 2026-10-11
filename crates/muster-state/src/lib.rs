//! Muster's pure state machines. Each one takes a signal and the current time
//! and returns the effects for the daemon to carry out.
//!
//! This crate does no IO and keeps no clock of its own (`ARC-01`).

pub mod session;
