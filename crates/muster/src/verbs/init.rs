//! `muster init zsh|bash|fish`: functions the person adds to their shell,
//! with `eval "$(muster init zsh)"` in its rc file, so a coding agent typed
//! by its own name in a workspace's terminal (`claude "fix it"`) starts as
//! a session of that workspace, shown in its place, as `muster claude`
//! would; anywhere else, and with a flag first (`claude --resume`) or
//! input from a pipe, the agent's own program runs as it is. Muster writes
//! no file of the person's (`PRD-13`): it prints the functions.

use std::process::ExitCode;
use std::time::Duration;

use clap::ValueEnum;
use muster_core::Paths;

use crate::commands;

/// How long `init` waits for the daemon to name the agents the person
/// added, as a shell starts: Muster's own are named without it.
const ASKING: Duration = Duration::from_millis(300);

/// The coding agents of Muster's own, by the names their programs go by.
const OWN: [&str; 5] = ["claude", "codex", "gemini", "opencode", "copilot"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Shell {
    Zsh,
    Bash,
    Fish,
}

pub(crate) async fn run(paths: &Paths, shell: Shell) -> ExitCode {
    let mut names: Vec<String> = OWN.iter().map(|name| (*name).to_owned()).collect();
    names.extend(added(paths).await);
    print!("{}", functions(shell, &names));
    ExitCode::SUCCESS
}

/// The agents the person added, where the daemon runs and says in time;
/// none otherwise.
async fn added(paths: &Paths) -> Vec<String> {
    let asked = async {
        let mut connection = commands::connect(&paths.socket()).await.ok()?;
        connection.subscribe().await.ok()
    };
    match tokio::time::timeout(ASKING, asked).await {
        Ok(Some(snapshot)) => snapshot
            .preferences
            .agents
            .into_iter()
            .map(|agent| agent.name)
            .filter(|name| usable(name) && !OWN.contains(&name.as_str()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether `name` can be a function's name in every shell, as an agent's
/// usually is: letters, digits, `_` and `-`, not first.
fn usable(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The functions for `shell`, one for each agent in `names`.
fn functions(shell: Shell, names: &[String]) -> String {
    let mut out = String::from(
        "# Muster (`muster init`): in a workspace's terminal, an agent typed by its name\n\
         # starts as a session of the workspace. `command <agent>` runs the program itself.\n",
    );
    for name in names.iter().filter(|name| usable(name)) {
        let function = match shell {
            Shell::Zsh | Shell::Bash => format!(
                "{name}() {{\n  case \"${{1-}}\" in\n    -*) command {name} \"$@\" ;;\n    *) if [ -n \"${{MUSTER_TERMINAL-}}\" ] && [ -t 0 ]; then\n         command muster {name} \"$@\"\n       else\n         command {name} \"$@\"\n       fi ;;\n  esac\n}}\n"
            ),
            Shell::Fish => format!(
                "function {name}\n  if set -q MUSTER_TERMINAL; and isatty stdin; and not string match -q -- '-*' \"$argv[1]\"\n    command muster {name} $argv\n  else\n    command {name} $argv\n  end\nend\n"
            ),
        };
        out.push_str(&function);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_agent_is_a_function_that_asks_muster_only_in_its_terminals_prd_13() {
        let names = vec!["claude".to_owned(), "my agent".to_owned()];
        let zsh = functions(Shell::Zsh, &names);
        assert!(zsh.contains("claude() {"));
        assert!(zsh.contains("command muster claude \"$@\""));
        assert!(zsh.contains("command claude \"$@\""));
        assert!(zsh.contains("${MUSTER_TERMINAL-}"));
        assert!(!zsh.contains("my agent"), "no name a shell cannot take");
        let fish = functions(Shell::Fish, &names);
        assert!(fish.contains("function claude"));
        assert!(fish.contains("command muster claude $argv"));
    }
}
