//! GitHub Copilot CLI: its command line, its hooks, and what they mean.
//!
//! Hooks come from a plugin of Muster's own, a folder loaded for the
//! session only with `--plugin-dir`, never installed (`PRD-13`). Copilot
//! sends no event name in a hook's payload, so each hook posts to its own
//! address. Copilot also reports busy and idle with the progress sequence,
//! which Muster reads as it does Claude Code's.
//!
//! Verified against Copilot CLI 1.0.95: `--plugin-dir` loads the plugin and
//! its `hooks/hooks.json` (its `/hooks` lists them), `--session-id` takes
//! the id Muster chooses and `--resume` takes it back. Its first hook,
//! `sessionStart`, comes with the first prompt, not at launch, so Muster
//! does not wait for word of its start. The payload fields read below are
//! those its documentation gives, in either case it may write them in.

use muster_state::session::{HookEvent, Origin};
use serde_json::{Value, json};

use super::{Launch, hook_command};

/// Copilot's tool for asking the person a question.
const QUESTION_TOOL: &str = "ask_user";

/// The hooks Muster installs.
const EVENTS: &[&str] = &[
    "sessionStart",
    "userPromptSubmitted",
    "agentStop",
    "permissionRequest",
    "notification",
    "preToolUse",
    "postToolUse",
    "postToolUseFailure",
];

/// Copilot's arguments for `launch`, with the plugin folder `plugin`.
pub(crate) fn arguments(launch: &Launch, plugin: Option<&str>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(id) = &launch.agent_session {
        let flag = if launch.resume {
            "--resume"
        } else {
            "--session-id"
        };
        args.push(format!("{flag}={id}"));
    }
    if let Some(plugin) = plugin.filter(|_| launch.hooks.is_some()) {
        args.extend(["--plugin-dir".to_owned(), plugin.to_owned()]);
    }
    if let Some(prompt) = launch.prompt.as_ref().filter(|_| !launch.resume) {
        args.extend(["--interactive".to_owned(), prompt.clone()]);
    }
    args
}

/// The plugin's manifest.
pub(crate) fn manifest() -> String {
    json!({
        "name": "muster-status",
        "version": "1.0.0",
        "description": "Reports this session's state to the Muster daemon that started it."
    })
    .to_string()
}

/// The plugin's hooks: each event posts to its own address.
pub(crate) fn hooks() -> String {
    let mut hooks = serde_json::Map::new();
    for event in EVENTS {
        hooks.insert(
            (*event).to_owned(),
            json!([{
                "type": "command",
                "bash": hook_command("copilot", Some(event)),
                "timeoutSec": 5
            }]),
        );
    }
    serde_json::to_string_pretty(&json!({ "version": 1, "hooks": hooks })).unwrap_or_default()
}

/// What hook `event`'s payload means to the session machine, with the
/// Copilot session it came from.
pub(crate) fn translate(event: &str, body: &[u8]) -> Option<(Option<String>, HookEvent)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    // camelCase as documented, or snake_case as written for hooks named
    // the way Claude Code names them.
    let text = |camel: &str, snake: &str| {
        value
            .get(camel)
            .or_else(|| value.get(snake))
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let is_question = text("toolName", "tool_name").as_deref() == Some(QUESTION_TOOL);
    let main = Origin::Main;
    let event = match event {
        "sessionStart" => HookEvent::SessionStarted,
        "userPromptSubmitted" => HookEvent::PromptSubmitted,
        "agentStop" => HookEvent::TurnEnded,
        // A permission asked for the question tool is the question itself.
        "permissionRequest" if is_question => HookEvent::QuestionAsked { origin: main },
        "permissionRequest" => HookEvent::PermissionRequested {
            origin: main,
            deferred: false,
        },
        "notification" => match text("notificationType", "notification_type")?.as_str() {
            "permission_prompt" => HookEvent::PermissionRequested {
                origin: main,
                deferred: true,
            },
            "elicitation_dialog" => HookEvent::QuestionAsked { origin: main },
            "agent_idle" => HookEvent::Idle,
            _ => return None,
        },
        "preToolUse" if is_question => HookEvent::QuestionAsked { origin: main },
        "preToolUse" => HookEvent::ToolStarted { origin: main },
        "postToolUse" | "postToolUseFailure" => HookEvent::ToolFinished {
            origin: main,
            question: is_question,
        },
        _ => return None,
    };
    Some((text("sessionId", "session_id"), event))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::*;
    use crate::harness::{HookEndpoint, Place};

    const SESSION: &str = "0cb916db-26aa-40f2-86b5-1ba81b225fd2";

    fn launch(resume: bool, prompt: Option<&str>) -> Launch {
        Launch {
            session: SessionId::new(),
            agent_session: Some(SESSION.to_owned()),
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
    fn a_new_session_takes_its_id_plugin_and_prompt_and_a_resumed_one_its_id() {
        assert_eq!(
            arguments(&launch(false, Some("fix it")), Some("/run/copilot")),
            [
                format!("--session-id={SESSION}").as_str(),
                "--plugin-dir",
                "/run/copilot",
                "--interactive",
                "fix it"
            ]
        );
        assert_eq!(
            arguments(&launch(true, Some("ignored")), Some("/run/copilot")),
            [
                format!("--resume={SESSION}").as_str(),
                "--plugin-dir",
                "/run/copilot"
            ]
        );
    }

    #[test]
    fn each_hook_posts_to_its_own_address() {
        let hooks: Value = serde_json::from_str(&hooks()).unwrap();
        assert_eq!(hooks["version"], 1);
        for event in EVENTS {
            let command = hooks["hooks"][event][0]["bash"].as_str().unwrap();
            assert!(
                command.contains(&format!("http://muster/hooks/copilot/{event} ")),
                "{command}"
            );
        }
        let manifest: Value = serde_json::from_str(&manifest()).unwrap();
        assert_eq!(manifest["name"], "muster-status");
    }

    #[test]
    fn hooks_translate_in_either_case() {
        let event = |name: &str, extra: &str| {
            let body = format!(r#"{{"sessionId":"{SESSION}","timestamp":1,"cwd":"/work"{extra}}}"#);
            let (session, event) = translate(name, body.as_bytes())?;
            assert_eq!(session.as_deref(), Some(SESSION));
            Some(event)
        };
        assert_eq!(
            event("sessionStart", r#","source":"new""#),
            Some(HookEvent::SessionStarted)
        );
        assert_eq!(
            event("userPromptSubmitted", r#","prompt":"hi""#),
            Some(HookEvent::PromptSubmitted)
        );
        assert_eq!(event("agentStop", ""), Some(HookEvent::TurnEnded));
        assert_eq!(
            event("permissionRequest", r#","toolName":"shell""#),
            Some(HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: false
            })
        );
        assert_eq!(
            event(
                "notification",
                r#","notification_type":"elicitation_dialog""#
            ),
            Some(HookEvent::QuestionAsked {
                origin: Origin::Main
            })
        );
        assert_eq!(
            event("preToolUse", r#","toolName":"ask_user""#),
            Some(HookEvent::QuestionAsked {
                origin: Origin::Main
            })
        );
        assert_eq!(
            event("permissionRequest", r#","toolName":"ask_user""#),
            Some(HookEvent::QuestionAsked {
                origin: Origin::Main
            })
        );
        assert_eq!(
            event("postToolUse", r#","toolName":"ask_user""#),
            Some(HookEvent::ToolFinished {
                origin: Origin::Main,
                question: true
            })
        );
        assert_eq!(
            event("notification", r#","notification_type":"shell_completed""#),
            None
        );
        assert_eq!(event("preCompact", ""), None);
        let snake = br#"{"session_id":"s1","hook_event_name":"Stop"}"#;
        assert_eq!(
            translate("agentStop", snake),
            Some((Some("s1".to_owned()), HookEvent::TurnEnded))
        );
    }
}
