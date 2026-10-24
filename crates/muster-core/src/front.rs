//! What runs in front in a terminal (`PRODUCT.md` §6, `ARC-28`): from the
//! foreground program's name and arguments, the name the terminal is
//! called by (`zsh`, `cargo`, `aider`: a script by its own name, not its
//! interpreter's) and the coding agent it is, if it is one: one of
//! Muster's own, or one the person added, by its program's name. Whatever
//! starts it is looked through: a runtime (`node cli.js`, `python -m`),
//! or a package runner (`npx @anthropic-ai/claude-code`). Pure: the
//! daemon reads the processes (`ARC-01`).

/// The foreground process of a terminal, as the daemon read it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProcessFacts {
    /// Its name as the system has it (`node`, `2.1.300`, `-zsh`).
    pub name: String,
    /// Its arguments, its own name first, as it was started.
    pub argv: Vec<String>,
}

/// What is in front in a terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Front {
    /// The name the terminal is called by while it runs.
    pub program: String,
    /// The coding agent it is, by its harness's id (`claude`, or the name
    /// of an agent the person added); `None` for any other program.
    pub agent: Option<String>,
}

/// Muster's own agents: each harness's id, and the names its program goes
/// by, after looking through whatever starts it.
const OWN: [(&str, &[&str]); 5] = [
    ("claude", &["claude", "claude-code"]),
    ("codex", &["codex"]),
    ("gemini", &["gemini"]),
    ("opencode", &["opencode"]),
    ("copilot", &["copilot"]),
];

/// Packages that are Muster's own agents, as a package runner names them
/// or a runtime's script lies in them.
const PACKAGES: [(&str, &str); 5] = [
    ("@anthropic-ai/claude-code", "claude"),
    ("@openai/codex", "codex"),
    ("@google/gemini-cli", "gemini"),
    ("opencode-ai", "opencode"),
    ("@github/copilot", "copilot"),
];

/// Programs that run a script named after the script: runtimes, and
/// shells running a script file (an agent can be one).
const RUNTIMES: [&str; 10] = [
    "node", "bun", "deno", "python", "ruby", "tsx", "sh", "bash", "zsh", "dash",
];

/// Programs that fetch a package and run it, named after them.
const RUNNERS: [&str; 4] = ["npx", "bunx", "pnpx", "uvx"];

/// What `facts` is: its name, and the agent it is among Muster's own and
/// the person's `added` (each `(name, program)`).
pub fn identify<'a>(
    facts: &ProcessFacts,
    added: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Front {
    let (program, package) = looked_through(facts);
    let agent = package
        .and_then(|package| {
            PACKAGES
                .iter()
                .find(|(named, _)| *named == package)
                .map(|(_, agent)| (*agent).to_owned())
        })
        .or_else(|| {
            OWN.iter()
                .find(|(_, names)| names.contains(&program.as_str()))
                .map(|(agent, _)| (*agent).to_owned())
        })
        .or_else(|| {
            added
                .into_iter()
                .find(|(_, theirs)| basename(theirs) == program)
                .map(|(name, _)| name.to_owned())
        });
    Front { program, agent }
}

/// The name of what runs, after looking through runtimes and package
/// runners, and the package it came from where one says.
fn looked_through(facts: &ProcessFacts) -> (String, Option<String>) {
    // The name it was started by before the system's: Claude Code's own
    // installer runs a file named for its version, started as `claude`.
    let first = facts
        .argv
        .first()
        .map_or(facts.name.as_str(), String::as_str);
    let mut program = command_name(first);
    if program.is_empty() {
        program = command_name(&facts.name);
    }
    let mut rest = facts.argv.get(1..).unwrap_or_default();
    if is_runtime(&program) {
        match script(rest) {
            Some((Script::Module(module), _)) => return (module.to_owned(), None),
            Some((Script::File(path), after)) => {
                if let Some(package) = package_of(path) {
                    return (named(&package), Some(package));
                }
                program = command_name(path);
                rest = after;
            }
            None => return (program, None),
        }
    }
    if RUNNERS.contains(&program.as_str())
        && let Some(package) = rest.iter().find(|word| !word.starts_with('-'))
    {
        let package = without_version(package);
        return (named(package), Some(package.to_owned()));
    }
    (program, None)
}

/// A runtime's script: a file, or a module (`python -m aider`).
enum Script<'a> {
    File(&'a str),
    Module(&'a str),
}

/// The script among a runtime's arguments, and the arguments after it;
/// `None` where it runs none (code given inline, or a prompt of its own).
fn script(args: &[String]) -> Option<(Script<'_>, &[String])> {
    let mut at = 0;
    while let Some(word) = args.get(at) {
        match word.as_str() {
            "-m" => {
                return args
                    .get(at + 1)
                    .map(|module| (Script::Module(module), &args[at + 2..]));
            }
            "-e" | "-p" | "-c" | "--eval" | "--print" => return None,
            // Flags that take a value of their own, which is no script.
            "-r" | "--require" | "--import" | "--loader" | "--env-file" | "-W" | "-X"
            | "--rcfile" | "--init-file" | "-o" | "-O" => {
                at += 2;
            }
            // `deno run`, `bun run`: the script follows.
            "run" | "x" if at == 0 => at += 1,
            flag if flag.starts_with('-') => at += 1,
            path => return Some((Script::File(path), &args[at + 1..])),
        }
    }
    None
}

/// The package a script lies in, where it is one of Muster's own agents'
/// (`…/node_modules/@anthropic-ai/claude-code/cli.js`).
fn package_of(path: &str) -> Option<String> {
    PACKAGES
        .iter()
        .find(|(package, _)| path.contains(&format!("/{package}/")))
        .map(|(package, _)| (*package).to_owned())
}

/// What a package is called by: its name without its scope
/// (`@anthropic-ai/claude-code` is `claude-code`).
fn named(package: &str) -> String {
    package.rsplit('/').next().unwrap_or(package).to_owned()
}

/// A package without the version asked for (`aider-chat@0.80`,
/// `@openai/codex@latest`).
fn without_version(package: &str) -> &str {
    let scoped = usize::from(package.starts_with('@'));
    match package[scoped..].find('@') {
        Some(at) => &package[..scoped + at],
        None => package,
    }
}

/// Whether `program` runs a script named after the script: `node`,
/// `python3.12`.
fn is_runtime(program: &str) -> bool {
    let bare = program.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
    RUNTIMES.contains(&bare)
}

/// A command's name as a person would say it: the last part of its path,
/// without a login shell's dash or a script's extension.
fn command_name(word: &str) -> String {
    let name = basename(word).trim_start_matches('-');
    let name = [".js", ".mjs", ".cjs", ".ts", ".py", ".exe"]
        .iter()
        .find_map(|extension| name.strip_suffix(extension))
        .unwrap_or(name);
    crate::text::one_line(name).trim().to_owned()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn front(argv: &[&str]) -> Front {
        front_named("", argv)
    }

    fn front_named(name: &str, argv: &[&str]) -> Front {
        let facts = ProcessFacts {
            name: name.to_owned(),
            argv: argv.iter().map(|word| (*word).to_owned()).collect(),
        };
        identify(&facts, [("pi", "/opt/pi/bin/pi"), ("aider", "aider")])
    }

    fn agent(argv: &[&str]) -> Option<String> {
        front(argv).agent
    }

    #[test]
    fn muster_s_own_agents_are_found_however_they_are_started_arc_28() {
        // Its own installer's file is named for its version.
        assert_eq!(
            front_named("2.1.300", &["claude", "--resume"])
                .agent
                .as_deref(),
            Some("claude")
        );
        assert_eq!(
            agent(&["/opt/homebrew/bin/claude"]).as_deref(),
            Some("claude")
        );
        assert_eq!(
            agent(&[
                "node",
                "--no-warnings",
                "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"
            ])
            .as_deref(),
            Some("claude")
        );
        assert_eq!(
            agent(&["node", "/opt/homebrew/bin/codex", "exec"]).as_deref(),
            Some("codex")
        );
        assert_eq!(
            agent(&["npx", "-y", "@anthropic-ai/claude-code@latest"]).as_deref(),
            Some("claude")
        );
        assert_eq!(agent(&["bunx", "@openai/codex"]).as_deref(), Some("codex"));
        assert_eq!(
            agent(&["bun", "run", "/x/gemini.js"]).as_deref(),
            Some("gemini")
        );
        assert_eq!(agent(&["opencode"]).as_deref(), Some("opencode"));
        assert_eq!(agent(&["copilot"]).as_deref(), Some("copilot"));
    }

    #[test]
    fn an_agent_the_person_added_is_found_by_its_program_s_name() {
        assert_eq!(agent(&["pi", "--tui"]).as_deref(), Some("pi"));
        assert_eq!(
            agent(&["/usr/bin/python3.12", "/home/f/.local/bin/aider"]).as_deref(),
            Some("aider")
        );
        assert_eq!(agent(&["python3", "-m", "aider"]).as_deref(), Some("aider"));
        assert_eq!(agent(&["uvx", "aider@0.80"]).as_deref(), Some("aider"));
    }

    #[test]
    fn anything_else_is_a_program_by_its_name() {
        let shell = front_named("zsh", &["-zsh"]);
        assert_eq!((shell.program.as_str(), shell.agent), ("zsh", None));
        assert_eq!(front(&["cargo", "build"]).program, "cargo");
        assert_eq!(front(&["vim", "claude.md"]).agent, None);
        // A script by its own name, not its interpreter's.
        let script = front(&["python3", "-u", "/srv/tools/serve.py", "--port", "8000"]);
        assert_eq!((script.program.as_str(), script.agent), ("serve", None));
        assert_eq!(front(&["node", "-e", "console.log(1)"]).program, "node");
        assert_eq!(front(&["node"]).program, "node");
        assert_eq!(front(&["npx", "vite"]).program, "vite");
        // No arguments to read: the system's name.
        assert_eq!(front_named("htop", &[]).program, "htop");
        // A shell at its prompt is itself; running a script, the script.
        assert_eq!(
            front(&["/bin/bash", "--rcfile", "/x/rc", "-i"]).program,
            "bash"
        );
        assert_eq!(front(&["bash", "-c", "make test"]).program, "bash");
        assert_eq!(
            front(&["/bin/sh", "/usr/local/bin/codex"]).agent.as_deref(),
            Some("codex")
        );
    }

    #[test]
    fn a_package_s_version_is_no_part_of_its_name() {
        assert_eq!(without_version("@openai/codex@0.162"), "@openai/codex");
        assert_eq!(without_version("aider-chat@0.80"), "aider-chat");
        assert_eq!(without_version("vite"), "vite");
        assert_eq!(without_version("@scope/tool"), "@scope/tool");
    }
}
