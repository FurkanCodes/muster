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

/// A span of time as people read it: `45 s`, `12 min`, `3 h 05 min`,
/// `2 days`.
pub fn span(millis: u64) -> String {
    let seconds = millis / 1000;
    match seconds {
        0..60 => format!("{seconds} s"),
        60..3600 => format!("{} min", seconds / 60),
        3600..86_400 => format!("{} h {:02} min", seconds / 3600, seconds % 3600 / 60),
        _ => {
            let days = seconds / 86_400;
            if days == 1 {
                "1 day".to_owned()
            } else {
                format!("{days} days")
            }
        }
    }
}

/// The time from `from` to `to`, or nothing if `to` is earlier.
pub fn between(from: Timestamp, to: Timestamp) -> u64 {
    u64::try_from(to.as_millis().saturating_sub(from.as_millis())).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_read_as_people_say_them() {
        assert_eq!(span(0), "0 s");
        assert_eq!(span(59_999), "59 s");
        assert_eq!(span(60_000), "1 min");
        assert_eq!(span(3_599_000), "59 min");
        assert_eq!(span(3_600_000), "1 h 00 min");
        assert_eq!(span(3_600_000 + 5 * 60_000 + 30_000), "1 h 05 min");
        assert_eq!(span(86_400_000), "1 day");
        assert_eq!(span(3 * 86_400_000 + 1), "3 days");
    }

    #[test]
    fn time_between_never_goes_negative() {
        let (early, late) = (
            Timestamp::from_millis(1_000),
            Timestamp::from_millis(61_000),
        );
        assert_eq!(between(early, late), 60_000);
        assert_eq!(between(late, early), 0);
    }
}
