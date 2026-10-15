//! The daemon's log stays bounded (`ARC-15`).
//!
//! Started from the UI, the daemon writes to `daemon.log` in the runtime
//! directory as its standard error. Every so often it looks: once that file
//! is from an earlier day (UTC) or past its size limit, it becomes
//! `daemon.log.1`, the older ones move up one and the oldest goes, so seven
//! are kept, and a fresh `daemon.log` takes the place of standard error, so
//! a panic lands there too. A daemon writing anywhere else, to a terminal
//! or a test's file, is left alone.

use std::fs::{File, Metadata, OpenOptions};
use std::io;
use std::os::fd::AsFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::clock::Clock;

/// How many files are kept, the one written to included.
pub(crate) const KEEP: usize = 7;

const DAY_MILLIS: u64 = 24 * 60 * 60 * 1000;

/// When the log moves on to a fresh file, and how often that is looked at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogLimits {
    /// Past this size the file is set aside, whatever its day.
    pub max_bytes: u64,
    /// How often the file is looked at.
    pub every: Duration,
}

impl Default for LogLimits {
    fn default() -> Self {
        Self {
            max_bytes: 10 * 1024 * 1024,
            every: Duration::from_secs(60),
        }
    }
}

/// Keeps the log at `path` bounded while it is this process's standard
/// error. Runs until dropped.
pub(crate) async fn run(path: PathBuf, limits: LogLimits, clock: Arc<dyn Clock>) {
    let mut ticks = tokio::time::interval(limits.every);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let today = u64::try_from(clock.now().as_millis()).unwrap_or(0) / DAY_MILLIS;
        match tend(&path, limits.max_bytes, today) {
            Ok(false) => {}
            Ok(true) => tracing::info!(
                earlier = %numbered(&path, 1).display(),
                "a fresh log; the earlier one was set aside"
            ),
            Err(error) => {
                tracing::warn!(%error, log = %path.display(), "could not set the log aside");
            }
        }
    }
}

/// Sets the log aside if standard error writes to it and it is due.
fn tend(path: &Path, max_bytes: u64, today: u64) -> io::Result<bool> {
    let Ok(log) = std::fs::metadata(path) else {
        return Ok(false);
    };
    let stderr = File::from(io::stderr().as_fd().try_clone_to_owned()?).metadata()?;
    if !same_file(&stderr, &log) || !due(&log, max_bytes, today) {
        return Ok(false);
    }
    let fresh = set_aside(path)?;
    nix::unistd::dup2_stderr(&fresh)?;
    Ok(true)
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    (a.dev(), a.ino()) == (b.dev(), b.ino())
}

/// Whether the log has outgrown its size or its day. A file system that
/// cannot tell when a file was made bounds it by size alone.
fn due(log: &Metadata, max_bytes: u64, today: u64) -> bool {
    log.len() > max_bytes
        || log
            .created()
            .ok()
            .and_then(day_of)
            .is_some_and(|day| day < today)
}

fn day_of(time: SystemTime) -> Option<u64> {
    let millis = time.duration_since(UNIX_EPOCH).ok()?.as_millis();
    u64::try_from(millis / u128::from(DAY_MILLIS)).ok()
}

/// Moves `path` to `path.1`, each older one up a number, the oldest kept
/// replaced, and opens a fresh `path`.
fn set_aside(path: &Path) -> io::Result<File> {
    for n in (1..KEEP - 1).rev() {
        match std::fs::rename(numbered(path, n), numbered(path, n + 1)) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
            _ => {}
        }
    }
    std::fs::rename(path, numbered(path, 1))?;
    OpenOptions::new().create(true).append(true).open(path)
}

fn numbered(path: &Path, n: usize) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".{n}"));
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn setting_aside_keeps_seven_files_and_drops_the_oldest() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("daemon.log");
        std::fs::write(&log, "first").unwrap();
        for round in 2..=10 {
            let mut fresh = set_aside(&log).unwrap();
            io::Write::write_all(&mut fresh, format!("round {round}").as_bytes()).unwrap();
        }
        assert_eq!(
            files(dir.path()),
            [
                "daemon.log",
                "daemon.log.1",
                "daemon.log.2",
                "daemon.log.3",
                "daemon.log.4",
                "daemon.log.5",
                "daemon.log.6"
            ]
        );
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "round 10");
        assert_eq!(
            std::fs::read_to_string(numbered(&log, 1)).unwrap(),
            "round 9"
        );
        assert_eq!(
            std::fs::read_to_string(numbered(&log, 6)).unwrap(),
            "round 4"
        );
    }

    #[test]
    fn a_log_is_due_past_its_size_or_its_day() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("daemon.log");
        std::fs::write(&log, "0123456789").unwrap();
        let meta = std::fs::metadata(&log).unwrap();
        let made = meta.created().ok().and_then(day_of);
        let today = made.unwrap_or(0);
        assert!(!due(&meta, 100, today));
        assert!(due(&meta, 9, today));
        // The next day, by its creation, where the file system tells it.
        assert_eq!(due(&meta, 100, today + 1), made.is_some());
    }

    #[test]
    fn only_the_file_standard_error_writes_to_is_tended() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("daemon.log");
        std::fs::write(&log, "a long line, past any limit").unwrap();
        // This test's standard error is not that file.
        assert!(!tend(&log, 1, u64::MAX).unwrap());
        assert_eq!(files(dir.path()), ["daemon.log"]);
        // Nor is there anything to tend where there is no log.
        assert!(!tend(&dir.path().join("none.log"), 1, u64::MAX).unwrap());
    }
}
