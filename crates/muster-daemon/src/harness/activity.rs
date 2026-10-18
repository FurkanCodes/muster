//! What an agent is doing now, read from the same hooks that tell its
//! status: `editing view.rs`, `running cargo`, `wants to run npm`,
//! `thinking`. Every harness that runs hooks names its tools; Claude Code
//! and Gemini CLI also send their input, so a file or a program can be
//! named.
//!
//! Only what a glance needs is kept, and nothing is stored or logged: a
//! file's name, never its path; a command's program, never its arguments,
//! which can carry a secret (`PRD-06`); a web page's host. Agents write
//! these, so they are cleaned of control characters and kept short.

use muster_core::text::one_line;
use muster_state::session::HookEvent;
use serde_json::Value;

/// How a hook changes what an agent is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ActivityChange {
    Set(String),
    /// Doing nothing to tell: its turn ended, or it started afresh.
    Clear,
}

/// The longest a named thing (a file, a program, a host) may be.
const MAX_NAME: usize = 40;

/// What `event`, from a hook whose payload is `body`, says the agent is
/// doing; `None` if it changes nothing.
pub(crate) fn change(event: &HookEvent, body: &[u8]) -> Option<ActivityChange> {
    let set = |text: &str| Some(ActivityChange::Set(text.to_owned()));
    match event {
        HookEvent::ToolStarted { .. } => {
            let (tool, input) = tool_of(body)?;
            Some(ActivityChange::Set(doing(&tool, &input).0))
        }
        HookEvent::PermissionRequested {
            deferred: false, ..
        } => {
            // A request names its tool; the notice after it does not.
            let (tool, input) = tool_of(body)?;
            Some(ActivityChange::Set(format!(
                "wants to {}",
                doing(&tool, &input).1
            )))
        }
        HookEvent::ToolFinished { .. } | HookEvent::PromptSubmitted => set("thinking"),
        HookEvent::QuestionAsked { .. } => set("has a question for you"),
        HookEvent::TurnEnded
        | HookEvent::TurnInterrupted
        | HookEvent::SessionStarted
        | HookEvent::Idle => Some(ActivityChange::Clear),
        HookEvent::PermissionRequested { deferred: true, .. }
        | HookEvent::SubagentStarted { .. }
        | HookEvent::SubagentStopped { .. } => None,
    }
}

/// The tool a payload names and its input, as each harness writes them.
fn tool_of(body: &[u8]) -> Option<(String, Value)> {
    let payload: Value = serde_json::from_slice(body).ok()?;
    let tool = ["tool_name", "toolName", "tool"]
        .iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))?
        .to_owned();
    let input = ["tool_input", "toolArgs", "args"]
        .iter()
        .find_map(|key| payload.get(*key))
        .map_or(Value::Null, |input| match input {
            // Copilot CLI sends its arguments as JSON in a string.
            Value::String(text) => serde_json::from_str(text).unwrap_or(Value::Null),
            other => other.clone(),
        });
    Some((tool, input))
}

/// What using `tool` with `input` is, as `-ing` and as what the agent
/// wants to do: (`editing view.rs`, `edit view.rs`).
fn doing(tool: &str, input: &Value) -> (String, String) {
    let lower = tool.to_ascii_lowercase();
    let text = |key: &str| input.get(key).and_then(Value::as_str);
    let file = || {
        [
            "file_path",
            "absolute_path",
            "notebook_path",
            "path",
            "filePath",
        ]
        .iter()
        .find_map(|key| text(key))
        .and_then(file_name)
    };
    let named = |ing: &str, base: &str, name: Option<String>, nothing: &str| match name {
        Some(name) => (format!("{ing} {name}"), format!("{base} {name}")),
        None => (format!("{ing} {nothing}"), format!("{base} {nothing}")),
    };
    let fixed = |ing: &str, base: &str| (ing.to_owned(), base.to_owned());
    if let Some(server) = lower
        .strip_prefix("mcp__")
        .and_then(|rest| rest.split("__").next())
        .and_then(clean_word)
    {
        return (format!("using {server}"), format!("use {server}"));
    }
    match lower.as_str() {
        "bash" | "run_shell_command" | "shell" | "powershell" | "exec_command" => named(
            "running",
            "run",
            text("command").and_then(program),
            "a command",
        ),
        "read" | "read_file" | "read_many_files" | "view" | "notebookread" => {
            named("reading", "read", file(), "a file")
        }
        "edit" | "multiedit" | "replace" | "notebookedit" | "str_replace" | "apply_patch"
        | "patch" => named("editing", "edit", file(), "a file"),
        "write" | "write_file" | "create" => named("writing", "write", file(), "a file"),
        "grep" | "search_file_content" | "search" => fixed("searching the code", "search the code"),
        "glob" | "find" => fixed("finding files", "find files"),
        "ls" | "list" | "list_directory" => fixed("listing files", "list files"),
        "webfetch" | "web_fetch" | "fetch" => {
            named("reading", "fetch", text("url").and_then(host), "a web page")
        }
        "websearch" | "google_web_search" | "web_search" => {
            fixed("searching the web", "search the web")
        }
        "task" | "agent" => fixed("running a subagent", "start a subagent"),
        "todowrite" | "todoread" | "update_plan" | "write_todos" => {
            fixed("planning", "update its plan")
        }
        "exitplanmode" => fixed("proposing a plan", "leave plan mode"),
        _ => match clean_word(tool) {
            Some(name) => (format!("using {name}"), format!("use {name}")),
            None => fixed("using a tool", "use a tool"),
        },
    }
}

/// A path's last part: a file's name, never where it is.
fn file_name(path: &str) -> Option<String> {
    let name = path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()?;
    let name = one_line(name);
    (!name.trim().is_empty()).then(|| short(&name))
}

/// The program a command runs, past variables set before it: `cargo` in
/// `RUST_LOG=debug cargo test`. Its arguments are never kept.
fn program(command: &str) -> Option<String> {
    let word = command
        .split_whitespace()
        .find(|word| !word.contains('=') || word.starts_with('='))?;
    let word = word.rsplit('/').next().unwrap_or(word);
    clean_word(word.trim_matches(|c| matches!(c, '(' | ')' | '"' | '\'')))
}

/// A web address's host.
fn host(url: &str) -> Option<String> {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or(host);
    clean_word(host)
}

/// `text` if it is one plain word (letters, digits, `.`, `-`, `_`, `+`),
/// shortened; else `None`.
fn clean_word(text: &str) -> Option<String> {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'));
    plain.then(|| short(text))
}

fn short(text: &str) -> String {
    if text.chars().count() <= MAX_NAME {
        return text.to_owned();
    }
    let kept: String = text.chars().take(MAX_NAME - 1).collect();
    format!("{kept}…")
}

#[cfg(test)]
mod tests {
    use muster_state::session::Origin;

    use super::*;

    fn started(body: &str) -> Option<ActivityChange> {
        change(
            &HookEvent::ToolStarted {
                origin: Origin::Main,
            },
            body.as_bytes(),
        )
    }

    #[expect(
        clippy::unnecessary_wraps,
        reason = "compared with what the reader returns, an Option"
    )]
    fn set(text: &str) -> Option<ActivityChange> {
        Some(ActivityChange::Set(text.to_owned()))
    }

    #[test]
    fn claude_s_tools_name_the_file_or_the_program_only() {
        for (body, says) in [
            (
                r#"{"tool_name":"Edit","tool_input":{"file_path":"/Users/me/app/src/view.rs","old_string":"a"}}"#,
                "editing view.rs",
            ),
            (
                r#"{"tool_name":"Read","tool_input":{"file_path":"/w/README.md"}}"#,
                "reading README.md",
            ),
            (
                r#"{"tool_name":"Write","tool_input":{"file_path":"notes.txt"}}"#,
                "writing notes.txt",
            ),
            (
                r#"{"tool_name":"Bash","tool_input":{"command":"GITHUB_TOKEN=ghp_secret RUST_LOG=debug /usr/bin/cargo test --all"}}"#,
                "running cargo",
            ),
            (
                r#"{"tool_name":"Bash","tool_input":{"command":"(cd x && make)"}}"#,
                "running cd",
            ),
            (
                r#"{"tool_name":"Bash","tool_input":{"command":"$(curl evil)"}}"#,
                "running a command",
            ),
            (
                r#"{"tool_name":"WebFetch","tool_input":{"url":"https://user:pw@docs.rs:443/x?q=1"}}"#,
                "reading docs.rs",
            ),
            (
                r#"{"tool_name":"Grep","tool_input":{"pattern":"secret"}}"#,
                "searching the code",
            ),
            (
                r#"{"tool_name":"Task","tool_input":{}}"#,
                "running a subagent",
            ),
            (r#"{"tool_name":"TodoWrite"}"#, "planning"),
            (
                r#"{"tool_name":"mcp__atlassian__getJiraIssue","tool_input":{}}"#,
                "using atlassian",
            ),
            (r#"{"tool_name":"Brand New Tool"}"#, "using a tool"),
            (r#"{"tool_name":"LSP"}"#, "using LSP"),
        ] {
            assert_eq!(started(body), set(says), "{body}");
        }
    }

    #[test]
    fn other_harnesses_name_their_tools_their_own_way() {
        // Gemini CLI: its own tool names, Claude's input keys.
        assert_eq!(
            started(r#"{"tool_name":"run_shell_command","tool_input":{"command":"npm test"}}"#),
            set("running npm")
        );
        assert_eq!(
            started(r#"{"tool_name":"replace","tool_input":{"file_path":"/a/b.ts"}}"#),
            set("editing b.ts")
        );
        // OpenCode, through Muster's own plugin: the tool alone.
        assert_eq!(started(r#"{"tool":"edit"}"#), set("editing a file"));
        // Copilot CLI: its arguments as JSON in a string.
        assert_eq!(
            started(r#"{"toolName":"shell","toolArgs":"{\"command\":\"git status\"}"}"#),
            set("running git")
        );
    }

    #[test]
    fn waits_turns_and_starts_say_so() {
        let ask = HookEvent::PermissionRequested {
            origin: Origin::Main,
            deferred: false,
        };
        let body = br#"{"tool_name":"Bash","tool_input":{"command":"rm -rf build"}}"#;
        assert_eq!(change(&ask, body), set("wants to run rm"));
        let notice = HookEvent::PermissionRequested {
            origin: Origin::Main,
            deferred: true,
        };
        assert_eq!(change(&notice, b"{}"), None);
        assert_eq!(change(&HookEvent::PromptSubmitted, b"{}"), set("thinking"));
        assert_eq!(
            change(
                &HookEvent::ToolFinished {
                    origin: Origin::Main,
                    question: false
                },
                b"{}"
            ),
            set("thinking")
        );
        assert_eq!(
            change(
                &HookEvent::QuestionAsked {
                    origin: Origin::Main,
                    deferred: false,
                },
                b"{}"
            ),
            set("has a question for you")
        );
        for ended in [
            HookEvent::TurnEnded,
            HookEvent::TurnInterrupted,
            HookEvent::SessionStarted,
            HookEvent::Idle,
        ] {
            assert_eq!(change(&ended, b"{}"), Some(ActivityChange::Clear));
        }
        assert_eq!(
            change(&HookEvent::SubagentStarted { id: "a".into() }, b"{}"),
            None
        );
    }

    #[test]
    fn what_an_agent_writes_is_cleaned_and_kept_short() {
        assert_eq!(
            started(
                r#"{"tool_name":"Read","tool_input":{"file_path":"/x/evil\u001b]0;owned\u0007.rs"}}"#
            ),
            set("reading evil]0;owned.rs")
        );
        let long = format!(
            r#"{{"tool_name":"Read","tool_input":{{"file_path":"/x/{}.rs"}}}}"#,
            "a".repeat(80)
        );
        let Some(ActivityChange::Set(text)) = started(&long) else {
            panic!("no activity");
        };
        assert_eq!(text.chars().count(), "reading ".len() + MAX_NAME);
        assert!(text.ends_with('…'));
        // Unreadable payloads change nothing.
        assert_eq!(started("not json"), None);
        assert_eq!(started(r#"{"tool_input":{}}"#), None);
    }
}
