//! `muster init zsh|bash|fish`: functions the person adds to their shell,
//! with `eval "$(muster init zsh)"` in its rc file, so a coding agent typed
//! by its own name in a terminal of Muster's (`claude "fix it"`) runs in
//! that terminal with Muster's hooks (`muster exec`), the terminal its chat
//! until it ends (`ARC-28`); anywhere else, and with a flag first (`claude
//! --resume`) or input from a pipe, the agent's own program runs as it is.
//! Muster writes no file of the person's (`PRD-13`): it prints the
//! functions.

use std::process::ExitCode;
use std::time::Duration;

use clap::ValueEnum;
use muster_core::Paths;
use muster_core::harness::OwnAgent;

use crate::commands;

/// How long `init` waits for the daemon to name the agents the person
/// added, as a shell starts: Muster's own are named without it.
const ASKING: Duration = Duration::from_millis(300);

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum Shell {
    Zsh,
    Bash,
    Fish,
}

pub(crate) async fn run(paths: &Paths, shell: Shell) -> ExitCode {
    let mut names: Vec<String> = OwnAgent::ALL.map(|agent| agent.program().to_owned()).into();
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
            .filter(|name| usable(name) && OwnAgent::of(name).is_none())
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
        "# Muster (`muster init`): in a terminal of Muster's, an agent typed by its name\n\
         # runs there with Muster's hooks. `command <agent>` runs the program itself.\n",
    );
    // zsh's `correct_all` (oh-my-zsh's `ENABLE_CORRECTION`) would offer to
    // turn `muster ssh` into `muster .ssh` in a home with `~/.ssh`.
    if shell == Shell::Zsh {
        out.push_str("alias muster='nocorrect muster'\n");
    }
    for name in names.iter().filter(|name| usable(name)) {
        let function = match shell {
            // An alias of the name, as some installers write, would be read
            // in place of the name in `name() {`, and would still shadow the
            // function; so it goes first, and `function` names it.
            Shell::Zsh | Shell::Bash => format!(
                "unalias {name} 2>/dev/null || true\nfunction {name} {{\n  case \"${{1-}}\" in\n    -*) command {name} \"$@\" ;;\n    *) if [ -n \"${{MUSTER_TERMINAL-}}\" ] && [ -t 0 ]; then\n         command muster exec {name} \"$@\"\n       else\n         command {name} \"$@\"\n       fi ;;\n  esac\n}}\n"
            ),
            Shell::Fish => format!(
                "function {name}\n  if set -q MUSTER_TERMINAL; and isatty stdin; and not string match -q -- '-*' \"$argv[1]\"\n    command muster exec {name} $argv\n  else\n    command {name} $argv\n  end\nend\n"
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
        assert!(zsh.contains("function claude {"));
        assert!(zsh.contains("command muster exec claude \"$@\""));
        assert!(zsh.contains("command claude \"$@\""));
        assert!(zsh.contains("${MUSTER_TERMINAL-}"));
        assert!(!zsh.contains("my agent"), "no name a shell cannot take");
        let fish = functions(Shell::Fish, &names);
        assert!(fish.contains("function claude"));
        assert!(fish.contains("command muster exec claude $argv"));
    }

    /// The functions read in each shell there is, an alias of an agent's
    /// name already set, and the name is the function then.
    #[test]
    fn the_functions_read_in_each_shell_over_an_alias_of_the_name() {
        let names = vec!["claude".to_owned(), "my-agent".to_owned()];
        // Bash expands aliases outside an interactive shell only if asked.
        let shells = [
            (Shell::Bash, "/bin/bash", "shopt -s expand_aliases\n"),
            (Shell::Zsh, "/bin/zsh", ""),
        ];
        for (shell, program, first) in shells {
            if !std::path::Path::new(program).exists() {
                continue;
            }
            let script = format!(
                "{first}alias claude='echo aliased'\n{}\ntype claude; type my-agent",
                functions(shell, &names)
            );
            let output = std::process::Command::new(program)
                .args(["-c", &script])
                .env("BASH_ENV", "/dev/null")
                .output()
                .unwrap();
            let said = String::from_utf8_lossy(&output.stdout);
            let errors = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{program}: {errors}");
            assert!(errors.is_empty(), "{program}: {errors}");
            assert_eq!(said.matches("function").count(), 2, "{program}: {said}");
            assert!(!said.contains("alias"), "{program}: {said}");
        }
    }

    /// zsh with `correct_all` never offers to correct a word after `muster`
    /// (`muster ssh` beside `~/.ssh`).
    #[test]
    fn zsh_never_corrects_muster_s_words() {
        if !std::path::Path::new("/bin/zsh").exists() {
            return;
        }
        let script = format!(
            "setopt correct_all\n{}\nwhence -v muster",
            functions(Shell::Zsh, &[])
        );
        let output = std::process::Command::new("/bin/zsh")
            .args(["-c", &script])
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&output.stdout);
        assert!(said.contains("nocorrect muster"), "{said}");
        assert!(!functions(Shell::Bash, &[]).contains("nocorrect"));
    }
}
