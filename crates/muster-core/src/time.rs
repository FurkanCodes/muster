//! Wall-clock time as stored and sent. Reading the clock belongs to the daemon
//! (`ARC-03`); this type only carries the value.

use serde::{Deserialize, Serialize};

/// A moment in wall-clock time: milliseconds since the Unix epoch, UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
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

/// `text` as a moment, if it is one in RFC 3339 as trackers write them:
/// `2026-10-22T09:30:00Z`, `2026-10-22T09:30:00.123+02:00`; none otherwise.
pub fn parse_rfc3339(text: &str) -> Option<Timestamp> {
    let text = text.trim();
    let (date, rest) = text.split_once(['T', 't', ' '])?;
    let mut parts = date.splitn(3, '-');
    let year: i64 = digits(parts.next()?, 4)?;
    let month: i64 = digits(parts.next()?, 2)?;
    let day: i64 = digits(parts.next()?, 2)?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let hour: i64 = digits(rest.get(0..2)?, 2)?;
    let minute: i64 = digits(rest.get(3..5)?, 2)?;
    let second: i64 = digits(rest.get(6..8)?, 2)?;
    if rest.get(2..3)? != ":" || rest.get(5..6)? != ":" || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let mut rest = &rest[8..];
    let mut millis = 0;
    if let Some(fraction) = rest.strip_prefix('.') {
        let end = fraction
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(fraction.len());
        if end == 0 {
            return None;
        }
        let first: String = fraction[..end]
            .chars()
            .chain("00".chars())
            .take(3)
            .collect();
        millis = first.parse().ok()?;
        rest = &fraction[end..];
    }
    let offset = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.get(0..1)? {
                "+" => 1,
                "-" => -1,
                _ => return None,
            };
            let hours: i64 = digits(rest.get(1..3)?, 2)?;
            let minutes: i64 = digits(rest.get(4..6)?, 2)?;
            if rest.get(3..4)? != ":" || rest.len() != 6 {
                return None;
            }
            sign * (hours * 60 + minutes) * 60
        }
    };
    let seconds =
        days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second - offset;
    Some(Timestamp::from_millis(seconds * 1000 + millis))
}

/// `at` in RFC 3339, in UTC to the second: `2026-10-22T09:30:00Z`.
pub fn rfc3339(at: Timestamp) -> String {
    let seconds = at.as_millis().div_euclid(1000);
    let (days, of_day) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        of_day / 3600,
        of_day % 3600 / 60,
        of_day % 60
    )
}

/// `text` as a number of exactly `width` ASCII digits.
fn digits(text: &str, width: usize) -> Option<i64> {
    (text.len() == width && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// Days from 1970-01-01 to a date in the proleptic Gregorian calendar
/// (Howard Hinnant's `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let of_era = year - era * 400;
    let of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let of_cycle = of_era * 365 + of_era / 4 - of_era / 100 + of_year;
    era * 146_097 + of_cycle - 719_468
}

/// The date `days` after 1970-01-01, as `days_from_civil` counts.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted + 2) / 5 + 1;
    let month = if shifted < 10 {
        shifted + 3
    } else {
        shifted - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
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
    fn rfc3339_moments_read_as_trackers_write_them() {
        let at = |text| parse_rfc3339(text).map(Timestamp::as_millis);
        assert_eq!(at("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(at("2026-10-22T09:30:00Z"), Some(1_792_661_400_000));
        assert_eq!(at("2026-10-22T11:30:00.250+02:00"), Some(1_792_661_400_250));
        assert_eq!(at("2026-10-22T04:30:00.1-05:00"), Some(1_792_661_400_100));
        assert_eq!(at("2024-02-29T23:59:59Z"), Some(1_709_251_199_000));
        for wrong in [
            "",
            "yesterday",
            "2026-13-01T00:00:00Z",
            "2026-10-22",
            "2026-10-22T9:30:00Z",
            "2026-10-22T09:30:00",
            "2026-10-22T09:30:00+0200",
        ] {
            assert_eq!(at(wrong), None, "{wrong}");
        }
    }

    #[test]
    fn rfc3339_writes_what_it_reads() {
        for text in [
            "1970-01-01T00:00:00Z",
            "2026-10-22T09:30:00Z",
            "2024-02-29T23:59:59Z",
            "1969-12-31T23:59:59Z",
        ] {
            assert_eq!(rfc3339(parse_rfc3339(text).unwrap()), text);
        }
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
