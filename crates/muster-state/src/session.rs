//! Session status: turns hooks, the terminal's progress sequence and process
//! exits into the status the person sees (`ARCHITECTURE.md` §5).
//!
//! Rules carried over from Nebula's experience:
//! - a harness session other than the one this process runs is ignored, except
//!   for the start announcement that follows `/clear`;
//! - the main agent ending its turn does not finish the session while its
//!   subagents still work; a grace period follows the last one;
//! - a permission prompt closes on a tool event from whoever raised it, and a
//!   question only on its own completion, unless the person cuts the turn
//!   short, which closes every prompt;
//! - a progress idle with no busy edge before it is the startup state.
//!
//! Changed from Nebula: several prompts can be open at once, and a deferred
//! permission notice is told apart from the request itself, so a real second
//! request is never dropped as an echo.

mod describe;
mod machine;
mod signal;

#[cfg(test)]
mod tests;

pub use machine::{Carried, Launch, SessionMachine, Start, Timing};
pub use signal::{Effect, HookEvent, Origin, Signal};
