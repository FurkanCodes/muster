//! What a machine has for agents to work there, asked by a script of
//! Muster's own and said to the person with what to do about what is
//! missing (`muster ssh vm --check`). Muster never copies a sign-in there
//! (`PRD-06`): the person signs in on the machine itself.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Asks a machine what agents work with there: each agent's program, git
/// and gh; whether Claude Code is signed in (its file on Linux, its
/// Keychain item on a Mac) and gh; and whether the person's processes
/// outlive their logging out (systemd's lingering). Each answer on a
/// marked line, so a shell's greeting is no answer. Run by `sh`.
pub(crate) fn script() -> String {
    format!(
        "{path}; echo \"muster-check:system=$(uname -s)\"; \
for a in claude codex gemini opencode copilot git gh curl; do echo \"muster-check:$a=$(command -v $a 2>/dev/null)\"; done; \
if [ -f \"$HOME/.claude/.credentials.json\" ]; then echo muster-check:claude-signed-in=yes; \
elif command -v security >/dev/null 2>&1; then \
  if security find-generic-password -s 'Claude Code-credentials' >/dev/null 2>&1; then echo muster-check:claude-signed-in=yes; \
  else echo muster-check:claude-signed-in=unknown; fi; \
else echo muster-check:claude-signed-in=no; fi; \
gh auth status >/dev/null 2>&1 && echo muster-check:gh-signed-in=yes; \
command -v loginctl >/dev/null 2>&1 && echo \"muster-check:linger=$(loginctl show-user \"$(id -un)\" -p Linger --value 2>/dev/null)\"; \
true\n",
        path = super::args::REMOTE_PATH
    )
}

/// The check's marked lines, by key.
fn found(output: &str) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    for line in output.lines() {
        if let Some((key, value)) = line
            .trim()
            .strip_prefix("muster-check:")
            .and_then(|rest| rest.split_once('='))
        {
            found.insert(key.to_owned(), value.trim().to_owned());
        }
    }
    found
}

/// Whether the check found `program` on the machine.
pub(crate) fn has(output: &str, program: &str) -> bool {
    found(output)
        .get(program)
        .is_some_and(|path| !path.is_empty())
}

/// Whether Claude Code is not on the machine, as the check says.
pub(crate) fn claude_missing(output: &str) -> bool {
    !has(output, "claude")
}

/// What the machine has for agents, from the check's marked lines, and
/// what to do about what is missing.
pub(crate) fn report(machine: &str, system: &str, version: &str, output: &str) -> String {
    let found = found(output);
    let has = |key: &str| found.get(key).is_some_and(|value| !value.is_empty());
    let said = |key: &str| found.get(key).map(String::as_str);
    let mac = said("system") == Some("Darwin");
    let mut out = format!("On {machine} ({system}), muster {version}:\n");
    if has("claude") {
        match said("claude-signed-in") {
            Some("yes") => out.push_str("  ✓ Claude Code, signed in\n"),
            Some("unknown") => out.push_str(
                "  ? Claude Code is there; whether it is signed in cannot be told from here: \
                 run `claude` once there\n",
            ),
            _ => {
                let _ = writeln!(
                    out,
                    "  ! Claude Code is there but not signed in: sign in there once with `muster \
                     ssh {machine} claude auth login` (Muster never copies your sign-in)"
                );
            }
        }
        if mac {
            out.push_str(
                "  · on a Mac reached over ssh, Claude Code's sign-in is in the Keychain: if \
                 agents there say they are signed out, run `security unlock-keychain` in an ssh \
                 session there\n",
            );
        }
    } else {
        let _ = writeln!(
            out,
            "  ✗ Claude Code is not there: `muster ssh {machine} --install-claude` installs it \
             with its own installer"
        );
    }
    let others: Vec<&str> = ["codex", "gemini", "opencode", "copilot"]
        .into_iter()
        .filter(|agent| has(agent))
        .collect();
    if !others.is_empty() {
        let _ = writeln!(out, "  ✓ also {}", others.join(", "));
    }
    out.push_str(if has("git") {
        "  ✓ git\n"
    } else {
        "  ✗ git is not there: worktrees need it\n"
    });
    match (has("gh"), said("gh-signed-in")) {
        (true, Some("yes")) => out.push_str("  ✓ gh, signed in\n"),
        (true, _) => out.push_str(
            "  ! gh is not signed in: `gh auth login` there, for pull requests and tickets\n",
        ),
        (false, _) => out.push_str("  · no gh: pull requests and GitHub tickets need it\n"),
    }
    if said("linger") == Some("no") {
        out.push_str(
            "  ! systemd ends your processes when you log out: `sudo loginctl enable-linger \
             $USER` there keeps the agents working\n",
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_says_what_agents_need_on_linux_arc_27() {
        let said = report(
            "vm",
            "Linux x86_64",
            "0.5.6",
            "muster-check:system=Linux\nmuster-check:claude=/usr/bin/claude\n\
             muster-check:claude-signed-in=no\nmuster-check:codex=\nmuster-check:git=/usr/bin/git\n\
             muster-check:gh=/usr/bin/gh\nmuster-check:linger=no\n",
        );
        assert!(said.contains("`muster ssh vm claude auth login`"), "{said}");
        assert!(said.contains("✓ git"), "{said}");
        assert!(said.contains("gh auth login"), "{said}");
        assert!(said.contains("enable-linger"), "{said}");
        assert!(!said.contains("codex"), "{said}");
        assert!(!said.contains("Keychain"), "{said}");
    }

    #[test]
    fn on_a_mac_the_sign_in_is_the_keychain_s() {
        let signed_in = report(
            "desk",
            "Darwin arm64",
            "0.5.6",
            "muster-check:system=Darwin\nmuster-check:claude=/opt/homebrew/bin/claude\n\
             muster-check:claude-signed-in=yes\n",
        );
        assert!(
            signed_in.contains("✓ Claude Code, signed in"),
            "{signed_in}"
        );
        assert!(signed_in.contains("unlock-keychain"), "{signed_in}");
        let unknown = report(
            "desk",
            "Darwin arm64",
            "0.5.6",
            "muster-check:system=Darwin\nmuster-check:claude=/x/claude\n\
             muster-check:claude-signed-in=unknown\n",
        );
        assert!(unknown.contains("cannot be told"), "{unknown}");
    }

    #[test]
    fn the_script_answers_on_marked_lines_whatever_the_shell_prints() {
        let script = script();
        assert!(script.contains("/opt/homebrew/bin"));
        assert!(script.contains("Claude Code-credentials"));
        assert!(script.trim_end().ends_with("true"));
    }
}
