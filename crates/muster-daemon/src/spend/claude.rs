//! Claude Code's tokens, from its own transcript: each assistant message's
//! `usage`, its input, cache writes, cache reads and output together.
//! Claude Code writes one line per block of a message, each with the same
//! usage, so a message counts once, by its id. Its subagents' transcripts,
//! `<session id>/subagents/agent-*.jsonl` beside it, count too. Claude Code
//! records no cost, so none is said (`PRD-12`).
//!
//! The transcript also says when the person cut a turn short, which no
//! hook does: pressing Esc as Claude works writes `[Request interrupted by
//! user]`, and declining a permission it asked for `[Request interrupted
//! by user for tool use]`, each a text block of a user message carrying
//! the turn's `promptId`. Only that id is kept; nothing of the
//! conversation is (`PRD-06`).
//!
//! It says, too, when a background task stopped, a subagent among them,
//! which for one the person stops by hand no hook does: Claude Code queues
//! a `<task-notification>` for the main agent, its first lines the task's
//! id (a subagent's agent id) and its status. Only the id is kept.
//!
//! Checked against Claude Code 2.1.296's transcripts, and 2.1.304's for a
//! task stopping.

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
    /// The last turn the person interrupted, by its prompt id, in what the
    /// last read found.
    interrupted: Option<String>,
    /// The background tasks the last read found stopped, by their ids.
    stopped: Vec<String>,
}

/// How the transcript begins the text it writes as the person interrupts.
const INTERRUPTED: &str = "[Request interrupted by user";

/// How the transcript begins the notice it queues as a background task stops.
const TASK_NOTICE: &str = "<task-notification>";

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
        let Self {
            transcript: main,
            subagents,
            seen,
            tokens,
            ..
        } = self;
        let mut count = |line: &str| {
            if let Some((message, used)) = usage(line)
                && seen.insert(message)
            {
                *tokens = tokens.saturating_add(used);
            }
        };
        let mut restarted = false;
        // The last turn cut short in what is read now; only the main agent's
        // own can be.
        let mut cut_short = None;
        // Only the main agent is told of its tasks.
        let mut stopped = Vec::new();
        if let Some(main) = main {
            let read = main.read(|line| {
                if let Some(prompt) = interrupted(line) {
                    cut_short = Some(prompt);
                }
                if let Some(task) = stopped_task(line) {
                    stopped.push(task);
                }
                count(line);
            });
            match read {
                Ok(anew) => restarted |= anew,
                Err(error) => tracing::debug!(%error, "a transcript could not be read"),
            }
        }
        for tail in subagents.values_mut() {
            match tail.read(&mut count) {
                Ok(anew) => restarted |= anew,
                Err(error) => tracing::debug!(%error, "a transcript could not be read"),
            }
        }
        if restarted {
            // Written anew: counted again from the start, every file of it.
            *self = Self::default();
            return self.read(projects, session_id);
        }
        self.interrupted = cut_short;
        self.stopped = stopped;
        Some(self.tokens)
    }

    /// The last turn the person interrupted in what the last read found,
    /// by its prompt id; once.
    pub(super) fn take_interrupted(&mut self) -> Option<String> {
        self.interrupted.take()
    }

    /// The background tasks the last read found stopped, by their ids;
    /// once.
    pub(super) fn take_stopped(&mut self) -> Vec<String> {
        std::mem::take(&mut self.stopped)
    }
}

/// The prompt id of the turn a line of the main transcript says the person
/// interrupted.
fn interrupted(line: &str) -> Option<String> {
    if !line.contains(INTERRUPTED) {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "user"
        || value.get("isSidechain").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    // A prompt the person typed is a string; the marker is a block.
    let blocks = value.get("message")?.get("content")?.as_array()?;
    let marked = blocks.iter().any(|block| {
        block.get("type").and_then(Value::as_str) == Some("text")
            && block
                .get("text")
                .and_then(Value::as_str)
                .is_some_and(|text| text.starts_with(INTERRUPTED))
    });
    if !marked {
        return None;
    }
    Some(value.get("promptId")?.as_str()?.to_owned())
}

/// The id of the background task a line of the main transcript says has
/// stopped: the notice queued for the main agent as one completes, fails
/// or is stopped.
fn stopped_task(line: &str) -> Option<String> {
    if !line.contains(TASK_NOTICE) {
        return None;
    }
    let value: Value = serde_json::from_str(line).ok()?;
    if value.get("type")?.as_str()? != "queue-operation"
        || value.get("operation")?.as_str()? != "enqueue"
    {
        return None;
    }
    let notice = value.get("content")?.as_str()?.strip_prefix(TASK_NOTICE)?;
    // The id and the status come first, before the task's own words.
    let status = between(notice, "<status>", "</status>")?;
    if !matches!(status, "completed" | "failed" | "killed" | "stopped") {
        return None;
    }
    Some(between(notice, "<task-id>", "</task-id>")?.to_owned())
}

/// What `text` has between the first `open` and the `close` after it.
fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let (_, rest) = text.split_once(open)?;
    Some(rest.split_once(close)?.0)
}

/// The transcript of `session_id`: `<projects>/<folder slug>/<id>.jsonl`,
/// under whichever slug it is.
pub(super) fn find(projects: &Path, session_id: &str) -> Option<PathBuf> {
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

    /// The lines Claude Code 2.1.296 writes as the person declines a
    /// permission: the tool's result, then the marker. Content shortened.
    pub(crate) fn declined(prompt: &str) -> String {
        [
            serde_json::json!({
                "type": "user", "isSidechain": false, "promptId": prompt,
                "toolUseResult": "User rejected tool use", "toolDenialKind": "user-rejected",
                "message": {"role": "user", "content": [{
                    "type": "tool_result", "is_error": true, "tool_use_id": "toolu_1",
                    "content": "The user doesn't want to proceed with this tool use."
                }]}
            })
            .to_string(),
            serde_json::json!({
                "type": "user", "isSidechain": false, "promptId": prompt,
                "message": {"role": "user", "content": [
                    {"type": "text", "text": "[Request interrupted by user for tool use]"}
                ]}
            })
            .to_string(),
            serde_json::json!({
                "type": "system", "subtype": "turn_duration", "durationMs": 1477
            })
            .to_string(),
        ]
        .join("\n")
    }

    #[test]
    fn a_turn_the_person_cut_short_is_found_once_by_its_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let slug = dir.path().join("-Users-u-app");
        std::fs::create_dir_all(slug.join("abc/subagents")).unwrap();
        let stopped = r#"{"type":"user","promptId":"p1","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#;
        // Typed by the person, it is a prompt, not the marker.
        let typed = r#"{"type":"user","promptId":"p3","message":{"role":"user","content":"[Request interrupted by user]"}}"#;
        std::fs::write(
            slug.join("abc.jsonl"),
            format!("{stopped}\n{}\n{typed}\n", declined("p2")),
        )
        .unwrap();
        // A subagent's own transcript is no turn of the person's.
        std::fs::write(
            slug.join("abc/subagents/agent-a1.jsonl"),
            declined("p9") + "\n",
        )
        .unwrap();
        let mut claude = Claude::default();
        claude.read(dir.path(), "abc");
        assert_eq!(claude.take_interrupted().as_deref(), Some("p2"));
        assert_eq!(claude.take_interrupted(), None, "taken once");
        claude.read(dir.path(), "abc");
        assert_eq!(claude.take_interrupted(), None, "nothing new");
        assert_eq!(interrupted(typed), None);
        let sidechain =
            stopped.replace(r#""type":"user","#, r#""type":"user","isSidechain":true,"#);
        assert_eq!(interrupted(&sidechain), None);
        assert_eq!(interrupted(stopped).as_deref(), Some("p1"));
    }

    /// The line Claude Code 2.1.304 writes as a background task stops: the
    /// notice queued for the main agent. Content shortened.
    pub(crate) fn task_stopped(task: &str, status: &str) -> String {
        serde_json::json!({
            "type": "queue-operation", "operation": "enqueue",
            "timestamp": "2026-10-20T14:14:14.985Z", "sessionId": "abc",
            "content": format!(
                "<task-notification>\n<task-id>{task}</task-id>\n\
                 <tool-use-id>toolu_1</tool-use-id>\n<status>{status}</status>\n\
                 <summary>Agent \"Look around\" was stopped by user</summary>\n\
                 </task-notification>"
            )
        })
        .to_string()
    }

    #[test]
    fn a_background_task_that_stopped_is_found_once_by_its_id() {
        let dir = tempfile::tempdir().unwrap();
        let slug = dir.path().join("-Users-u-app");
        std::fs::create_dir_all(slug.join("abc/subagents")).unwrap();
        // The notice handed to the main agent later is the same stop.
        let delivered = r#"{"type":"user","isSidechain":false,"promptId":"p1","origin":{"kind":"task-notification"},"message":{"role":"user","content":"<task-notification>\n<task-id>a1</task-id>\n<status>killed</status>\n</task-notification>"}}"#;
        let dequeued = r#"{"type":"queue-operation","operation":"dequeue","sessionId":"abc"}"#;
        std::fs::write(
            slug.join("abc.jsonl"),
            [
                task_stopped("a1", "killed"),
                delivered.to_owned(),
                dequeued.to_owned(),
                task_stopped("b2", "completed"),
            ]
            .join("\n")
                + "\n",
        )
        .unwrap();
        // A subagent's own transcript tells the main agent nothing.
        std::fs::write(
            slug.join("abc/subagents/agent-a1.jsonl"),
            task_stopped("a9", "failed") + "\n",
        )
        .unwrap();
        let mut claude = Claude::default();
        claude.read(dir.path(), "abc");
        assert_eq!(claude.take_stopped(), ["a1", "b2"]);
        assert!(claude.take_stopped().is_empty(), "taken once");
        claude.read(dir.path(), "abc");
        assert!(claude.take_stopped().is_empty(), "nothing new");
        // A task said to be going on has not stopped; a prompt the person
        // queued is no notice.
        assert_eq!(stopped_task(&task_stopped("a1", "running")), None);
        let typed = r#"{"type":"queue-operation","operation":"enqueue","content":"what is <task-notification>"}"#;
        assert_eq!(stopped_task(typed), None);
    }
}
