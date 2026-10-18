//! A copy of the database before it is migrated (`PRD-09`), so an upgrade
//! that goes wrong never loses the history.
//!
//! Before an existing database is brought up from an older schema,
//! `VACUUM INTO` writes a consistent copy of it, what is still in its WAL
//! included, into `backups/` beside it (Muster's data directory). The copy
//! is named with both schema versions and the time, readable by the person
//! alone, and the newest [`KEEP`] are kept. A new database, or one already
//! current, is not copied.

use std::fs::{DirBuilder, File, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use muster_core::Timestamp;
use rusqlite::Connection;

/// How many copies are kept: the newest.
pub(super) const KEEP: usize = 3;

/// What every copy's name starts and ends with; nothing else in the folder
/// is ever let go.
const PREFIX: &str = "muster-";
const SUFFIX: &str = ".db";

/// The folder copies of the database at `database` go into.
pub(super) fn folder(database: &Path) -> PathBuf {
    database
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("backups")
}

/// The name of a copy made at `now` before an upgrade from schema `from`
/// to `to`: `muster-2026-10-18T14-22-33.123Z-schema-5-to-6.db`. The time
/// comes first and is of one width, so names sort as the copies were made.
pub(super) fn name(from: u32, to: u32, now: Timestamp) -> String {
    format!("{PREFIX}{}-schema-{from}-to-{to}{SUFFIX}", utc_stamp(now))
}

/// Copies the database `conn` has open into `folder`, as [`name`] says,
/// then lets older copies go past the newest [`KEEP`]. Returns where the
/// copy is, or where it was to be and why it is not.
pub(super) fn copy(
    conn: &Connection,
    folder: &Path,
    from: u32,
    to: u32,
    now: Timestamp,
) -> Result<PathBuf, (PathBuf, io::Error)> {
    let path = folder.join(name(from, to, now));
    match write(conn, folder, &path) {
        Ok(()) => {}
        Err(error) => {
            // Nothing half written is left to be taken for a copy.
            if error.kind() != io::ErrorKind::AlreadyExists {
                let _ = std::fs::remove_file(&path);
            }
            return Err((path, error));
        }
    }
    tracing::info!(
        copy = %path.display(),
        from,
        to,
        "copied the database before upgrading its schema"
    );
    if let Err(error) = prune(folder, KEEP) {
        tracing::warn!(%error, folder = %folder.display(), "could not let older copies of the database go");
    }
    Ok(path)
}

/// Writes the copy: the file made first, empty and readable by the person
/// alone, which `VACUUM INTO` takes as it is, then put on disk.
fn write(conn: &Connection, folder: &Path, path: &Path) -> io::Result<()> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(folder)?;
    let target = path.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path is not UTF-8, which SQLite needs",
        )
    })?;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    conn.execute("VACUUM INTO ?1", [target])
        .map_err(io::Error::other)?;
    File::open(path)?.sync_all()
}

/// Lets every copy in `folder` go but the newest `keep`, by their names.
fn prune(folder: &Path, keep: usize) -> io::Result<()> {
    let mut copies: Vec<String> = std::fs::read_dir(folder)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(PREFIX) && name.ends_with(SUFFIX))
        .collect();
    copies.sort_unstable_by(|a, b| b.cmp(a));
    for old in copies.iter().skip(keep) {
        std::fs::remove_file(folder.join(old))?;
    }
    Ok(())
}

/// `now` in UTC, of one width and fit for a file name:
/// `2026-10-18T14-22-33.123Z`. A time before 1970 reads as 1970.
fn utc_stamp(now: Timestamp) -> String {
    const DAY: i64 = 86_400_000;
    let millis = now.as_millis().max(0);
    let (days, of_day) = (millis / DAY, millis % DAY);
    let (year, month, day) = civil(days);
    let seconds = of_day / 1000;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}-{:02}-{:02}.{:03}Z",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        of_day % 1000
    )
}

/// The proleptic Gregorian date `days` after 1970-01-01, by Howard
/// Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let of_era = z.rem_euclid(146_097);
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * of_year + 2) / 153;
    let day = of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_named_by_its_time_in_utc_and_its_schemas() {
        assert_eq!(
            name(5, 6, Timestamp::from_millis(0)),
            "muster-1970-01-01T00-00-00.000Z-schema-5-to-6.db"
        );
        // 2026-10-18 14:22:33.123 UTC.
        assert_eq!(
            utc_stamp(Timestamp::from_millis(1_792_333_353_123)),
            "2026-10-18T14-22-33.123Z"
        );
        // A leap day, and the last moment of a year.
        assert_eq!(
            utc_stamp(Timestamp::from_millis(951_782_400_000)),
            "2000-02-29T00-00-00.000Z"
        );
        assert_eq!(
            utc_stamp(Timestamp::from_millis(1_798_761_599_999)),
            "2026-12-31T23-59-59.999Z"
        );
        assert_eq!(
            utc_stamp(Timestamp::from_millis(-5)),
            "1970-01-01T00-00-00.000Z"
        );
    }

    #[test]
    fn names_sort_as_the_copies_were_made() {
        let early = name(9, 10, Timestamp::from_millis(1_000));
        let late = name(1, 2, Timestamp::from_millis(2_000));
        assert!(early < late, "{early} {late}");
    }
}
