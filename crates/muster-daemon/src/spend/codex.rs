//! Codex's tokens, from its own rollout file: the last `token_count`
//! event's `total_token_usage.total_tokens`, its input (cached input in
//! it) and output together. The file is
//! `<sessions>/<year>/<month>/<day>/rollout-<time>-<thread id>.jsonl`.
//! Codex records no cost, so none is said (`PRD-12`).
//!
//! Checked against Codex 0.159's rollout files.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::tail::Tail;

/// How long a rollout not found waits before it is looked for again: the
/// search walks Codex's sessions, day by day.
const LOOK_AGAIN: Duration = Duration::from_secs(30);

/// One session's rollout, as read so far.
#[derive(Debug, Default)]
pub(super) struct Codex {
    rollout: Option<Tail>,
    looked: Option<Instant>,
    tokens: Option<u64>,
}

impl Codex {
    /// The thread's tokens so far, once Codex has counted some; `sessions`
    /// is where Codex keeps its rollouts.
    pub(super) fn read(&mut self, sessions: &Path, thread: &str, now: Instant) -> Option<u64> {
        if self.rollout.is_none() && self.looked.is_none_or(|at| now >= at + LOOK_AGAIN) {
            self.looked = Some(now);
            self.rollout = find(sessions, thread).map(Tail::new);
        }
        match self.rollout.as_mut()?.read() {
            Ok(read) => {
                if read.restarted {
                    self.tokens = None;
                }
                if let Some(total) = read.lines.iter().rev().find_map(|line| total(line)) {
                    self.tokens = Some(total);
                }
            }
            Err(error) => tracing::debug!(%error, "a rollout could not be read"),
        }
        self.tokens
    }
}

/// The newest rollout of `thread`, its name ending in its id.
fn find(sessions: &Path, thread: &str) -> Option<PathBuf> {
    if thread.is_empty() || thread.contains(['/', '\\']) {
        return None;
    }
    let ending = format!("-{thread}.jsonl");
    let newest_first = |dir: &Path| -> Vec<PathBuf> {
        let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|listed| listed.flatten().map(|entry| entry.path()).collect())
            .unwrap_or_default();
        entries.sort_unstable_by(|one, other| other.cmp(one));
        entries
    };
    for year in newest_first(sessions) {
        for month in newest_first(&year) {
            for day in newest_first(&month) {
                let found = newest_first(&day).into_iter().find(|file| {
                    file.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(&ending))
                });
                if found.is_some() {
                    return found;
                }
            }
        }
    }
    None
}

/// The running total in a `token_count` event's line.
fn total(line: &str) -> Option<u64> {
    if !line.contains("token_count") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    let payload = value.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    payload
        .get("info")?
        .get("total_token_usage")?
        .get("total_tokens")?
        .as_u64()
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A `token_count` event as Codex writes it, `None` for one with only
    /// rate limits in it.
    pub(crate) fn token_count(total: Option<u64>) -> String {
        let info = total.map(|total| {
            serde_json::json!({
                "total_token_usage": {"input_tokens": total - 100, "cached_input_tokens": 50, "output_tokens": 100, "reasoning_output_tokens": 20, "total_tokens": total},
                "last_token_usage": {"total_tokens": 10},
                "model_context_window": 258_400
            })
        });
        serde_json::json!({
            "timestamp": "2026-09-30T09:51:12.000Z",
            "type": "event_msg",
            "payload": {"type": "token_count", "info": info, "rate_limits": {"primary": null}}
        })
        .to_string()
    }

    #[test]
    fn the_last_running_total_of_the_thread_s_rollout_is_its_tokens() {
        let dir = tempfile::tempdir().unwrap();
        let day = dir.path().join("2026/09/30");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::create_dir_all(dir.path().join("2026/10/01")).unwrap();
        let rollout = day.join("rollout-2026-09-30T11-51-12-01a0f182-bea0.jsonl");
        let meta = r#"{"type":"session_meta","payload":{"id":"01a0f182-bea0"}}"#;
        std::fs::write(
            &rollout,
            [meta.to_owned(), token_count(Some(1_000)), token_count(None)].join("\n") + "\n",
        )
        .unwrap();
        let now = crate::clock::monotonic();
        assert_eq!(Codex::default().read(dir.path(), "someone-else", now), None);
        let mut codex = Codex::default();
        assert_eq!(codex.read(dir.path(), "01a0f182-bea0", now), Some(1_000));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rollout)
            .unwrap();
        std::io::Write::write_all(&mut file, (token_count(Some(5_400)) + "\n").as_bytes()).unwrap();
        assert_eq!(codex.read(dir.path(), "01a0f182-bea0", now), Some(5_400));
        assert_eq!(
            codex.read(dir.path(), "01a0f182-bea0", now),
            Some(5_400),
            "nothing new"
        );
        assert_eq!(Codex::default().read(dir.path(), "../2026", now), None);
    }

    #[test]
    fn a_rollout_not_found_is_looked_for_again_only_after_a_while() {
        let dir = tempfile::tempdir().unwrap();
        let now = crate::clock::monotonic();
        let mut codex = Codex::default();
        assert_eq!(codex.read(dir.path(), "t1", now), None);
        let day = dir.path().join("2026/10/15");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("rollout-2026-10-15T10-00-00-t1.jsonl"),
            token_count(Some(300)) + "\n",
        )
        .unwrap();
        assert_eq!(
            codex.read(dir.path(), "t1", now + Duration::from_secs(1)),
            None
        );
        assert_eq!(codex.read(dir.path(), "t1", now + LOOK_AGAIN), Some(300));
    }
}
