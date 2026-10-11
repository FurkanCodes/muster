//! The Muster terminal UI. It renders what the daemon reports and sends the
//! person's commands back; it holds no business logic (`ARCHITECTURE.md` §3).
//!
//! The parts: `model` mirrors the daemon's state, `app` is the state
//! machine that turns keys and frames into frames to send, `view` draws it,
//! `screen` and `keys` handle session terminals, and `runtime` connects it
//! all to the person's terminal and the daemon.

mod app;
mod input;
mod keys;
mod model;
mod mouse;
mod runtime;
mod screen;
mod view;

pub use runtime::{Options, TuiError, run};
