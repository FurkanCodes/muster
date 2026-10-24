//! `muster exec AGENT [WORDS…]` (hidden): a coding agent run in this
//! terminal of Muster's, with Muster's hooks, as `muster init`'s functions
//! run one typed by its name (`PRODUCT.md` §6b, `ARC-28`). The daemon
//! gives the agent's command line for this terminal (`Command::ExecAgent`)
//! and hears it from then on as the terminal's agent; this runs it here,
//! in the person's foreground, and as it ends tells the daemon on the
//! terminal itself (`muster_core::front::AGENT_ENDED`), so the terminal
//! is a terminal again at once. Outside a terminal of Muster's, or where
//! its daemon cannot say, the agent's own program runs as it is.

use std::io::Write as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::ExitCode;

use muster_core::front::{AGENT_ENDED, AGENT_VARIABLE};
use muster_core::protocol::{Command, EXEC_PROTOCOL, Reply};
use muster_core::{Paths, SessionId};
use nix::sys::signal::{Signal, raise};
use nix::sys::wait::{WaitPidFlag, WaitStatus, waitpid};
use nix::unistd::Pid;

use tokio::signal::unix::SignalKind;

use crate::commands;

/// What `muster exec` was asked.
#[derive(Debug, clap::Args)]
pub(crate) struct Asked {
    /// The agent, by its harness's name.
    agent: String,
    /// Once it ends, start this shell in its place: a terminal Muster
    /// opened for an agent stays the person's terminal.
    #[arg(long, value_name = "SHELL")]
    then_shell: Option<std::path::PathBuf>,
    /// The words are the agent's whole command line, as the daemon gave it
    /// at the terminal's start: nothing is asked.
    #[arg(long)]
    as_given: bool,
    /// The prompt's words, or the command line with `--as-given`.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    words: Vec<String>,
}

/// The agent's command line: its program, arguments and variables.
type Line = (String, Vec<String>, Vec<(String, String)>);

pub(crate) async fn run(paths: &Paths, asked: Asked) -> ExitCode {
    let inside = super::started_by();
    let line: Line = if asked.as_given {
        let mut words = asked.words.into_iter();
        let Some(program) = words.next() else {
            return crate::fail("no command line to run");
        };
        (program, words.collect(), Vec::new())
    } else {
        let prompt = (!asked.words.is_empty()).then(|| asked.words.join(" "));
        match inside {
            Some(session) => match command_line(paths, session, &asked.agent, prompt).await {
                Ok(line) => line,
                Err(error) => {
                    eprintln!("muster: {error}; {} runs on its own", asked.agent);
                    (asked.agent.clone(), asked.words, Vec::new())
                }
            },
            None => (asked.agent.clone(), asked.words, Vec::new()),
        }
    };
    let code = match run_here(&asked.agent, &line) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("muster: could not run {}: {error}", line.0);
            127
        }
    };
    if inside.is_some() {
        let mut out = std::io::stdout();
        let _ = write!(out, "\x1b]{AGENT_ENDED}{code}\x07");
        let _ = out.flush();
    }
    if let Some(shell) = asked.then_shell {
        return then(&shell);
    }
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}

/// What the daemon says to run `agent` with in terminal `session`.
async fn command_line(
    paths: &Paths,
    session: SessionId,
    agent: &str,
    prompt: Option<String>,
) -> Result<Line, String> {
    let mut connection = commands::connect(&paths.socket())
        .await
        .map_err(|error| error.to_string())?;
    if connection.welcome().version < EXEC_PROTOCOL {
        return Err("the daemon running is older than this muster".to_owned());
    }
    let reply = connection
        .command(Command::ExecAgent {
            session,
            harness: agent.to_owned(),
            prompt,
        })
        .await
        .map_err(|error| error.to_string())?;
    match reply {
        Reply::AgentCommand { program, args, env } => Ok((program, args, env)),
        other => Err(format!("unexpected reply {other:?}")),
    }
}

/// Runs the agent in this terminal's foreground until it ends: its exit
/// code, or 128 and the signal that ended it. Keys that interrupt or quit
/// are the agent's alone, and where it stops itself (Ctrl+Z), this stops
/// too, so the shell above has the terminal back until `fg`.
fn run_here(agent: &str, (program, args, env): &Line) -> std::io::Result<i32> {
    let child = std::process::Command::new(program)
        .args(args)
        .envs(env.iter().map(|(key, value)| (key, value)))
        .env(AGENT_VARIABLE, agent)
        .spawn()?;
    // Heard and passed over, never ending this: a listener's handler, not
    // an ignored signal, which the agent would inherit.
    let _held = [SignalKind::interrupt(), SignalKind::quit()]
        .map(|kind| tokio::signal::unix::signal(kind).ok());
    let pid = Pid::from_raw(i32::try_from(child.id()).unwrap_or(i32::MAX));
    loop {
        match waitpid(pid, Some(WaitPidFlag::WUNTRACED)) {
            Ok(WaitStatus::Exited(_, code)) => return Ok(code),
            Ok(WaitStatus::Signaled(_, signal, _)) => return Ok(128 + signal as i32),
            Ok(WaitStatus::Stopped(..)) => {
                let _ = raise(Signal::SIGSTOP);
            }
            Ok(_) | Err(nix::errno::Errno::EINTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

/// Starts `shell` in this process's place; returns only if it cannot.
fn then(shell: &Path) -> ExitCode {
    let error = std::process::Command::new(shell).exec();
    crate::fail(format!("could not start {}: {error}", shell.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn the_agent_s_exit_code_is_this_one_s() {
        let line = (
            "/bin/sh".to_owned(),
            vec!["-c".to_owned(), "exit 3".to_owned()],
            Vec::new(),
        );
        assert_eq!(run_here("sh", &line).unwrap(), 3);
        let killed = (
            "/bin/sh".to_owned(),
            vec!["-c".to_owned(), "kill -TERM $$".to_owned()],
            Vec::new(),
        );
        assert_eq!(run_here("sh", &killed).unwrap(), 128 + 15);
    }
}
