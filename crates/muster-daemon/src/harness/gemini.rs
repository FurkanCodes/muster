//! Gemini CLI: its command line, its hooks, and its window title.
//!
//! Hooks are read from a system-defaults settings file of Muster's own,
//! named by `GEMINI_CLI_SYSTEM_DEFAULTS_PATH` for the session only (`PRD-13`).
//! That file is the lowest of Gemini's settings: every setting of the
//! person's, user or project, wins over it, and hooks from all of them run
//! side by side. A person who names a defaults file of their own keeps it,
//! and the session's status comes from the title alone.
//!
//! Gemini runs settings' hooks only in a folder the person trusts; in any
//! other it first asks whether to trust it, which Muster shows as asking
//! before it starts, and then the title says whether it works or is ready.
//!
//! Verified against Gemini CLI 0.63.0: `--session-id` takes the id Muster
//! chooses, `--resume` takes it back, the payloads carry the fields read
//! below (`hookSystem.createBaseInput`, `fireNotificationEvent`), and the
//! title reads as below.

use muster_state::session::{HookEvent, Origin};
use serde_json::{Value, json};

use super::{Launch, TitleState, hook_command};

/// The variable naming the defaults file.
pub(crate) const DEFAULTS_VARIABLE: &str = "GEMINI_CLI_SYSTEM_DEFAULTS_PATH";

/// Gemini's tool for asking the person a question.
const QUESTION_TOOL: &str = "ask_user";

/// The hooks Muster installs.
const EVENTS: &[&str] = &[
    "SessionStart",
    "BeforeAgent",
    "AfterAgent",
    "Notification",
    "BeforeTool",
    "AfterTool",
];

/// Gemini's arguments for `launch`.
pub(crate) fn arguments(launch: &Launch) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(id) = &launch.agent_session {
        let flag = if launch.resume {
            "--resume"
        } else {
            "--session-id"
        };
        args.extend([flag.to_owned(), id.clone()]);
    }
    if let Some(prompt) = launch.prompt.as_ref().filter(|_| !launch.resume) {
        args.extend(["--prompt-interactive".to_owned(), prompt.clone()]);
    }
    args
}

/// The defaults file: Muster's hooks, and nothing else.
pub(crate) fn defaults() -> String {
    let command = hook_command("gemini", None);
    let mut hooks = serde_json::Map::new();
    for event in EVENTS {
        hooks.insert(
            (*event).to_owned(),
            json!([{
                "matcher": "",
                "hooks": [{ "type": "command", "name": "muster", "command": command }]
            }]),
        );
    }
    serde_json::to_string_pretty(&json!({ "hooks": hooks })).unwrap_or_default()
}

/// What a payload means to the session machine, with the Gemini session it
/// came from. Each field is read on its own: drift must mean "status stops
/// updating", never a lost event.
pub(crate) fn translate(body: &[u8]) -> Option<(Option<String>, HookEvent)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    let is_question = text("tool_name") == Some(QUESTION_TOOL);
    let event = match text("hook_event_name")? {
        "SessionStart" => HookEvent::SessionStarted,
        "BeforeAgent" => HookEvent::PromptSubmitted,
        "AfterAgent" => HookEvent::TurnEnded,
        "Notification" if text("notification_type") == Some("ToolPermission") => {
            let kind = value
                .get("details")
                .and_then(|details| details.get("type"))
                .and_then(Value::as_str);
            if kind == Some(QUESTION_TOOL) {
                HookEvent::QuestionAsked {
                    origin: Origin::Main,
                    deferred: false,
                }
            } else {
                HookEvent::PermissionRequested {
                    origin: Origin::Main,
                    deferred: false,
                }
            }
        }
        "BeforeTool" if is_question => HookEvent::QuestionAsked {
            origin: Origin::Main,
            deferred: false,
        },
        "BeforeTool" => HookEvent::ToolStarted {
            origin: Origin::Main,
        },
        "AfterTool" => HookEvent::ToolFinished {
            origin: Origin::Main,
            question: is_question,
        },
        _ => return None,
    };
    Some((text("session_id").map(str::to_owned), event))
}

/// What Gemini's window title says: working or ready. It also says when it
/// waits on the person, but so do its hooks, which say for what; and its
/// title reads "Ready" while it asks whether to trust a folder, so it is
/// taken for no word of its start.
pub(crate) fn read_title(title: &str) -> Option<TitleState> {
    let title = title.trim_start();
    if title.starts_with('⏲') || title.starts_with('✦') {
        Some(TitleState::Working)
    } else if title.starts_with('◇') {
        Some(TitleState::Ready)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::*;
    use crate::harness::Place;

    const SESSION: &str = "5b2c8e0a-1111-4222-8333-444455556666";

    fn launch(resume: bool, prompt: Option<&str>) -> Launch {
        Launch {
            session: SessionId::new(),
            agent_session: Some(SESSION.to_owned()),
            resume,
            prompt: prompt.map(str::to_owned),
            hooks: None,
            place: Place::Host(PathBuf::from("/work")),
            size: TermSize::default(),
            checks: None,
            support: None,
            ssh_agent: None,
        }
    }

    fn event(extra: &str) -> Option<HookEvent> {
        let body = format!(
            r#"{{"session_id":"{SESSION}","transcript_path":"/t.jsonl","cwd":"/work","timestamp":"2026-10-12T10:00:00Z",{extra}}}"#
        );
        let (session, event) = translate(body.as_bytes())?;
        assert_eq!(session.as_deref(), Some(SESSION));
        Some(event)
    }

    #[test]
    fn a_new_session_takes_its_id_and_prompt_and_a_resumed_one_its_id() {
        assert_eq!(
            arguments(&launch(false, Some("fix it"))),
            ["--session-id", SESSION, "--prompt-interactive", "fix it"]
        );
        assert_eq!(
            arguments(&launch(true, Some("ignored"))),
            ["--resume", SESSION]
        );
    }

    #[test]
    fn the_defaults_carry_every_hook_with_the_same_command() {
        let defaults: Value = serde_json::from_str(&defaults()).unwrap();
        let command = hook_command("gemini", None);
        for name in EVENTS {
            assert_eq!(
                defaults["hooks"][name][0]["hooks"][0]["command"], command,
                "{name}"
            );
        }
        assert_eq!(defaults.as_object().unwrap().len(), 1, "only hooks");
    }

    #[test]
    fn turns_permissions_questions_and_tools_translate() {
        assert_eq!(
            event(r#""hook_event_name":"SessionStart","source":"startup""#),
            Some(HookEvent::SessionStarted)
        );
        assert_eq!(
            event(r#""hook_event_name":"BeforeAgent","prompt":"hi""#),
            Some(HookEvent::PromptSubmitted)
        );
        assert_eq!(
            event(r#""hook_event_name":"AfterAgent","prompt":"hi","prompt_response":"ok""#),
            Some(HookEvent::TurnEnded)
        );
        assert_eq!(
            event(
                r#""hook_event_name":"Notification","notification_type":"ToolPermission","message":"Run ls?","details":{"type":"exec"}"#
            ),
            Some(HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: false
            })
        );
        assert_eq!(
            event(
                r#""hook_event_name":"Notification","notification_type":"ToolPermission","details":{"type":"ask_user"}"#
            ),
            Some(HookEvent::QuestionAsked {
                origin: Origin::Main,
                deferred: false,
            })
        );
        assert_eq!(
            event(r#""hook_event_name":"BeforeTool","tool_name":"run_shell_command""#),
            Some(HookEvent::ToolStarted {
                origin: Origin::Main
            })
        );
        assert_eq!(
            event(r#""hook_event_name":"AfterTool","tool_name":"ask_user""#),
            Some(HookEvent::ToolFinished {
                origin: Origin::Main,
                question: true
            })
        );
        assert_eq!(event(r#""hook_event_name":"PreCompress""#), None);
        assert_eq!(
            event(r#""hook_event_name":"Notification","notification_type":"Other""#),
            None
        );
        assert_eq!(translate(b"[]"), None);
    }

    #[test]
    fn the_title_says_working_or_ready() {
        assert_eq!(
            read_title("⏲  Working… (work)      "),
            Some(TitleState::Working)
        );
        assert_eq!(
            read_title("✦  Thinking about it (work)"),
            Some(TitleState::Working)
        );
        assert_eq!(read_title("◇  Ready (work)"), Some(TitleState::Ready));
        assert_eq!(read_title("✋  Action Required (work)"), None);
        assert_eq!(read_title("Gemini CLI"), None);
    }
}
