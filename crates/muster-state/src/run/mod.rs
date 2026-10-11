//! Run stages: a ticket taken from pickup to a merged pull request, with
//! checks and an agent review before a person is asked (`PRODUCT.md` §5,
//! `ARCHITECTURE.md` §8).
//!
//! Rules:
//! - a failed check, a reviewer's and a person's request for changes each
//!   spend an attempt; past the budget, the run needs the person, with the
//!   evidence (`PRD-02`, `PRD-12`);
//! - time counts only while the run works on its own; a cost limit applies
//!   where the harness reports cost; hitting either stops the work;
//! - a person's retry or hand-back starts a fresh budget;
//! - every transition is stored, with its reason, before its work starts
//!   (`ARC-10`), and work reports back with the generation it was asked
//!   in, so a report on work overtaken since (a review after a cancel,
//!   checks after a take-over) changes nothing.

mod machine;
mod signal;

#[cfg(test)]
mod tests;

pub use machine::{Limits, RunMachine};
pub use signal::{AgentOutcome, Feedback, RunEffect, RunEvent, Verdict};
