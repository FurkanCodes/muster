//! Wall-clock time as stored and sent. Reading the clock belongs to the daemon
//! (`ARC-03`); this type only carries the value.

use serde::{Deserialize, Serialize};

/// A moment in wall-clock time: milliseconds since the Unix epoch, UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Timestamp(i64);

impl Timestamp {
    /// The moment `millis` milliseconds after the Unix epoch.
    pub const fn from_millis(millis: i64) -> Self {
        Self(millis)
    }

    /// Milliseconds since the Unix epoch.
    pub const fn as_millis(self) -> i64 {
        self.0
    }
}
