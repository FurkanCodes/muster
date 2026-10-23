//! Muster on a machine of the person's own, reached with their `ssh`
//! (`PRODUCT.md` §6b): `muster machine add vm furkan@203.0.113.7` saves
//! one, and `muster ssh vm` asks it what it has, installs Muster there
//! after asking (a copy of this one where it runs the same build, else the
//! release of this version from its own installer), says what is missing
//! for agents to work there, and opens its Muster through ssh: its UI, or
//! one command (`muster ssh vm list agents`). Agents there keep working
//! while the person's own computer sleeps.
//!
//! The machine is reached only through the `ssh` program on `PATH`, with
//! the person's own settings, keys and agent (`ARC-27`): Muster has no
//! server, account or listener of its own, and keeps no credential.

use std::fmt::Write as _;
use std::io::{IsTerminal, Write as _};
use std::os::unix::process::CommandExt as _;
use std::process::{ExitCode, Stdio};

use clap::Subcommand;
use muster_client::ClientError;
use muster_core::Paths;
use muster_core::machines::{self, Machines, Probe};
use sha2::Digest as _;

/// This build's version, which a machine is brought up to.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where Muster puts itself on a machine, as its installer does.
const REMOTE_BIN: &str = "$HOME/.local/bin/muster";

/// Asks a machine where its `muster` is, which version, and its system,
/// each line marked (`muster_core::machines::Probe`). Run by `sh`, whatever
/// the person's shell there is.
const PROBE: &str = "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH\"; \
m=$(command -v muster 2>/dev/null); echo \"muster-probe:path=$m\"; \
[ -n \"$m\" ] && echo \"muster-probe:version=$(\"$m\" --version 2>/dev/null)\"; \
echo \"muster-probe:system=$(uname -sm)\"";

/// Asks a machine what agents work with there: each agent's program, git
/// and gh, whether Claude Code and gh are signed in, and whether the
/// person's processes outlive their logging out (systemd's lingering).
const CHECK: &str = "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH\"; \
for a in claude codex gemini opencode copilot git gh; do echo \"muster-check:$a=$(command -v $a 2>/dev/null)\"; done; \
[ -f \"$HOME/.claude/.credentials.json\" ] && echo muster-check:claude-signed-in=yes; \
gh auth status >/dev/null 2>&1 && echo muster-check:gh-signed-in=yes; \
command -v loginctl >/dev/null 2>&1 && echo \"muster-check:linger=$(loginctl show-user \"$(id -un)\" -p Linger --value 2>/dev/null)\"; true";

#[derive(Debug, Subcommand)]
pub(crate) enum MachineCommand {
    /// Save machine NAME: what `ssh` is given to reach it (`user@host`, a
    /// host, or a name from your ~/.ssh/config). Only that is kept: your
    /// keys and settings stay ssh's.
    Add { name: String, target: String },
    /// Forget machine NAME. Muster on it, and its sessions, are left as
    /// they are.
    #[command(visible_alias = "rm")]
    Remove { name: String },
}

/// The machines saved, from Muster's own settings folder.
fn read_machines(paths: &Paths) -> Result<Machines, Box<dyn std::error::Error>> {
    match std::fs::read_to_string(paths.config_dir.join(machines::FILE)) {
        Ok(text) => Ok(Machines::parse(&text)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Machines::default()),
        Err(error) => Err(error.into()),
    }
}

fn write_machines(paths: &Paths, machines: &Machines) -> std::io::Result<()> {
    std::fs::create_dir_all(&paths.config_dir)?;
    let path = paths.config_dir.join(machines::FILE);
    let partial = path.with_extension("toml.part");
    std::fs::write(&partial, machines.to_text())?;
    std::fs::rename(partial, path)
}

pub(crate) fn machine(
    paths: &Paths,
    command: MachineCommand,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let mut machines = read_machines(paths)?;
    match command {
        MachineCommand::Add { name, target } => {
            machines.add(&name, &target)?;
            write_machines(paths, &machines)?;
            println!("Saved {name} ({target}). `muster ssh {name}` goes there.");
        }
        MachineCommand::Remove { name } => {
            if machines.machines.remove(&name).is_none() {
                return Err(
                    format!("no machine is called {name:?} (`muster list machines`)").into(),
                );
            }
            write_machines(paths, &machines)?;
            println!("Forgot {name}; Muster on it is left as it is.");
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `muster list machines`.
pub(crate) fn list(paths: &Paths, json: bool) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let machines = read_machines(paths)?;
    if json {
        let listed: Vec<serde_json::Value> = machines
            .machines
            .iter()
            .map(|(name, machine)| serde_json::json!({"name": name, "target": machine.target}))
            .collect();
        crate::json::print(&serde_json::json!({
            "format": crate::json::FORMAT,
            "machines": listed,
        }))?;
        return Ok(ExitCode::SUCCESS);
    }
    if machines.machines.is_empty() {
        println!("No machines. `muster machine add vm you@host` saves one.");
    }
    for (name, machine) in &machines.machines {
        println!("{name:<16}  {}", machine.target);
    }
    Ok(ExitCode::SUCCESS)
}

/// The machine every command talks to, with `--on`: its name and what
/// ssh is given.
#[derive(Debug)]
pub(crate) struct Remote {
    pub(crate) name: String,
    pub(crate) target: String,
}

static REMOTE: std::sync::OnceLock<Remote> = std::sync::OnceLock::new();

/// Sends every command to machine `name` (`--on`), as it is saved or as
/// ssh knows it.
pub(crate) fn set_remote(paths: &Paths, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let target = read_machines(paths)?.target(name)?;
    let _ = REMOTE.set(Remote {
        name: name.to_owned(),
        target,
    });
    Ok(())
}

/// The machine commands go to, with `--on`.
pub(crate) fn remote() -> Option<&'static Remote> {
    REMOTE.get()
}

/// Has Muster on `remote` carry this connection: `ssh` runs `muster bridge`
/// there, which joins it to that machine's daemon, starting one where none
/// runs; the frames cross ssh's standard input and output as they are.
pub(crate) async fn connect(remote: &Remote) -> Result<muster_client::Connection, ClientError> {
    let bridge =
        "PATH=\"$HOME/.local/bin:$HOME/.cargo/bin:/usr/local/bin:$PATH\"; exec muster bridge";
    let mut child = tokio::process::Command::new("ssh")
        .args([
            "-T",
            "--",
            &remote.target,
            &format!("sh -c {}", quoted(bridge)),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| ClientError::Carrier(format!("could not run ssh: {error}")))?;
    let (Some(writer), Some(reader)) = (child.stdin.take(), child.stdout.take()) else {
        return Err(ClientError::Carrier("ssh gave no way to talk".to_owned()));
    };
    let name = &remote.name;
    match muster_client::Connection::over(reader, writer, crate::commands::CLIENT).await {
        Ok(connection) => Ok(connection.holding(child)),
        Err(ClientError::Closed | ClientError::Transport(_)) => Err(ClientError::Carrier(format!(
            "could not reach Muster on {name} over ssh: `muster ssh {name} --check` says why"
        ))),
        Err(ClientError::Incompatible(theirs)) => Err(ClientError::Carrier(format!(
            "Muster on {name} is {}, another version than this one: `muster ssh {name} --install` \
             brings it to {VERSION}",
            theirs.daemon_version
        ))),
        Err(error) => Err(error),
    }
}

/// [`connect`] to `remote`, for the UI to call each time its link is lost.
pub(crate) fn connector(remote: &'static Remote) -> muster_tui::Connector {
    Box::new(move || {
        let connecting: std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<muster_client::Connection, ClientError>>
                    + Send,
            >,
        > = Box::pin(connect(remote));
        connecting
    })
}

/// `muster bridge`, which `--on` has ssh run on the machine: joins its
/// standard input and output to this machine's daemon, starting it where
/// none runs, until either side closes.
pub(crate) async fn bridge(paths: &Paths) -> Result<ExitCode, Box<dyn std::error::Error>> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    let socket = paths.socket();
    let stream = if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
        stream
    } else {
        crate::daemon_start::start(paths)?;
        let deadline = crate::instant_now() + muster_tui::START_WAIT;
        loop {
            match tokio::net::UnixStream::connect(&socket).await {
                Ok(stream) => break stream,
                Err(_) if crate::instant_now() < deadline => {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                }
                Err(error) => return Err(format!("the daemon did not start: {error}").into()),
            }
        }
    };
    let (mut from_daemon, mut to_daemon) = stream.into_split();
    let to = async {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = stdin.read(&mut buffer).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(());
            }
            to_daemon.write_all(&buffer[..read]).await?;
        }
    };
    let from = async {
        let mut stdout = tokio::io::stdout();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            let read = from_daemon.read(&mut buffer).await?;
            if read == 0 {
                return Ok::<_, std::io::Error>(());
            }
            stdout.write_all(&buffer[..read]).await?;
            stdout.flush().await?;
        }
    };
    tokio::select! {
        _ = to => {}
        _ = from => {}
    }
    // Ended at once: a read of standard input still waiting would hold the
    // process until ssh closed it.
    std::process::exit(0);
}

/// What `muster ssh` was asked.
#[derive(Debug)]
pub(crate) struct Asked {
    pub(crate) machine: String,
    pub(crate) install: bool,
    pub(crate) no_install: bool,
    pub(crate) check: bool,
    pub(crate) dir: Option<String>,
    pub(crate) words: Vec<String>,
}

/// `muster ssh MACHINE [COMMAND…]`.
pub(crate) fn run(paths: &Paths, asked: &Asked) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let target = read_machines(paths)?.target(&asked.machine)?;
    let probe = Probe::read(&ask(&target, PROBE)?);
    let system = probe.system.clone().unwrap_or_else(|| "?".to_owned());
    let mut remote = probe.muster.clone();
    let mut installed = false;
    if probe.needs(VERSION) && !asked.no_install {
        let how = install_words(&probe);
        let question = match &probe.version {
            Some(theirs) => format!(
                "{} has muster {theirs}; this is {VERSION}. Bring it up to {VERSION} ({how})?",
                asked.machine
            ),
            None => format!(
                "Muster is not on {} ({system}). Install muster {VERSION} there ({how})?",
                asked.machine
            ),
        };
        if !asked.install && !agrees(&question)? {
            if probe.muster.is_none() {
                return Err(format!(
                    "Muster is not on {}: `muster ssh {} --install` installs it, or install it \
                     there yourself (its README's one line)",
                    asked.machine, asked.machine
                )
                .into());
            }
        } else {
            install(&target, &probe)?;
            remote = Some(REMOTE_BIN.to_owned());
            installed = true;
        }
    }
    let Some(remote) = remote else {
        return Err(format!("Muster is not on {}", asked.machine).into());
    };
    if installed || asked.check {
        let report = report(&asked.machine, &system, &ask(&target, CHECK)?);
        eprint!("{report}");
    }
    if asked.check && asked.words.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let command = remote_command(&remote, asked.dir.as_deref(), &asked.words);
    let mut ssh = std::process::Command::new("ssh");
    if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
        ssh.arg("-t");
    }
    let error = ssh.arg("--").arg(&target).arg(command).exec();
    Err(format!("could not run ssh: {error}").into())
}

/// How Muster gets onto a machine with `probe`'s system: a copy of this
/// one where it runs the same build, else this version's release.
fn install_words(probe: &Probe) -> &'static str {
    if probe.same_build(std::env::consts::OS, std::env::consts::ARCH) {
        "a copy of this one, over ssh"
    } else {
        "its release, from GitHub, checked"
    }
}

/// Runs `script` with `sh` on the machine and returns what it printed.
fn ask(target: &str, script: &str) -> Result<String, Box<dyn std::error::Error>> {
    let output = std::process::Command::new("ssh")
        .args(["-T", "--", target, &format!("sh -c {}", quoted(script))])
        .stdin(Stdio::null())
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("could not run ssh: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "ssh to {target} failed ({}): check you can `ssh {target}`",
            output.status
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Installs this version on the machine: this binary copied over ssh and
/// checked by its SHA-256 where it runs the same build, else the
/// installer of this version's release run there (checksum-verified),
/// then its daemon moved onto it if one runs (`PRD-09`).
fn install(target: &str, probe: &Probe) -> Result<(), Box<dyn std::error::Error>> {
    if probe.same_build(std::env::consts::OS, std::env::consts::ARCH) {
        let exe = std::env::current_exe()?;
        let bytes = std::fs::read(&exe)?;
        let ours = muster_core::hex::encode(&sha2::Sha256::digest(&bytes));
        let put = format!(
            "mkdir -p \"$HOME/.local/bin\" && cat > \"{REMOTE_BIN}.part\" && chmod 755 \"{REMOTE_BIN}.part\" && mv \"{REMOTE_BIN}.part\" \"{REMOTE_BIN}\""
        );
        let mut child = std::process::Command::new("ssh")
            .args(["-T", "--", target, &format!("sh -c {}", quoted(&put))])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()?;
        child
            .stdin
            .take()
            .ok_or("no way to send it")?
            .write_all(&bytes)?;
        if !child.wait()?.success() {
            return Err("could not copy muster there".into());
        }
        let theirs = ask(
            target,
            &format!("shasum -a 256 \"{REMOTE_BIN}\" 2>/dev/null || sha256sum \"{REMOTE_BIN}\""),
        )?;
        if theirs.split_whitespace().next() != Some(ours.as_str()) {
            return Err("the copy there is not this muster: it was changed on its way".into());
        }
    } else {
        let script = format!(
            "curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/v{VERSION}/install.sh | MUSTER_VERSION=v{VERSION} sh -s -- --no-app"
        );
        let status = std::process::Command::new("ssh")
            .args(["-T", "--", target, &format!("sh -c {}", quoted(&script))])
            .stdin(Stdio::null())
            .status()?;
        if !status.success() {
            return Err("its installer failed there; what it said is above".into());
        }
    }
    // A daemon running there moves onto it in place; none running is fine.
    let _ = std::process::Command::new("ssh")
        .args([
            "-T",
            "--",
            target,
            &format!("sh -c {}", quoted(&format!("\"{REMOTE_BIN}\" reload"))),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    eprintln!("Installed muster {VERSION} there.");
    Ok(())
}

/// What the machine has for agents, from the check's marked lines, and
/// what to do about what is missing.
fn report(machine: &str, system: &str, output: &str) -> String {
    let mut found = std::collections::BTreeMap::new();
    for line in output.lines() {
        if let Some((key, value)) = line
            .trim()
            .strip_prefix("muster-check:")
            .and_then(|rest| rest.split_once('='))
        {
            found.insert(key.to_owned(), value.trim().to_owned());
        }
    }
    let has = |key: &str| found.get(key).is_some_and(|value| !value.is_empty());
    let mut out = format!("On {machine} ({system}), muster {VERSION}:\n");
    let agents: Vec<&str> = ["claude", "codex", "gemini", "opencode", "copilot"]
        .into_iter()
        .filter(|agent| has(agent))
        .collect();
    if has("claude") {
        if found.get("claude-signed-in").map(String::as_str) == Some("yes") {
            out.push_str("  ✓ Claude Code, signed in\n");
        } else {
            out.push_str(
                "  ! Claude Code is there but may not be signed in: run `claude` once there to \
                 sign in (Muster never copies your sign-in)\n",
            );
        }
    } else {
        out.push_str(
            "  ✗ Claude Code is not there: install it there, then run `claude` once to sign in\n",
        );
    }
    let others: Vec<&str> = agents.iter().copied().filter(|a| *a != "claude").collect();
    if !others.is_empty() {
        let _ = writeln!(out, "  ✓ also {}", others.join(", "));
    }
    out.push_str(if has("git") {
        "  ✓ git\n"
    } else {
        "  ✗ git is not there: worktrees need it\n"
    });
    match (has("gh"), found.get("gh-signed-in").map(String::as_str)) {
        (true, Some("yes")) => out.push_str("  ✓ gh, signed in\n"),
        (true, _) => out.push_str(
            "  ! gh is not signed in: `gh auth login` there, for pull requests and tickets\n",
        ),
        (false, _) => out.push_str("  · no gh: pull requests and GitHub tickets need it\n"),
    }
    if found.get("linger").map(String::as_str) == Some("no") {
        out.push_str(
            "  ! systemd ends your processes when you log out: `loginctl enable-linger` there keeps \
             the agents working\n",
        );
    }
    out
}

/// The command ssh runs there: `muster` with the words, quoted for a
/// shell, in `dir` where given.
fn remote_command(remote: &str, dir: Option<&str>, words: &[String]) -> String {
    let muster = if remote.starts_with("$HOME") {
        format!("\"{remote}\"")
    } else {
        quoted(remote)
    };
    let mut command = format!("exec {muster}");
    for word in words {
        command.push(' ');
        command.push_str(&quoted(word));
    }
    match dir {
        Some(dir) => format!("cd {} && {command}", quoted(dir)),
        None => command,
    }
}

/// `text` in single quotes, as every shell reads it whole.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Whether the person, asked on their terminal, says yes; never, without
/// a terminal to ask on.
fn agrees(question: &str) -> std::io::Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_there_is_quoted_whole_arc_27() {
        assert_eq!(
            remote_command(
                "$HOME/.local/bin/muster",
                None,
                &["list".into(), "agents".into()]
            ),
            "exec \"$HOME/.local/bin/muster\" 'list' 'agents'"
        );
        assert_eq!(
            remote_command(
                "/opt/m/muster",
                Some("~/code"),
                &["claude".into(), "it's broken; rm -rf /".into()]
            ),
            "cd '~/code' && exec '/opt/m/muster' 'claude' 'it'\\''s broken; rm -rf /'"
        );
    }

    #[test]
    fn the_report_says_what_agents_need_there() {
        let said = report(
            "vm",
            "Linux x86_64",
            "muster-check:claude=/usr/bin/claude\nmuster-check:codex=\nmuster-check:git=/usr/bin/git\n\
             muster-check:gh=/usr/bin/gh\nmuster-check:linger=no\n",
        );
        assert!(said.contains("may not be signed in"), "{said}");
        assert!(said.contains("✓ git"), "{said}");
        assert!(said.contains("gh auth login"), "{said}");
        assert!(said.contains("enable-linger"), "{said}");
        assert!(!said.contains("codex"), "{said}");
    }
}
