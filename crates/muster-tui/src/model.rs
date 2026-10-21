//! The UI's copy of the daemon's state: the model the desktop app keeps
//! too (`muster-model`), so both say alike what needs the person and in
//! what order.

pub(crate) use muster_model::{Model, Row, needs_attention, stands_beside};

/// Entities to build tests' states from.
#[cfg(test)]
pub(crate) use muster_model::fixtures as tests;
