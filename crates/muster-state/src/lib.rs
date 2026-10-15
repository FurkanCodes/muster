//! Muster's pure state machines. Each one takes a signal and the current time
//! and returns the effects for the daemon to carry out.
//!
//! This crate does no IO and keeps no clock of its own (`ARC-01`).

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

pub mod run;
pub mod session;
