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
//!
//! The terminal the daemon was started in sets variables too, naming itself
//! or addressing its window, tab, remote-control socket or shell
//! integration. A session's terminal is Muster's, not that one: a program
//! told otherwise would send it sequences of its own, or reach past Muster
//! into the person's terminal (`kitty @`, `wezterm cli`, `tmux` would open
//! windows there). Muster names itself instead (`TERM_PROGRAM`, set with
//! the terminal type). Kept on purpose: `COLORTERM`, which says what colours
//! Muster draws too; `TERMINFO`, which may be the person's own;
//! `ITERM_SHELL_INTEGRATION_INSTALLED`, which keeps iTerm2's shell
//! integration from loading inside Muster; and VS Code's git sign-in
//! prompt (`GIT_ASKPASS` with its `VSCODE_GIT_*`), which git needs whole or
//! not at all and which is the person's git, not their terminal.

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

/// Set by `OpenCode` 1.4 for what it runs.
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

/// Set by Muster for each session it starts, which sets its own again where
/// a session has hooks. A daemon started from inside a Muster session would
/// otherwise hand that session's hook token to every session it starts
/// without hooks, a shell or an agent the person added, which could then
/// report as it (`ARC-11`). A plugin's panel is told where it is
/// (`MUSTER_PLUGIN_*`), and only it.
const MUSTER: &[&str] = &["MUSTER_SESSION", "MUSTER_HOOK_SOCKET", "MUSTER_HOOK_TOKEN"];

/// Prefixes of the same, for families of them.
const PREFIXES: &[&str] = &[
    "CLAUDE_AGENT_SDK_",
    "CLAUDE_CODE_SDK_",
    "CODEX_NETWORK_PROXY_",
    "MUSTER_PLUGIN_",
];

/// Set by the terminal the daemon was started in, or a multiplexer in it:
/// they name it, or address its window, tab, socket or shell integration.
const TERMINAL: &[&str] = &[
    // macOS's Terminal and iTerm2.
    "TERM_SESSION_ID",
    "ITERM_SESSION_ID",
    "LC_TERMINAL",
    "LC_TERMINAL_VERSION",
    // kitty: its window, process, remote control and shell integration.
    "KITTY_WINDOW_ID",
    "KITTY_PID",
    "KITTY_LISTEN_ON",
    "KITTY_PUBLIC_KEY",
    "KITTY_INSTALLATION_DIR",
    "KITTY_SHELL_INTEGRATION",
    "WEZTERM_PANE",
    "WEZTERM_EXECUTABLE",
    "WEZTERM_EXECUTABLE_DIR",
    "WEZTERM_UNIX_SOCKET",
    "ALACRITTY_WINDOW_ID",
    "ALACRITTY_SOCKET",
    "ALACRITTY_LOG",
    "GHOSTTY_BIN_DIR",
    "GHOSTTY_RESOURCES_DIR",
    "GHOSTTY_SHELL_FEATURES",
    // GNOME Terminal and the others built on VTE.
    "VTE_VERSION",
    // Windows Terminal, for Linux under WSL.
    "WT_SESSION",
    "WT_PROFILE_ID",
    // JetBrains' terminal.
    "TERMINAL_EMULATOR",
    "KONSOLE_DBUS_SESSION",
    "KONSOLE_DBUS_SERVICE",
    "KONSOLE_DBUS_WINDOW",
    "KONSOLE_VERSION",
    // VS Code's terminal: its shell integration, its window's socket.
    "VSCODE_INJECTION",
    "VSCODE_NONCE",
    "VSCODE_SHELL_INTEGRATION",
    "VSCODE_SHELL_ENV_REPORTING",
    "VSCODE_STABLE",
    "VSCODE_ENV_REPLACE",
    "VSCODE_ENV_PREPEND",
    "VSCODE_ENV_APPEND",
    "VSCODE_IPC_HOOK_CLI",
    "VSCODE_PID",
    // Multiplexers: their server and pane.
    "TMUX",
    "TMUX_PANE",
    "STY",
    "ZELLIJ",
    "ZELLIJ_SESSION_NAME",
    "ZELLIJ_PANE_ID",
];

/// Which of `keys` a session must not inherit.
pub(crate) fn scrubbed(keys: impl Iterator<Item = String>) -> Vec<String> {
    keys.filter(|key| is_scrubbed(key)).collect()
}

fn is_scrubbed(key: &str) -> bool {
    [CLAUDE, CODEX, GEMINI, OPENCODE, COPILOT, TERMINAL, MUSTER]
        .iter()
        .any(|names| names.contains(&key))
        || PREFIXES.iter().any(|prefix| key.starts_with(prefix))
}

/// The variables of the daemon's environment every plugin's program gets,
/// as sessions do: where programs are, whose they are, how text reads, and
/// the person's own description of terminals.
fn is_basic(key: &str) -> bool {
    matches!(
        key,
        "PATH" | "HOME" | "USER" | "LOGNAME" | "SHELL" | "TMPDIR" | "TZ" | "LANG" | "TERMINFO"
    ) || key.starts_with("LC_")
}

/// Which of `keys` a plugin's program must not inherit (`ARC-23`): all
/// but the basics and those its manifest names (`seen`), and never one a
/// session must not inherit either.
pub(crate) fn kept_from_plugin(keys: impl Iterator<Item = String>, seen: &[String]) -> Vec<String> {
    keys.filter(|key| is_scrubbed(key) || !(is_basic(key) || seen.contains(key)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_s_program_gets_the_basics_and_what_it_names_arc_23() {
        let keys = [
            "PATH",
            "HOME",
            "LANG",
            "LC_ALL",
            "LC_TERMINAL",
            "GITHUB_TOKEN",
            "ZOE_THEME",
            "CLAUDE_CODE_ENTRYPOINT",
            "MUSTER_HOOK_TOKEN",
        ]
        .map(str::to_owned);
        let seen = ["ZOE_THEME".to_owned(), "CLAUDE_CODE_ENTRYPOINT".to_owned()];
        assert_eq!(
            kept_from_plugin(keys.into_iter(), &seen),
            [
                // An iTerm2 marker, though it starts as the basics do.
                "LC_TERMINAL",
                "GITHUB_TOKEN",
                // Named, but no session inherits it.
                "CLAUDE_CODE_ENTRYPOINT",
                "MUSTER_HOOK_TOKEN",
            ]
        );
    }

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
            "MUSTER_HOOK_TOKEN",
            "MUSTER_HOOK_SOCKET",
            "MUSTER_SESSION",
            "MUSTER_PLUGIN_BRANCH",
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
                "MUSTER_HOOK_TOKEN",
                "MUSTER_HOOK_SOCKET",
                "MUSTER_SESSION",
                "MUSTER_PLUGIN_BRANCH",
            ]
        );
    }

    #[test]
    fn the_outer_terminal_s_names_and_addresses_are_scrubbed_and_colours_kept() {
        let outer = [
            "TERM_SESSION_ID",
            "ITERM_SESSION_ID",
            "LC_TERMINAL",
            "LC_TERMINAL_VERSION",
            "KITTY_WINDOW_ID",
            "KITTY_LISTEN_ON",
            "KITTY_PUBLIC_KEY",
            "WEZTERM_PANE",
            "WEZTERM_UNIX_SOCKET",
            "ALACRITTY_SOCKET",
            "GHOSTTY_RESOURCES_DIR",
            "VTE_VERSION",
            "WT_SESSION",
            "TERMINAL_EMULATOR",
            "KONSOLE_DBUS_SESSION",
            "VSCODE_NONCE",
            "VSCODE_IPC_HOOK_CLI",
            "TMUX",
            "TMUX_PANE",
            "ZELLIJ",
        ];
        let kept = [
            "TERM",
            "COLORTERM",
            "TERMINFO",
            "LC_ALL",
            "LC_CTYPE",
            "ITERM_SHELL_INTEGRATION_INSTALLED",
            "GIT_ASKPASS",
            "VSCODE_GIT_IPC_HANDLE",
            "WEZTERM_CONFIG_FILE",
            "TMUX_TMPDIR",
        ];
        let keys = outer.iter().chain(&kept).map(|key| (*key).to_owned());
        assert_eq!(scrubbed(keys), outer);
    }
}
