//! Where each coding agent Muster can start is installed, and the version
//! it says it is (`PRODUCT.md` §6, *your agents*): the daemon's answer, as
//! it is the daemon that runs them, with its own `PATH`. From protocol 45
//! ([`AGENTS_FOUND_PROTOCOL`]).

use serde::{Deserialize, Serialize};

/// The first protocol whose daemon answers [`super::Command::Agents`]; a
/// client sends it only to one of this or later.
pub const AGENTS_FOUND_PROTOCOL: u32 = 45;

/// One coding agent's program, as the daemon finds it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AgentFound {
    /// The agent's harness: one of Muster's own by its id (`claude`), or
    /// one the person added by its name (`[agents.<name>]`).
    pub harness: String,
    /// Where its program is, as the daemon would start it; `None` where it
    /// is not found.
    pub path: Option<String>,
    /// The version it says it is (`--version`), that alone; `None` where it
    /// is not found or said none in time.
    pub version: Option<String>,
}
