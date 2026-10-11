//! Headless Claude Code's output: `claude -p --output-format stream-json
//! --verbose`, one JSON object per line, read into [`AgentEvent`]s
//! (`ARCHITECTURE.md` §7).
//!
//! Recorded from Claude Code 2.1.296 (fixtures in `fixtures/`, scrubbed of
//! paths, ids and the account's tools): a `system`/`init` line with the
//! session id, `assistant` lines with text, thinking and tool calls,
//! `user` lines with tool results, `system`/`permission_denied` when a tool
//! was not allowed, and a `result` line that ends the turn with its cost.
//! Fields are read one by one and unknown lines are skipped, so a changed
//! field loses only itself. Run with stdin closed: otherwise it waits three
//! seconds for input first.

// Wired into runs by a later M2 step; until then only its tests use it.
#![cfg_attr(not(test), allow(dead_code))]

use serde_json::Value;

/// The longest line kept; a longer one (a huge tool result) is skipped.
const MAX_LINE: usize = 8 * 1024 * 1024;

/// One thing a headless agent reported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum AgentEvent {
    /// The agent session began; resuming it later needs this id.
    SessionStarted {
        session_id: String,
        model: Option<String>,
    },
    /// Text the agent wrote.
    Text(String),
    /// The agent called a tool.
    ToolUse { name: String },
    /// A tool answered.
    ToolResult { is_error: bool },
    /// A tool call was refused for want of permission.
    PermissionDenied { tool: String },
    /// The turn ended.
    TurnFinished(TurnEnd),
}

/// How a headless turn ended.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TurnEnd {
    pub(crate) outcome: TurnOutcome,
    /// The final text, when it succeeded.
    pub(crate) result: Option<String>,
    /// What the session has cost so far, in millionths of a dollar.
    pub(crate) cost_micros: Option<u64>,
    pub(crate) turns: Option<u32>,
    /// Tool calls refused in the turn.
    pub(crate) denials: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TurnOutcome {
    Success,
    /// It ran out of turns before finishing.
    MaxTurns,
    /// Anything else that went wrong, with what the agent said about it.
    Error {
        kind: String,
        message: String,
    },
}

/// Reads one line of output; `None` for lines that mean nothing to Muster.
pub(crate) fn parse_line(line: &str) -> Option<AgentEvent> {
    let value: Value = serde_json::from_str(line.trim()).ok()?;
    let text = |value: &Value, key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    match (
        text(&value, "type")?.as_str(),
        text(&value, "subtype").as_deref(),
    ) {
        ("system", Some("init")) => Some(AgentEvent::SessionStarted {
            session_id: text(&value, "session_id")?,
            model: text(&value, "model"),
        }),
        ("system", Some("permission_denied")) => Some(AgentEvent::PermissionDenied {
            tool: text(&value, "tool_name").unwrap_or_else(|| "a tool".to_owned()),
        }),
        ("result", subtype) => Some(AgentEvent::TurnFinished(turn_end(&value, subtype))),
        _ => None,
    }
}

/// Reads every event in one line: an `assistant` or `user` line can carry
/// several blocks.
pub(crate) fn parse_events(line: &str) -> Vec<AgentEvent> {
    if let Some(event) = parse_line(line) {
        return vec![event];
    }
    let Ok(value) = serde_json::from_str::<Value>(line.trim()) else {
        return Vec::new();
    };
    let kind = value.get("type").and_then(Value::as_str);
    let blocks = value
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_array);
    let Some(blocks) = blocks else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter_map(|block| {
            let block_type = block.get("type").and_then(Value::as_str)?;
            match (kind, block_type) {
                (Some("assistant"), "text") => block
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.is_empty())
                    .map(|text| AgentEvent::Text(text.to_owned())),
                (Some("assistant"), "tool_use") => Some(AgentEvent::ToolUse {
                    name: block.get("name").and_then(Value::as_str)?.to_owned(),
                }),
                (Some("user"), "tool_result") => Some(AgentEvent::ToolResult {
                    is_error: block
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                }),
                _ => None,
            }
        })
        .collect()
}

fn turn_end(value: &Value, subtype: Option<&str>) -> TurnEnd {
    let is_error = value
        .get("is_error")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let errors: Vec<String> = value
        .get("errors")
        .and_then(Value::as_array)
        .map(|errors| {
            errors
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let result = value
        .get("result")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let outcome = match subtype {
        Some("success") if !is_error => TurnOutcome::Success,
        Some("error_max_turns") => TurnOutcome::MaxTurns,
        other => TurnOutcome::Error {
            kind: other.unwrap_or("unknown").to_owned(),
            message: if errors.is_empty() {
                result
                    .clone()
                    .unwrap_or_else(|| "no reason given".to_owned())
            } else {
                errors.join("; ")
            },
        },
    };
    let cost_micros = value
        .get("total_cost_usd")
        .and_then(Value::as_f64)
        .filter(|usd| usd.is_finite() && *usd >= 0.0)
        .map(|usd| (usd * 1_000_000.0).round() as u64);
    TurnEnd {
        result: (outcome == TurnOutcome::Success)
            .then_some(result)
            .flatten(),
        outcome,
        cost_micros,
        turns: value
            .get("num_turns")
            .and_then(Value::as_u64)
            .and_then(|turns| u32::try_from(turns).ok()),
        denials: value
            .get("permission_denials")
            .and_then(Value::as_array)
            .map_or(0, Vec::len),
    }
}

/// Splits output that arrives in pieces into whole lines.
#[derive(Debug, Default)]
pub(crate) struct Lines {
    partial: Vec<u8>,
    /// The line being read is too long and is being skipped to its end.
    skipping: bool,
}

impl Lines {
    /// The whole lines `data` completes. A line longer than the limit is
    /// skipped, and a line that is not UTF-8 is read lossily.
    pub(crate) fn push(&mut self, data: &[u8]) -> Vec<String> {
        let mut lines = Vec::new();
        for piece in data.split_inclusive(|&byte| byte == b'\n') {
            let ends = piece.last() == Some(&b'\n');
            let body = if ends {
                &piece[..piece.len() - 1]
            } else {
                piece
            };
            if !self.skipping {
                if self.partial.len() + body.len() > MAX_LINE {
                    self.partial.clear();
                    self.skipping = true;
                } else {
                    self.partial.extend_from_slice(body);
                }
            }
            if ends {
                if !self.skipping && !self.partial.is_empty() {
                    lines.push(String::from_utf8_lossy(&self.partial).into_owned());
                }
                self.partial.clear();
                self.skipping = false;
            }
        }
        lines
    }

    /// Whatever is left once the output has ended: a last line without its
    /// newline.
    pub(crate) fn finish(&mut self) -> Option<String> {
        let rest = std::mem::take(&mut self.partial);
        let skipped = std::mem::take(&mut self.skipping);
        (!skipped && !rest.is_empty()).then(|| String::from_utf8_lossy(&rest).into_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SUCCESS: &str = include_str!("fixtures/claude-success.jsonl");
    const TOOLS: &str = include_str!("fixtures/claude-tools.jsonl");
    const MAX_TURNS: &str = include_str!("fixtures/claude-max_turns.jsonl");

    fn events(output: &str) -> Vec<AgentEvent> {
        let mut lines = Lines::default();
        let mut events: Vec<AgentEvent> = Vec::new();
        // Fed in awkward pieces, as a pipe delivers them.
        for piece in output.as_bytes().chunks(97) {
            for line in lines.push(piece) {
                events.extend(parse_events(&line));
            }
        }
        if let Some(line) = lines.finish() {
            events.extend(parse_events(&line));
        }
        events
    }

    fn end(events: &[AgentEvent]) -> &TurnEnd {
        match events.last() {
            Some(AgentEvent::TurnFinished(end)) => end,
            other => panic!("the last event is not the turn's end: {other:?}"),
        }
    }

    #[test]
    fn a_short_turn_reads_as_start_text_and_success_with_its_cost() {
        let events = events(SUCCESS);
        assert_eq!(
            events[0],
            AgentEvent::SessionStarted {
                session_id: "00000000-0000-4000-8000-000000000001".to_owned(),
                model: Some("claude-opus-5-5".to_owned()),
            }
        );
        assert_eq!(events[1], AgentEvent::Text("ok".to_owned()));
        assert_eq!(events.len(), 3);
        let end = end(&events);
        assert_eq!(end.outcome, TurnOutcome::Success);
        assert_eq!(end.result.as_deref(), Some("ok"));
        assert_eq!(end.cost_micros, Some(200_648));
        assert_eq!(end.turns, Some(1));
        assert_eq!(end.denials, 0);
    }

    #[test]
    fn tool_calls_their_results_and_refusals_are_read() {
        let events = events(TOOLS);
        let tools: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolUse { name } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(tools, ["Write", "Bash"]);
        let results: Vec<_> = events
            .iter()
            .filter_map(|event| match event {
                AgentEvent::ToolResult { is_error } => Some(*is_error),
                _ => None,
            })
            .collect();
        assert_eq!(results, [false, true]);
        assert!(events.contains(&AgentEvent::PermissionDenied {
            tool: "Bash".to_owned()
        }));
        let end = end(&events);
        assert_eq!(end.outcome, TurnOutcome::Success);
        assert_eq!(end.denials, 1);
        assert_eq!(end.turns, Some(3));
    }

    #[test]
    fn running_out_of_turns_is_not_success() {
        let events = events(MAX_TURNS);
        let end = end(&events);
        assert_eq!(end.outcome, TurnOutcome::MaxTurns);
        assert_eq!(end.result, None);
        assert!(end.cost_micros.is_some());
    }

    #[test]
    fn errors_and_drift_are_read_field_by_field() {
        let error = r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["API Error: 529 overloaded"],"total_cost_usd":"lots"}"#;
        let Some(AgentEvent::TurnFinished(end)) = parse_line(error) else {
            panic!("not an end");
        };
        assert_eq!(
            end.outcome,
            TurnOutcome::Error {
                kind: "error_during_execution".to_owned(),
                message: "API Error: 529 overloaded".to_owned()
            }
        );
        assert_eq!(end.cost_micros, None, "a cost of the wrong type is missing");
        // A success flagged as an error is not a success.
        let flagged = r#"{"type":"result","subtype":"success","is_error":true,"result":"Prompt is too long"}"#;
        let Some(AgentEvent::TurnFinished(end)) = parse_line(flagged) else {
            panic!("not an end");
        };
        assert!(
            matches!(end.outcome, TurnOutcome::Error { ref message, .. } if message == "Prompt is too long")
        );
        for nothing in [
            "",
            "not json",
            "[1]",
            r#"{"type":"stream_event"}"#,
            r#"{"type":"system","subtype":"init"}"#,
        ] {
            assert!(parse_events(nothing).is_empty(), "{nothing}");
        }
    }

    #[test]
    fn lines_come_whole_and_an_endless_one_is_skipped() {
        let mut lines = Lines::default();
        assert!(lines.push(b"{\"a\":").is_empty());
        assert_eq!(
            lines.push(b"1}\n{\"b\":2}\n{\"c\""),
            ["{\"a\":1}", "{\"b\":2}"]
        );
        assert_eq!(lines.finish().as_deref(), Some("{\"c\""));
        assert_eq!(lines.finish(), None);
        // Blank lines are nothing.
        assert!(lines.push(b"\n\n").is_empty());

        let huge = vec![b'x'; MAX_LINE + 10];
        assert!(lines.push(&huge).is_empty());
        assert!(lines.push(b"still the same line").is_empty());
        assert_eq!(lines.push(b"\nnext\n"), ["next"]);
    }
}
