//! Claude Code: its command line, its hooks, and what its hook payloads mean.
//!
//! Hooks are passed on the command line with `--settings`, never written to
//! a file (`PRD-13`, `ARC-05`), so they exist only for sessions Muster
//! starts and leave nothing behind. Each hook posts its payload to the
//! daemon's hook socket with the session's own token (`ARC-11`).
//!
//! Verified against Claude Code 2.1.291: `--settings` with an inline JSON
//! string runs these hooks, `--session-id` is honoured and comes back as the
//! payloads' `session_id`, and the payload fields read below are present.
//!
//! The event list and its reasons are adapted from Nebula's
//! `hooks/installer.rs`, and the payload fields from its `hooks/mod.rs` (MIT,
//! Copyright (c) 2026 Cody Seibert; see NOTICE). Changed: nothing is written
//! into the worktree or the user's settings, and the token is per session
//! rather than per daemon, with the session derived from it.

use muster_state::session::{HookEvent, Origin};
use serde_json::{Value, json};

use super::Launch;

/// The question tool whose start and end bracket a wait on the person.
const QUESTION_TOOL: &str = "AskUserQuestion";

/// The hooks Muster installs, with an optional matcher.
const EVENTS: &[(&str, Option<&str>)] = &[
    ("SessionStart", None),
    ("UserPromptSubmit", None),
    ("Stop", None),
    ("PermissionRequest", None),
    // Unmatched: both `permission_prompt` and `idle_prompt` matter. The
    // second is the only signal when a turn ends without a Stop.
    ("Notification", None),
    // Only the question tool: its start is the agent asking the person.
    ("PreToolUse", Some(QUESTION_TOOL)),
    // Every tool's end, unmatched: an approved permission fires no hook of
    // its own, so the approved tool finishing is the first word of it; and
    // the question tool finishing is the answer.
    ("PostToolUse", None),
    ("SubagentStart", None),
    ("SubagentStop", None),
];

/// The command each hook runs. It does nothing outside a Muster session,
/// never fails the agent's turn, and prints nothing (a hook's output can
/// reach the model).
pub(crate) fn hook_command() -> String {
    super::hook_command("claude", None)
}

/// The `--settings` value carrying the hooks.
pub(crate) fn settings() -> String {
    let command = hook_command();
    let mut hooks = serde_json::Map::new();
    for (event, matcher) in EVENTS {
        let mut group = json!({ "hooks": [{ "type": "command", "command": command }] });
        if let (Some(matcher), Some(group)) = (matcher, group.as_object_mut()) {
            group.insert("matcher".to_owned(), Value::from(*matcher));
        }
        hooks.insert((*event).to_owned(), json!([group]));
    }
    json!({ "hooks": hooks }).to_string()
}

/// Claude Code's arguments for `launch`.
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
    if launch.hooks.is_some() {
        args.extend(["--settings".to_owned(), settings()]);
    }
    if let Some(prompt) = launch.prompt.as_ref().filter(|_| !launch.resume) {
        args.push(prompt.clone());
    }
    args
}

/// A new session id in the form Claude Code expects: a version-4 UUID.
pub(crate) fn new_session_id() -> std::io::Result<String> {
    use std::io::Read;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

/// The fields of a hook payload Muster reads. Each is read on its own and
/// unknown ones are ignored: payloads change between Claude Code versions,
/// and drift must mean "status stops updating", never a lost event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HookPayload {
    pub(crate) hook_event_name: Option<String>,
    pub(crate) session_id: Option<String>,
    pub(crate) notification_type: Option<String>,
    pub(crate) tool_name: Option<String>,
    /// Set when a subagent raised the event.
    pub(crate) agent_id: Option<String>,
}

impl HookPayload {
    /// Reads a payload. A field of an unexpected type counts as missing;
    /// only a body that is not a JSON object is refused.
    pub(crate) fn parse(body: &[u8]) -> Option<Self> {
        let value: Value = serde_json::from_slice(body).ok()?;
        let object = value.as_object()?;
        let text = |key: &str| object.get(key).and_then(Value::as_str).map(str::to_owned);
        Some(Self {
            hook_event_name: text("hook_event_name"),
            session_id: text("session_id"),
            notification_type: text("notification_type"),
            tool_name: text("tool_name"),
            agent_id: text("agent_id"),
        })
    }
}

/// What a payload means to the session machine, with the harness session
/// it came from. `None` for events the machine has no use for.
pub(crate) fn translate(payload: &HookPayload) -> Option<(Option<String>, HookEvent)> {
    let origin = || match &payload.agent_id {
        Some(id) => Origin::Subagent(id.clone()),
        None => Origin::Main,
    };
    let is_question = payload.tool_name.as_deref() == Some(QUESTION_TOOL);
    let event = match payload.hook_event_name.as_deref()? {
        "SessionStart" => HookEvent::SessionStarted,
        "UserPromptSubmit" => HookEvent::PromptSubmitted,
        "Stop" => HookEvent::TurnEnded,
        "PermissionRequest" => HookEvent::PermissionRequested {
            origin: origin(),
            deferred: false,
        },
        "Notification" => match payload.notification_type.as_deref()? {
            "permission_prompt" => HookEvent::PermissionRequested {
                origin: origin(),
                deferred: true,
            },
            "idle_prompt" => HookEvent::Idle,
            _ => return None,
        },
        "PreToolUse" if is_question => HookEvent::QuestionAsked { origin: origin() },
        "PreToolUse" => HookEvent::ToolStarted { origin: origin() },
        "PostToolUse" => HookEvent::ToolFinished {
            origin: origin(),
            question: is_question,
        },
        "SubagentStart" => HookEvent::SubagentStarted {
            id: payload.agent_id.clone()?,
        },
        "SubagentStop" => HookEvent::SubagentStopped {
            id: payload.agent_id.clone()?,
        },
        _ => return None,
    };
    Some((payload.session_id.clone(), event))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::*;
    use crate::harness::HookEndpoint;

    const SESSION: &str = "6f3c2a10-8b1e-4c5d-9a7f-0123456789ab";

    fn payload(json: &str) -> HookPayload {
        HookPayload::parse(json.as_bytes()).unwrap()
    }

    fn event(json: &str) -> HookEvent {
        let (session, event) = translate(&payload(json)).expect("an event");
        assert_eq!(session.as_deref(), Some(SESSION));
        event
    }

    // Shaped like the payloads Claude Code 2.1.291 sent, paths changed.

    #[test]
    fn real_payloads_translate() {
        let stop = r#"{"session_id":"6f3c2a10-8b1e-4c5d-9a7f-0123456789ab","transcript_path":"/home/u/.claude/projects/-work/6f3c2a10-8b1e-4c5d-9a7f-0123456789ab.jsonl","cwd":"/work","permission_mode":"default","hook_event_name":"Stop","stop_hook_active":false,"last_assistant_message":"done","background_tasks":[],"session_crons":[],"prompt_id":"p1"}"#;
        assert_eq!(event(stop), HookEvent::TurnEnded);

        let post = r#"{"session_id":"6f3c2a10-8b1e-4c5d-9a7f-0123456789ab","cwd":"/work","hook_event_name":"PostToolUse","tool_name":"Bash","tool_input":{"command":"echo probe-ok"},"tool_response":{"stdout":"probe-ok"},"tool_use_id":"t1","duration_ms":12}"#;
        assert_eq!(
            event(post),
            HookEvent::ToolFinished {
                origin: Origin::Main,
                question: false,
            }
        );

        let start = r#"{"session_id":"6f3c2a10-8b1e-4c5d-9a7f-0123456789ab","cwd":"/work","hook_event_name":"SessionStart","source":"startup"}"#;
        assert_eq!(event(start), HookEvent::SessionStarted);

        let prompt = r#"{"session_id":"6f3c2a10-8b1e-4c5d-9a7f-0123456789ab","hook_event_name":"UserPromptSubmit","prompt":"hi","prompt_id":"p1"}"#;
        assert_eq!(event(prompt), HookEvent::PromptSubmitted);
    }

    #[test]
    fn waits_on_the_person_and_their_answers_translate() {
        let base = |extra: &str| format!(r#"{{"session_id":"{SESSION}",{extra}}}"#);
        assert_eq!(
            event(&base(
                r#""hook_event_name":"PermissionRequest","tool_name":"Bash""#
            )),
            HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: false,
            }
        );
        assert_eq!(
            event(&base(
                r#""hook_event_name":"Notification","notification_type":"permission_prompt""#
            )),
            HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: true,
            }
        );
        assert_eq!(
            event(&base(
                r#""hook_event_name":"Notification","notification_type":"idle_prompt""#
            )),
            HookEvent::Idle
        );
        assert_eq!(
            event(&base(
                r#""hook_event_name":"PreToolUse","tool_name":"AskUserQuestion","agent_id":"a1""#
            )),
            HookEvent::QuestionAsked {
                origin: Origin::Subagent("a1".to_owned()),
            }
        );
        assert_eq!(
            event(&base(
                r#""hook_event_name":"PostToolUse","tool_name":"AskUserQuestion""#
            )),
            HookEvent::ToolFinished {
                origin: Origin::Main,
                question: true,
            }
        );
        assert_eq!(
            event(&base(r#""hook_event_name":"SubagentStop","agent_id":"a1""#)),
            HookEvent::SubagentStopped {
                id: "a1".to_owned(),
            }
        );
    }

    #[test]
    fn unknown_or_incomplete_payloads_are_ignored() {
        for json in [
            r#"{"hook_event_name":"PreCompact"}"#,
            r#"{"hook_event_name":"Notification","notification_type":"auth_success"}"#,
            r#"{"hook_event_name":"SubagentStart"}"#,
            r#"{"session_id":"x"}"#,
        ] {
            assert_eq!(translate(&payload(json)), None, "{json}");
        }
        // A field of the wrong type is missing, and the rest still count.
        let drifted = payload(r#"{"hook_event_name":"Stop","session_id":"x","tool_name":7}"#);
        assert_eq!(drifted.tool_name, None);
        assert!(translate(&drifted).is_some());
        assert_eq!(HookPayload::parse(b"[1, 2]"), None);
        assert_eq!(HookPayload::parse(b"not json"), None);
    }

    #[test]
    fn settings_carry_every_hook_with_the_same_command() {
        let settings: Value = serde_json::from_str(&settings()).unwrap();
        let hooks = settings["hooks"].as_object().unwrap();
        let events: Vec<_> = hooks.keys().map(String::as_str).collect();
        for (event, _) in EVENTS {
            assert!(events.contains(event), "{event}");
        }
        assert_eq!(hooks["PreToolUse"][0]["matcher"], QUESTION_TOOL);
        assert!(hooks["Stop"][0].get("matcher").is_none());
        assert_eq!(hooks["Stop"][0]["hooks"][0]["command"], hook_command());
    }

    #[test]
    fn the_hook_command_is_inert_outside_muster_and_never_fails() {
        let command = hook_command();
        assert!(command.starts_with("[ -n \"$MUSTER_HOOK_TOKEN\" ]"));
        assert!(command.contains("--unix-socket \"$MUSTER_HOOK_SOCKET\""));
        assert!(command.ends_with("|| true"));
        let status = std::process::Command::new("sh")
            .args(["-c", &command])
            .env_remove("MUSTER_HOOK_TOKEN")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn arguments_start_resume_and_prompt() {
        let launch = |resume, prompt: Option<&str>| Launch {
            session: SessionId::new(),
            agent_session: Some(SESSION.to_owned()),
            resume,
            prompt: prompt.map(str::to_owned),
            hooks: Some(HookEndpoint {
                socket: PathBuf::from("/s"),
                token: "t".to_owned(),
            }),
            place: crate::harness::Place::Host(PathBuf::from("/w")),
            size: TermSize::default(),
            checks: None,
            support: None,
        };
        let fresh = arguments(&launch(false, Some("fix the login")));
        assert_eq!(&fresh[..2], ["--session-id", SESSION]);
        assert_eq!(fresh[2], "--settings");
        assert_eq!(fresh.last().map(String::as_str), Some("fix the login"));

        let resumed = arguments(&launch(true, Some("ignored on resume")));
        assert_eq!(&resumed[..2], ["--resume", SESSION]);
        assert_eq!(resumed.len(), 4);
    }

    #[test]
    fn session_ids_are_version_4_uuids() {
        let id = new_session_id().unwrap();
        let parts: Vec<_> = id.split('-').map(str::len).collect();
        assert_eq!(parts, [8, 4, 4, 4, 12]);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
        assert_ne!(id, new_session_id().unwrap());
    }
}
