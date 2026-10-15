//! Gemini CLI's tokens, from its own chat recording: each of the model's
//! messages' `tokens.total`, its input (cached input in it), output,
//! thoughts and tool use together. The recording is one JSON document,
//! `<home>/tmp/<project hash>/chats/session-<time>-<id's first 8>.json`,
//! written whole as it changes, so it is read again whenever it has.
//! Gemini CLI records no cost, so none is said (`PRD-12`).
//!
//! Checked against a recording of Gemini CLI from early 2026.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;

/// How long a recording not found waits before it is looked for again.
const LOOK_AGAIN: Duration = Duration::from_secs(30);

/// One session's recording, as last read.
#[derive(Debug, Default)]
pub(super) struct Gemini {
    recording: Option<PathBuf>,
    looked: Option<Instant>,
    /// The recording's modification time and length when last read.
    read_as: Option<(SystemTime, u64)>,
    tokens: Option<u64>,
}

#[derive(Deserialize)]
struct Recording {
    #[serde(rename = "sessionId", default)]
    session_id: String,
    #[serde(default)]
    messages: Vec<Message>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    tokens: Option<Tokens>,
}

#[derive(Deserialize)]
struct Tokens {
    #[serde(default)]
    input: u64,
    #[serde(default)]
    output: u64,
    #[serde(default)]
    thoughts: u64,
    #[serde(default)]
    tool: u64,
    #[serde(default)]
    total: Option<u64>,
}

impl Gemini {
    /// The session's tokens so far, once its recording is found; `tmp` is
    /// Gemini CLI's `tmp` folder.
    pub(super) fn read(&mut self, tmp: &Path, session_id: &str, now: Instant) -> Option<u64> {
        if self.recording.is_none() && self.looked.is_none_or(|at| now >= at + LOOK_AGAIN) {
            self.looked = Some(now);
            self.recording = find(tmp, session_id);
        }
        let recording = self.recording.as_ref()?;
        let as_now = std::fs::metadata(recording)
            .ok()
            .and_then(|meta| Some((meta.modified().ok()?, meta.len())));
        if as_now.is_some() && as_now != self.read_as {
            match read(recording) {
                Some(found) if found.session_id == session_id => {
                    self.read_as = as_now;
                    self.tokens = Some(tokens(&found));
                }
                // Half written, most likely: read again next time.
                _ => {}
            }
        }
        self.tokens
    }
}

fn read(path: &Path) -> Option<Recording> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn tokens(recording: &Recording) -> u64 {
    recording
        .messages
        .iter()
        .filter(|message| message.kind == "gemini")
        .filter_map(|message| message.tokens.as_ref())
        .map(|tokens| {
            tokens
                .total
                .unwrap_or(tokens.input + tokens.output + tokens.thoughts + tokens.tool)
        })
        .fold(0_u64, u64::saturating_add)
}

/// The recording of `session_id`, in any project's chats.
fn find(tmp: &Path, session_id: &str) -> Option<PathBuf> {
    let prefix: String = session_id.chars().take(8).collect();
    if prefix.len() < 8
        || !prefix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let ending = format!("-{prefix}.json");
    for project in std::fs::read_dir(tmp).ok()?.flatten() {
        let Ok(chats) = std::fs::read_dir(project.path().join("chats")) else {
            continue;
        };
        for chat in chats.flatten() {
            let path = chat.path();
            let named = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("session-") && name.ends_with(&ending));
            if named && read(&path).is_some_and(|found| found.session_id == session_id) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "dea9457f-1315-439d-b94b-3ca4c8293f70";

    fn recording(totals: &[u64]) -> String {
        let mut messages = vec![serde_json::json!({"id": "u1", "type": "user", "content": "hi"})];
        for (at, total) in totals.iter().enumerate() {
            messages.push(serde_json::json!({
                "id": format!("g{at}"), "type": "gemini", "content": "…", "model": "gemini-3-pro",
                "tokens": {"input": total - 10, "output": 6, "cached": 3, "thoughts": 4, "tool": 0, "total": total}
            }));
        }
        serde_json::json!({"sessionId": ID, "projectHash": "0b0a", "startTime": "t", "lastUpdated": "t", "messages": messages}).to_string()
    }

    #[test]
    fn the_model_s_messages_totals_are_its_tokens_read_again_as_the_recording_changes() {
        let dir = tempfile::tempdir().unwrap();
        let chats = dir.path().join("0b0a/chats");
        std::fs::create_dir_all(&chats).unwrap();
        let path = chats.join("session-2026-01-20T20-35-dea9457f.json");
        std::fs::write(&path, recording(&[100, 250])).unwrap();
        let now = Instant::now();
        let mut gemini = Gemini::default();
        assert_eq!(gemini.read(dir.path(), ID, now), Some(350));
        std::fs::write(&path, recording(&[100, 250, 4_000])).unwrap();
        assert_eq!(gemini.read(dir.path(), ID, now), Some(4_350));
        assert_eq!(
            Gemini::default().read(dir.path(), "dea9457f-0000-0000-0000-000000000000", now),
            None,
            "another session with the same first eight"
        );
        assert_eq!(Gemini::default().read(dir.path(), "../x", now), None);
    }
}
