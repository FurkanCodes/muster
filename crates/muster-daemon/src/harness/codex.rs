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
                quoted.push_str(&format!("\\u{:04X}", u32::from(character)));
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
    if value.get("type")?.as_str()? != "agent-turn-complete" {
        return None;
    }
    let thread = value
        .get("thread-id")
        .and_then(Value::as_str)
        .map(str::to_owned);
    Some((thread, HookEvent::TurnEnded))
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
                token: "t".to_owned(),
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
