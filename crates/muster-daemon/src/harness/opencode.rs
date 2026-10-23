//! `OpenCode`: its command line, its plugin, and what the plugin reports.
//!
//! `OpenCode` hears from plugins, never hooks. Muster's plugin is a file of
//! its own, added for the session only through `OPENCODE_CONFIG_CONTENT`,
//! which `OpenCode` merges over the person's config with plugin lists joined,
//! not replaced (`PRD-13`); a value the person set is kept and the plugin
//! added to it. The plugin posts the events that matter to the daemon's
//! hook socket with the session's token, read from its environment.
//!
//! `OpenCode` chooses its own session ids. The plugin reports each session
//! as it starts; a subagent's is a child session, which the plugin names by
//! the session at the root of it, so the machine sees one agent session
//! with subagents in it.
//!
//! At each turn's end the plugin also says what the session and its
//! subagents' sessions have used, as `OpenCode` counts it through its own
//! API: every token and the cost (`PRD-12`), which it alone of the agents
//! reports.
//!
//! Verified against `OpenCode` 1.4.6: the variable loads a `file://` plugin,
//! which sees the session's environment; the event names and fields read
//! below are those of its bus; the client's `session.children` and
//! `session.messages`, and a message's `cost` and `tokens`, are those of
//! its SDK's types.

use muster_core::Spend;
use muster_state::session::{HookEvent, Origin};
use serde_json::{Value, json};

use super::Launch;

/// The variable carrying config for this session only.
pub(crate) const CONFIG_VARIABLE: &str = "OPENCODE_CONFIG_CONTENT";

/// `OpenCode`'s tool for asking the person a question.
const QUESTION_TOOL: &str = "question";

/// `OpenCode`'s arguments for `launch` in `cwd`.
pub(crate) fn arguments(launch: &Launch, cwd: &str) -> Vec<String> {
    let mut args = vec![cwd.to_owned()];
    match (&launch.agent_session, launch.resume, &launch.prompt) {
        (Some(id), true, _) => args.extend(["--session".to_owned(), id.clone()]),
        (_, false, Some(prompt)) => args.extend(["--prompt".to_owned(), prompt.clone()]),
        _ => {}
    }
    args
}

/// The config to set for the session: the person's own, if they set any,
/// with Muster's plugin added. A value that is not a JSON object is theirs
/// to keep as it is, and the session goes without the plugin.
pub(crate) fn config_content(own: Option<&str>, plugin: &str) -> Option<String> {
    let mut config = match own.filter(|own| !own.trim().is_empty()) {
        Some(own) => serde_json::from_str::<Value>(own).ok()?,
        None => json!({}),
    };
    let object = config.as_object_mut()?;
    let plugins = object
        .entry("plugin")
        .or_insert_with(|| Value::Array(Vec::new()));
    let spec = Value::from(format!("file://{plugin}"));
    match plugins {
        Value::Array(list) if !list.contains(&spec) => list.push(spec),
        Value::Array(_) => {}
        _ => return None,
    }
    Some(config.to_string())
}

/// The plugin. Plain JavaScript with no imports, run by `OpenCode`'s Bun: it
/// posts with `fetch` over the hook socket, never waits on the daemon for
/// long, and swallows every failure, so it can never hold up or break a
/// session. Streamed text is never sent.
pub(crate) const PLUGIN: &str = r#"// Muster's plugin: reports this OpenCode session's state to the Muster
// daemon that started it. Inert anywhere else.
export const MusterStatus = async ({ client } = {}) => {
  const socket = process.env.MUSTER_HOOK_SOCKET;
  const token = process.env.MUSTER_HOOK_TOKEN;
  if (!socket || !token) return {};
  const parents = new Map();
  const root = (id) => {
    let seen = 0;
    while (id && parents.has(id) && seen++ < 64) id = parents.get(id);
    return id;
  };
  const send = (type, session, extra) => {
    const at = root(session);
    const body = { type, session: at ?? null, child: session && session !== at ? session : null, ...extra };
    fetch("http://muster/hooks/opencode", {
      method: "POST",
      unix: socket,
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout(3000),
    }).catch(() => {});
  };
  // What a root session and every session under it used, as OpenCode
  // itself counts it.
  const spend = async (session) => {
    try {
      let tokens = 0;
      let cost = 0;
      const ids = [session];
      for (let at = 0; at < ids.length && at < 256; at++) {
        const children = await client.session.children({ path: { id: ids[at] } });
        for (const child of children?.data ?? []) if (child?.id) ids.push(child.id);
        const listed = await client.session.messages({ path: { id: ids[at] } });
        for (const message of listed?.data ?? []) {
          const info = message?.info ?? {};
          if (info.role !== "assistant") continue;
          const used = info.tokens ?? {};
          tokens += (used.input ?? 0) + (used.output ?? 0) + (used.reasoning ?? 0)
            + (used.cache?.read ?? 0) + (used.cache?.write ?? 0);
          cost += info.cost ?? 0;
        }
      }
      send("muster.spend", session, { tokens, cost });
    } catch {}
  };
  const wanted = new Set([
    "session.idle",
    "permission.asked",
    "permission.replied",
    "question.asked",
    "question.replied",
    "question.rejected",
  ]);
  send("muster.loaded", null, {});
  return {
    event: async ({ event }) => {
      const props = event?.properties ?? {};
      if (event?.type === "session.created") {
        const info = props.info ?? {};
        if (info.parentID) parents.set(info.id, info.parentID);
        send("session.created", info.id, { parent: info.parentID ?? null });
      } else if (wanted.has(event?.type)) {
        send(event.type, props.sessionID, {});
        if (event.type === "session.idle" && client?.session && !parents.has(props.sessionID)) {
          spend(props.sessionID);
        }
      }
    },
    "chat.message": async (input) => send("chat.message", input?.sessionID, {}),
    "tool.execute.before": async (input) => send("tool.before", input?.sessionID, { tool: input?.tool ?? null }),
    "tool.execute.after": async (input) => send("tool.after", input?.sessionID, { tool: input?.tool ?? null }),
  };
};
"#;

/// What a turn's end said the session has used: every token, and its cost
/// in dollars, kept to millionths.
pub(crate) fn spend(body: &[u8]) -> Option<Spend> {
    let value: Value = serde_json::from_slice(body).ok()?;
    if value.get("type")?.as_str()? != "muster.spend" {
        return None;
    }
    let tokens = value.get("tokens")?.as_f64()?;
    let cost = value.get("cost")?.as_f64()?;
    let sane = |amount: f64, most: f64| amount.is_finite() && (0.0..most).contains(&amount);
    if !sane(tokens, 1e15) || !sane(cost, 1e9) {
        return None;
    }
    Some(Spend {
        tokens: muster_core::num::round_u64(tokens),
        cost_micros: Some(muster_core::num::round_u64(cost * 1_000_000.0)),
    })
}

/// What a report means to the session machine, with the root session it
/// came from.
pub(crate) fn translate(body: &[u8]) -> Option<(Option<String>, HookEvent)> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let text = |key: &str| value.get(key).and_then(Value::as_str).map(str::to_owned);
    let session = text("session");
    let child = text("child");
    let origin = || match &child {
        Some(id) => Origin::Subagent(id.clone()),
        None => Origin::Main,
    };
    let is_question = text("tool").as_deref() == Some(QUESTION_TOOL);
    let event = match text("type")?.as_str() {
        "muster.loaded" => return Some((None, HookEvent::SessionStarted)),
        // A child session is a subagent starting; a root one, a session
        // starting in the process, at its first prompt or after `/new`.
        "session.created" => match &child {
            Some(id) => HookEvent::SubagentStarted { id: id.clone() },
            None => HookEvent::SessionStarted,
        },
        "chat.message" if child.is_none() => HookEvent::PromptSubmitted,
        "session.idle" => match &child {
            Some(id) => HookEvent::SubagentStopped { id: id.clone() },
            None => HookEvent::TurnEnded,
        },
        "permission.asked" => HookEvent::PermissionRequested {
            origin: origin(),
            deferred: false,
        },
        // Answered either way, the tool goes on or the turn does.
        "permission.replied" => HookEvent::ToolStarted { origin: origin() },
        "question.asked" => HookEvent::QuestionAsked {
            origin: origin(),
            deferred: false,
        },
        "question.replied" | "question.rejected" => HookEvent::ToolFinished {
            origin: origin(),
            question: true,
        },
        "tool.before" if !is_question => HookEvent::ToolStarted { origin: origin() },
        "tool.after" => HookEvent::ToolFinished {
            origin: origin(),
            question: is_question,
        },
        _ => return None,
    };
    Some((session, event))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use muster_core::SessionId;
    use muster_core::protocol::TermSize;

    use super::*;
    use crate::harness::Place;

    fn launch(agent_session: Option<&str>, resume: bool, prompt: Option<&str>) -> Launch {
        Launch {
            session: SessionId::new(),
            agent_session: agent_session.map(str::to_owned),
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

    fn event(json: &str) -> Option<(Option<String>, HookEvent)> {
        translate(json.as_bytes())
    }

    #[test]
    fn a_new_session_takes_its_prompt_and_a_resumed_one_its_session() {
        assert_eq!(
            arguments(&launch(None, false, Some("fix it")), "/work"),
            ["/work", "--prompt", "fix it"]
        );
        assert_eq!(
            arguments(&launch(Some("ses_1"), true, Some("ignored")), "/work"),
            ["/work", "--session", "ses_1"]
        );
        assert_eq!(arguments(&launch(None, false, None), "/work"), ["/work"]);
    }

    #[test]
    fn the_plugin_is_added_to_the_person_s_own_config() {
        let plugin = "/run/muster/opencode/muster.js";
        let fresh: Value = serde_json::from_str(&config_content(None, plugin).unwrap()).unwrap();
        assert_eq!(
            fresh,
            json!({ "plugin": ["file:///run/muster/opencode/muster.js"] })
        );

        let own = r#"{"model":"anthropic/x","plugin":["their-plugin"]}"#;
        let merged: Value =
            serde_json::from_str(&config_content(Some(own), plugin).unwrap()).unwrap();
        assert_eq!(merged["model"], "anthropic/x");
        assert_eq!(
            merged["plugin"],
            json!(["their-plugin", "file:///run/muster/opencode/muster.js"])
        );
        // Theirs, if it is not an object Muster can add to, stays as it is.
        assert_eq!(config_content(Some("not json"), plugin), None);
        assert_eq!(config_content(Some(r#"{"plugin":"x"}"#), plugin), None);
    }

    #[test]
    fn the_plugin_is_inert_outside_muster_and_sends_no_streamed_text() {
        assert!(PLUGIN.contains("if (!socket || !token) return {};"));
        assert!(PLUGIN.contains("http://muster/hooks/opencode"));
        assert!(PLUGIN.contains(".catch(() => {})"));
        assert!(!PLUGIN.contains("message.part"));
        assert!(!PLUGIN.contains("import "));
    }

    #[test]
    fn a_turn_s_end_says_what_the_session_used_tokens_and_cost() {
        let spend = |json: &str| {
            super::spend(json.as_bytes()).map(|spend| (spend.tokens, spend.cost_micros))
        };
        assert_eq!(
            spend(
                r#"{"type":"muster.spend","session":"ses_root","child":null,"tokens":152340,"cost":0.4213}"#
            ),
            Some((152_340, Some(421_300)))
        );
        assert_eq!(
            spend(r#"{"type":"muster.spend","session":"s","tokens":10,"cost":0}"#),
            Some((10, Some(0)))
        );
        for odd in [
            r#"{"type":"muster.spend","session":"s","tokens":-5,"cost":0.1}"#,
            r#"{"type":"muster.spend","session":"s","tokens":5,"cost":"lots"}"#,
            r#"{"type":"muster.spend","session":"s","tokens":5}"#,
            r#"{"type":"session.idle","session":"s","tokens":5,"cost":0.1}"#,
        ] {
            assert_eq!(spend(odd), None, "{odd}");
        }
        // Not news to the session machine.
        assert_eq!(
            event(r#"{"type":"muster.spend","session":"s","child":null,"tokens":5,"cost":0.1}"#),
            None
        );
        assert!(PLUGIN.contains("client.session.messages({ path: { id: ids[at] } })"));
        assert!(PLUGIN.contains("send(\"muster.spend\", session, { tokens, cost })"));
    }

    #[test]
    fn reports_translate_with_the_root_session() {
        assert_eq!(
            event(r#"{"type":"muster.loaded","session":null,"child":null}"#),
            Some((None, HookEvent::SessionStarted))
        );
        let root = |json: &str| {
            let (session, event) = event(json).expect("an event");
            assert_eq!(session.as_deref(), Some("ses_root"));
            event
        };
        assert_eq!(
            root(r#"{"type":"session.created","session":"ses_root","child":null}"#),
            HookEvent::SessionStarted
        );
        assert_eq!(
            root(r#"{"type":"chat.message","session":"ses_root","child":null}"#),
            HookEvent::PromptSubmitted
        );
        assert_eq!(
            root(r#"{"type":"session.idle","session":"ses_root","child":null}"#),
            HookEvent::TurnEnded
        );
        assert_eq!(
            root(r#"{"type":"permission.asked","session":"ses_root","child":null}"#),
            HookEvent::PermissionRequested {
                origin: Origin::Main,
                deferred: false
            }
        );
        assert_eq!(
            root(r#"{"type":"permission.replied","session":"ses_root","child":null}"#),
            HookEvent::ToolStarted {
                origin: Origin::Main
            }
        );
        assert_eq!(
            root(r#"{"type":"question.asked","session":"ses_root","child":null}"#),
            HookEvent::QuestionAsked {
                origin: Origin::Main,
                deferred: false,
            }
        );
        assert_eq!(
            root(r#"{"type":"question.rejected","session":"ses_root","child":null}"#),
            HookEvent::ToolFinished {
                origin: Origin::Main,
                question: true
            }
        );
        assert_eq!(
            root(r#"{"type":"tool.before","session":"ses_root","child":null,"tool":"bash"}"#),
            HookEvent::ToolStarted {
                origin: Origin::Main
            }
        );
        assert_eq!(
            event(r#"{"type":"tool.before","session":"ses_root","tool":"question"}"#),
            None
        );
    }

    #[test]
    fn a_child_session_is_a_subagent_of_its_root() {
        let child = |json: &str| {
            let (session, event) = event(json).expect("an event");
            assert_eq!(session.as_deref(), Some("ses_root"));
            event
        };
        assert_eq!(
            child(r#"{"type":"session.created","session":"ses_root","child":"ses_kid"}"#),
            HookEvent::SubagentStarted {
                id: "ses_kid".to_owned()
            }
        );
        assert_eq!(
            child(r#"{"type":"permission.asked","session":"ses_root","child":"ses_kid"}"#),
            HookEvent::PermissionRequested {
                origin: Origin::Subagent("ses_kid".to_owned()),
                deferred: false
            }
        );
        assert_eq!(
            child(r#"{"type":"session.idle","session":"ses_root","child":"ses_kid"}"#),
            HookEvent::SubagentStopped {
                id: "ses_kid".to_owned()
            }
        );
        // A subagent's own prompt is not the person's.
        assert_eq!(
            event(r#"{"type":"chat.message","session":"ses_root","child":"ses_kid"}"#),
            None
        );
    }
}
