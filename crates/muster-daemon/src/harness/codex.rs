//! Codex: its command line, its turn-end notice, and its window title.
//!
//! Codex runs hooks passed at launch only once the person has trusted them,
//! a trust kept in their own config, or with `--dangerously-bypass-hook-trust`,
//! which would run their untrusted hooks too. Muster uses neither
//! (`PRD-13`). It hears Codex through what needs no trust, set with `-c` for
//! the session only:
//!
//! - `notify`, a program Codex runs as each turn ends, with the turn as JSON
//!   in its last argument: its `thread-id` is the id to resume the session by.
//!   It runs for Codex's hidden title thread too, which names a thread as its
//!   first prompt arrives and usually ends before the turn does; that notice
//!   is dropped, or the session would seem finished, hear none of its own
//!   turns' ends and be resumed by a thread kept nowhere.
//! - the window title, set to the activity and the run state: `⠋ Working`,
//!   `Ready`, or `[ ! ] Action Required` while it waits on the person, for a
//!   permission and a question alike.
//!
//! `--no-daemon` keeps Codex off its shared background server, so what it
//! runs is the session's own, with the session's environment. Verified
//! against Codex 0.162.1: these flags and overrides are taken, `-C` saves
//! the question of where to resume, and the title reads as below.

use muster_state::session::HookEvent;
use serde_json::Value;
use std::fmt::Write as _;

use super::{Launch, TitleState, hook_curl};

/// Codex's arguments for `launch` in `cwd`.
pub(crate) fn arguments(launch: &Launch, cwd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let resumed = launch.agent_session.as_ref().filter(|_| launch.resume);
    if resumed.is_some() {
        args.push("resume".to_owned());
    }
    args.extend(["--no-daemon".to_owned(), "-C".to_owned(), cwd.to_owned()]);
    if launch.hooks.is_some() {
        args.extend(["-c".to_owned(), format!("notify={}", notify())]);
    }
    args.extend([
        "-c".to_owned(),
        r#"tui.terminal_title=["activity","run-state"]"#.to_owned(),
    ]);
    if let Some(id) = resumed {
        args.push(id.clone());
    }
    if let Some(prompt) = launch.prompt.as_ref().filter(|_| !launch.resume) {
        args.push(prompt.clone());
    }
    args
}

/// The `notify` program, as a TOML array: a shell posting its last
/// argument to the daemon. Inert outside a Muster session, and it never
/// fails the turn.
fn notify() -> String {
    let script = format!(
        "[ -n \"$MUSTER_HOOK_TOKEN\" ] && [ -S \"$MUSTER_HOOK_SOCKET\" ] || exit 0; \
         printf '%s' \"$1\" | {}",
        hook_curl("codex", None)
    );
    let parts = ["sh", "-c", &script, "muster-notify"];
    let items: Vec<String> = parts.iter().map(|part| toml_string(part)).collect();
    format!("[{}]", items.join(","))
}

/// `text` as a TOML basic string.
fn toml_string(text: &str) -> String {
    let mut quoted = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\t' => quoted.push_str("\\t"),
            character if character.is_control() => {
                let _ = write!(quoted, "\\u{:04X}", u32::from(character));
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

/// What a notice means: a turn's end, with the thread to resume.
pub(crate) fn translate(body: &[u8]) -> Option<(Option<String>, HookEvent)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value.get("type")?.as_str()? != "agent-turn-complete" || names_the_thread(&value) {
        return None;
    }
    let thread = value
        .get("thread-id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some((thread, HookEvent::TurnEnded))
}

/// Whether a notice is the end of Codex's hidden title thread: its whole
/// answer is the title it was asked for, `{"title": …}`, which Codex reads
/// with no other field allowed.
fn names_the_thread(notice: &Value) -> bool {
    let Some(answer) = notice.get("last-assistant-message").and_then(Value::as_str) else {
        return false;
    };
    let Ok(Value::Object(answer)) = serde_json::from_str::<Value>(answer) else {
        return false;
    };
    answer.len() == 1 && answer.get("title").is_some_and(Value::is_string)
}

/// What Codex's window title says: waiting on the person, working, or
/// ready. Anything else, such as a title the person's program set, says
/// nothing.
pub(crate) fn read_title(title: &str) -> Option<TitleState> {
    if title.contains("Action Required") {
        return Some(TitleState::Waiting);
    }
    // The activity spinner, if any, then the run state.
    let state = title.split(" | ").next()?.trim();
    let state = state
        .split_once(' ')
        .filter(|(spinner, _)| spinner.chars().count() == 1)
        .map_or(state, |(_, state)| state);
    match state {
        "Working" | "Thinking" | "Waiting" => Some(TitleState::Working),
        "Ready" | "Starting" => Some(TitleState::Ready),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::*;
    use crate::harness::{HookEndpoint, Place};

    fn launch(resume: bool, prompt: Option<&str>) -> Launch {
        Launch {
            session: SessionId::new(),
            agent_session: resume.then(|| "019a-thread".to_owned()),
            resume,
            prompt: prompt.map(str::to_owned),
            hooks: Some(HookEndpoint {
                socket: PathBuf::from("/run/hooks.sock"),
                token: crate::secrets::Token::new("t"),
            }),
            place: Place::Host(PathBuf::from("/work")),
            size: TermSize::default(),
            checks: None,
            support: None,
        }
    }

    #[test]
    fn a_new_session_takes_its_prompt_and_a_resumed_one_its_thread() {
        let fresh = arguments(&launch(false, Some("fix it")), "/work");
        assert_eq!(fresh[..3], ["--no-daemon", "-C", "/work"]);
        assert_eq!(fresh[3], "-c");
        assert!(
            fresh[4].starts_with("notify=[\"sh\",\"-c\","),
            "{}",
            fresh[4]
        );
        assert_eq!(
            fresh[5..],
            [
                "-c",
                r#"tui.terminal_title=["activity","run-state"]"#,
                "fix it"
            ]
        );

        let resumed = arguments(&launch(true, Some("ignored")), "/work");
        assert_eq!(resumed[..4], ["resume", "--no-daemon", "-C", "/work"]);
        assert_eq!(resumed.last().map(String::as_str), Some("019a-thread"));
        assert!(!resumed.iter().any(|arg| arg == "ignored"));
    }

    #[test]
    fn the_notice_program_posts_its_last_argument_and_stays_inert_outside_muster() {
        let notify = notify();
        assert!(notify.ends_with(",\"muster-notify\"]"), "{notify}");
        assert!(notify.contains("printf '%s' \\\"$1\\\""), "{notify}");
        assert!(notify.contains("http://muster/hooks/codex"), "{notify}");
        assert!(notify.contains("|| exit 0"), "{notify}");
        assert!(notify.contains("|| true"), "{notify}");
    }

    #[test]
    fn toml_strings_escape_what_they_must() {
        assert_eq!(toml_string(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        assert_eq!(toml_string("x\ny\u{1b}"), "\"x\\ny\\u001B\"");
    }

    #[test]
    fn a_turn_s_end_carries_its_thread() {
        let body = br#"{"type":"agent-turn-complete","thread-id":"019a-thread","turn-id":"t1","cwd":"/work","client":"codex-tui","input-messages":["hi"],"last-assistant-message":"done"}"#;
        assert_eq!(
            translate(body),
            Some((Some("019a-thread".to_owned()), HookEvent::TurnEnded))
        );
        assert_eq!(translate(br#"{"type":"something-else"}"#), None);
        assert_eq!(translate(b"not json"), None);
    }

    #[test]
    fn the_hidden_title_thread_s_end_is_not_the_session_s() {
        // As Codex 0.162.1 sends it, a few seconds into the session's first turn.
        let title = br#"{"type":"agent-turn-complete","thread-id":"019a-title","turn-id":"t0","cwd":"/work","client":"codex-tui","input-messages":["Generate a concise, single-line task title of at most 36 characters and under five words where possible.\n\nUser prompt:\nfix it"],"last-assistant-message":"{\"title\":\"Fix it\"}"}"#;
        assert_eq!(translate(title), None);

        // An answer that only mentions a title, or carries more, is a turn's.
        for answer in [
            r#"The title is \"Fix it\""#,
            r#"{\"title\":\"Fix it\",\"done\":true}"#,
            r#"{\"title\":7}"#,
        ] {
            let body = format!(
                r#"{{"type":"agent-turn-complete","thread-id":"019a-thread","input-messages":["name it"],"last-assistant-message":"{answer}"}}"#
            );
            assert_eq!(
                translate(body.as_bytes()),
                Some((Some("019a-thread".to_owned()), HookEvent::TurnEnded)),
                "{answer}"
            );
        }
    }

    #[test]
    fn the_title_says_working_ready_or_waiting() {
        assert_eq!(read_title("⠋ Working"), Some(TitleState::Working));
        assert_eq!(read_title("Thinking"), Some(TitleState::Working));
        assert_eq!(read_title("⠙ Waiting"), Some(TitleState::Working));
        assert_eq!(read_title("Ready"), Some(TitleState::Ready));
        assert_eq!(read_title("Starting"), Some(TitleState::Ready));
        assert_eq!(read_title("Ready | 019a-thread"), Some(TitleState::Ready));
        assert_eq!(
            read_title("[ ! ] Action Required"),
            Some(TitleState::Waiting)
        );
        assert_eq!(
            read_title("[ . ] Action Required | my-project"),
            Some(TitleState::Waiting)
        );
        assert_eq!(read_title("vim README.md"), None);
        assert_eq!(read_title(""), None);
    }
}
