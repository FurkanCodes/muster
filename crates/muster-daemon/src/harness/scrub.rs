//! The daemon's variables a session must not inherit.
//!
//! An agent sets variables for what it runs: they say a program runs inside
//! that agent's session, reach for its sockets, or carry its account and
//! effort. A daemon started from inside one would pass them on to every
//! session, and an agent inheriting another's markers misbehaves (Claude Code
//! inheriting `CLAUDE_CODE_CHILD_SESSION` stops saving its transcript, so it
//! could not be resumed). The person's own settings stay, such as
//! `CLAUDE_CODE_USE_BEDROCK`, `CODEX_HOME` or `GEMINI_API_KEY`: a session runs
//! as if started in their terminal (`PRD-06`).

/// Set by Claude Code, or the app hosting it, for what it runs. Seen in
/// Claude Code 2.1.296 and its desktop app.
const CLAUDE: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_PID",
    "CLAUDE_EFFORT",
    "CLAUDE_PREVIEW_CLASSIFIER_FLOOR",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_HOST_SESSION_ID",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_CODE_EXECPATH",
    "CLAUDE_CODE_DESKTOP_APP_VERSION",
    "CLAUDE_CODE_ORGANIZATION_UUID",
    "CLAUDE_CODE_ACCOUNT_UUID",
    "CLAUDE_CODE_USER_EMAIL",
    "CLAUDE_CODE_OAUTH_SCOPES",
    "CLAUDE_CODE_SIMPLE",
    "CLAUDE_CODE_DISABLE_CRON",
    "CLAUDE_CODE_TERMINAL_MCP_TOOLS",
    "CLAUDE_CODE_EAGER_FLUSH",
    "CLAUDE_CODE_REPORT_FINDINGS",
    "CLAUDE_CODE_EMIT_TOOL_USE_SUMMARIES",
    "CLAUDE_CODE_ENABLE_ASK_USER_QUESTION_TOOL",
    "CLAUDE_CODE_ENABLE_SDK_FILE_CHECKPOINTING",
    // Set for plugins' hooks, by Claude Code and by others that follow it.
    "CLAUDE_PROJECT_DIR",
    "CLAUDE_PLUGIN_ROOT",
    "CLAUDE_PLUGIN_DATA",
];

/// Set by Codex 0.162 for what it runs, sandboxed or not.
const CODEX: &[&str] = &[
    "CODEX_THREAD_ID",
    "CODEX_SESSION_ID",
    "CODEX_TOOL_CALL_ID",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "CODEX_CI",
    "CODEX_EXEC_SERVER_URL",
];

/// Set by Gemini CLI 0.63 for its tools and hooks.
const GEMINI: &[&str] = &[
    "GEMINI_CLI",
    "GEMINI_CLI_NO_RELAUNCH",
    "GEMINI_CLI_TRUST_WORKSPACE",
    "GEMINI_SESSION_ID",
    "GEMINI_PROJECT_DIR",
    "GEMINI_CWD",
    "GEMINI_PLANS_DIR",
];

/// Set by OpenCode 1.4 for what it runs.
const OPENCODE: &[&str] = &[
    "OPENCODE",
    "OPENCODE_PID",
    "OPENCODE_TERMINAL",
    "OPENCODE_CLIENT",
];

/// Set by Copilot CLI 1.0 for what it runs and for its plugins' hooks.
const COPILOT: &[&str] = &[
    "COPILOT_CLI",
    "COPILOT_AGENT_SESSION_ID",
    "COPILOT_PLUGIN_ROOT",
    "PLUGIN_ROOT",
];

/// Prefixes of the same, for families of them.
const PREFIXES: &[&str] = &[
    "CLAUDE_AGENT_SDK_",
    "CLAUDE_CODE_SDK_",
    "CODEX_NETWORK_PROXY_",
];

/// Those naming the terminal tab the daemon was started in.
const TERMINAL: &[&str] = &["TERM_SESSION_ID", "ITERM_SESSION_ID"];

/// Which of `keys` a session must not inherit.
pub(crate) fn scrubbed(keys: impl Iterator<Item = String>) -> Vec<String> {
    keys.filter(|key| {
        [CLAUDE, CODEX, GEMINI, OPENCODE, COPILOT, TERMINAL]
            .iter()
            .any(|names| names.contains(&key.as_str()))
            || PREFIXES.iter().any(|prefix| key.starts_with(prefix))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_and_terminal_markers_are_scrubbed_and_settings_kept() {
        let keys = [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CLAUDE_CODE_CHILD_SESSION",
            "CLAUDE_PID",
            "CLAUDE_EFFORT",
            "CLAUDE_AGENT_SDK_VERSION",
            "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
            "TERM_SESSION_ID",
            "CODEX_THREAD_ID",
            "CODEX_SANDBOX",
            "CODEX_NETWORK_PROXY_ACTIVE",
            "GEMINI_CLI",
            "GEMINI_SESSION_ID",
            "OPENCODE",
            "OPENCODE_PID",
            "COPILOT_CLI",
            "COPILOT_AGENT_SESSION_ID",
            // The person's own settings.
            "PATH",
            "HOME",
            "CLAUDE_CONFIG_DIR",
            "CLAUDE_CODE_USE_BEDROCK",
            "CLAUDE_CODE_USE_VERTEX",
            "CLAUDE_CODE_OAUTH_TOKEN",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "ANTHROPIC_API_KEY",
            "CODEX_HOME",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "OPENCODE_CONFIG",
            "COPILOT_HOME",
            "GH_TOKEN",
        ]
        .map(str::to_owned);
        assert_eq!(
            scrubbed(keys.into_iter()),
            [
                "CLAUDECODE",
                "CLAUDE_CODE_ENTRYPOINT",
                "CLAUDE_CODE_CHILD_SESSION",
                "CLAUDE_PID",
                "CLAUDE_EFFORT",
                "CLAUDE_AGENT_SDK_VERSION",
                "CLAUDE_CODE_SDK_HAS_HOST_AUTH_REFRESH",
                "TERM_SESSION_ID",
                "CODEX_THREAD_ID",
                "CODEX_SANDBOX",
                "CODEX_NETWORK_PROXY_ACTIVE",
                "GEMINI_CLI",
                "GEMINI_SESSION_ID",
                "OPENCODE",
                "OPENCODE_PID",
                "COPILOT_CLI",
                "COPILOT_AGENT_SESSION_ID",
            ]
        );
    }
}
