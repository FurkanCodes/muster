//! Claude Code's tokens, from its own transcript: each assistant message's
//! `usage`, its input, cache writes, cache reads and output together.
//! Claude Code writes one line per block of a message, each with the same
//! usage, so a message counts once, by its id. Its subagents' transcripts,
//! `<session id>/subagents/agent-*.jsonl` beside it, count too. Claude Code
//! records no cost, so none is said (`PRD-12`).
//!
//! Checked against Claude Code 2.1.296's transcripts.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::tail::Tail;

/// One session's transcripts, as read so far.
#[derive(Debug, Default)]
pub(super) struct Claude {
    transcript: Option<Tail>,
    subagents: HashMap<PathBuf, Tail>,
    seen: HashSet<String>,
    tokens: u64,
}

impl Claude {
    /// The session's tokens so far, once its transcript is found; `projects`
    /// is where Claude Code keeps transcripts.
    pub(super) fn read(&mut self, projects: &Path, session_id: &str) -> Option<u64> {
        if self.transcript.is_none() {
            self.transcript = find(projects, session_id).map(Tail::new);
        }
        let transcript = self.transcript.as_ref()?.path().to_owned();
        let subagents = transcript.with_extension("").join("subagents");
        if let Ok(listed) = std::fs::read_dir(&subagents) {
            for entry in listed.flatten() {
                let path = entry.path();
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.starts_with("agent-") && name.ends_with(".jsonl") {
                    self.subagents
                        .entry(path.clone())
                        .or_insert_with(|| Tail::new(path));
                }
            }
        }
        let mut lines = Vec::new();
        let mut restarted = false;
        let tails = self
            .transcript
            .iter_mut()
            .chain(self.subagents.values_mut());
        for tail in tails {
            match tail.read() {
                Ok(read) => {
                    restarted |= read.restarted;
                    lines.extend(read.lines);
                }
                Err(error) => tracing::debug!(%error, "a transcript could not be read"),
            }
        }
        if restarted {
            // Written anew: counted again from the start, every file of it.
            *self = Self::default();
            return self.read(projects, session_id);
        }
        for line in &lines {
            if let Some((message, tokens)) = usage(line)
                && self.seen.insert(message)
            {
                self.tokens = self.tokens.saturating_add(tokens);
            }
        }
        Some(self.tokens)
    }
}

/// The transcript of `session_id`: `<projects>/<folder slug>/<id>.jsonl`,
/// under whichever slug it is.
fn find(projects: &Path, session_id: &str) -> Option<PathBuf> {
    let file = format!("{session_id}.jsonl");
    if Path::new(&file).file_name() != Some(std::ffi::OsStr::new(&file)) {
        return None;
    }
    std::fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|slug| slug.path().join(&file))
        .find(|path| path.is_file())
}

/// An assistant message's id and its tokens, from one transcript line.
fn usage(line: &str) -> Option<(String, u64)> {
    if !line.contains("\"assistant\"") {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "assistant" {
        return None;
    }
    let message = value.get("message")?;
    let id = message.get("id")?.as_str()?.to_owned();
    let usage = message.get("usage")?;
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let tokens = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ]
    .into_iter()
    .map(count)
    .fold(0_u64, u64::saturating_add);
    Some((id, tokens))
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A transcript line for one block of assistant message `id`.
    pub(crate) fn assistant(id: &str, input: u64, cache_read: u64, output: u64) -> String {
        serde_json::json!({
            "type": "assistant",
            "sessionId": "s",
            "message": {
                "id": id,
                "model": "claude-opus-5-5",
                "role": "assistant",
                "content": [{"type": "text", "text": "…"}],
                "usage": {
                    "input_tokens": input,
                    "cache_creation_input_tokens": 100,
                    "cache_read_input_tokens": cache_read,
                    "output_tokens": output,
                    "service_tier": "standard"
                }
            }
        })
        .to_string()
    }

    #[test]
    fn each_message_counts_once_with_its_subagents_and_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let slug = dir.path().join("-Users-u-app");
        std::fs::create_dir_all(slug.join("abc/subagents")).unwrap();
        let user = r#"{"type":"user","message":{"role":"user","content":"the assistant said"}}"#;
        // Two blocks of one message, then another message.
        std::fs::write(
            slug.join("abc.jsonl"),
            [
                user.to_owned(),
                assistant("msg_1", 2, 1_000, 50),
                assistant("msg_1", 2, 1_000, 50),
                assistant("msg_2", 3, 2_000, 70),
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();
        let mut claude = Claude::default();
        assert_eq!(claude.read(dir.path(), "nope"), None);
        let mut claude = Claude::default();
        assert_eq!(claude.read(dir.path(), "abc"), Some(1_152 + 2_173));
        std::fs::write(
            slug.join("abc/subagents/agent-a1.jsonl"),
            assistant("msg_9", 1, 10, 9) + "\n",
        )
        .unwrap();
        assert_eq!(claude.read(dir.path(), "abc"), Some(1_152 + 2_173 + 120));
        assert_eq!(
            claude.read(dir.path(), "abc"),
            Some(3_445),
            "read again, counted once"
        );
        assert_eq!(usage(user), None);
        assert_eq!(Claude::default().read(dir.path(), "../abc"), None);
    }
}
