//! The Muster daemon: owns every session and run, and is the only process that
//! talks to agents, Docker, GitHub and ticket sources (`ARCHITECTURE.md` §4).

pub mod store;
