//! The Muster terminal UI. It renders what the daemon reports and sends the
//! person's commands back; it holds no business logic (`ARCHITECTURE.md` §3).
//!
//! The parts: `model` mirrors the daemon's state, `app` is the state
//! machine that turns keys and frames into frames to send, `view` draws it
//! (`motion` the little that moves),
//! `screen` and `keys` handle session terminals, and `runtime` connects it
//! all to the person's terminal and the daemon.

// No unsafe code here, ever: it lives in one audited daemon module (`ARC-12`).
#![forbid(unsafe_code)]

mod app;
mod clipboard;
mod colours;
mod folders;
mod input;
mod keys;
mod model;
mod motion;
mod mouse;
mod runtime;
mod screen;
mod view;

pub use runtime::{Options, START_AGAIN_AFTER, START_WAIT, TuiError, run};
