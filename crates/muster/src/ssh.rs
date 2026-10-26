//! Muster on a machine of the person's own, reached with their `ssh`
//! (`PRODUCT.md` §6b): `muster machine add vm furkan@203.0.113.7` saves
//! one, and `muster ssh vm` asks it what it has, installs Muster there
//! after asking (a copy of this one where it runs the same build, else
//! this version's release for it, downloaded here and sent over ssh, so
//! the machine needs only ssh), says what is missing
//! for agents to work there, and opens a shell there through ssh, where
//! `muster` and the agents are found, or runs one command (`muster ssh vm
//! claude auth login`); Muster's own commands from here are `muster --on
//! vm list agents`. Agents there keep working while the person's own
//! computer sleeps.
//!
//! The machine is reached only through the `ssh` program on `PATH`, with
//! the person's own settings, keys and agent (`ARC-27`): Muster has no
//! server, account or listener of its own, and keeps no credential.

mod args;
mod link;
mod report;

use std::io::{IsTerminal, Write as _};
use std::os::unix::process::CommandExt as _;
use std::process::{ExitCode, Stdio};

use clap::Subcommand;
use muster_client::ClientError;
use muster_core::Paths;
use muster_core::machines::{self, Machines, Probe};
use sha2::Digest as _;

use self::args::{
    How, REMOTE_PATH, control_dir, login_shell, quoted, shell_command, ssh_args, user_for,
};
pub(crate) use self::link::{bridge, connector};

/// This build's version, which a machine is brought up to.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where Muster puts itself on a machine, as its installer does.
const REMOTE_BIN: &str = "$HOME/.local/bin/muster";

/// Asks a machine where its `muster` is, which version, and its system,
/// each line marked (`muster_core::machines::Probe`). Run by `sh`, whatever
/// the person's shell there is.
fn probe_script() -> String {
    format!(
        "{REMOTE_PATH}; m=$(command -v muster 2>/dev/null); echo \"muster-probe:path=$m\"; \
[ -n \"$m\" ] && echo \"muster-probe:version=$(\"$m\" --version 2>/dev/null)\"; \
echo \"muster-probe:system=$(uname -sm)\"; true\n"
    )
}

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

/// A connection to Muster on `remote` through ssh, asking the person only
/// where they are at a terminal (`link::connect`).
pub(crate) async fn connect(remote: &Remote) -> Result<muster_client::Connection, ClientError> {
    link::connect(remote, link::how_here()).await
}

/// What `muster ssh` was asked.
#[derive(Debug)]
pub(crate) struct Asked {
    pub(crate) machine: String,
    pub(crate) install: bool,
    pub(crate) no_install: bool,
    pub(crate) check: bool,
    pub(crate) install_claude: bool,
    pub(crate) dir: Option<String>,
    pub(crate) words: Vec<String>,
}

/// `muster ssh MACHINE [COMMAND…]`.
pub(crate) async fn run(
    paths: &Paths,
    asked: &Asked,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let target = read_machines(paths)?.target(&asked.machine)?;
    say_user(&target);
    progress(&format!("Connecting to {target}…"));
    let probe = Probe::read(&ask(&target, &probe_script())?);
    let system = probe.system.clone().unwrap_or_else(|| "?".to_owned());
    progress(&found(&probe, &system));
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
        if !asked.install && !crate::person::agrees(&question)? {
            if probe.muster.is_none() {
                return Err(format!(
                    "Muster is not on {}: `muster ssh {} --install` installs it, or install it \
                     there yourself (its README's one line)",
                    asked.machine, asked.machine
                )
                .into());
            }
        } else {
            install(&target, &probe).await?;
            installed = true;
        }
    }
    if installed || asked.check || asked.install_claude {
        progress("Checking what agents need there…");
        let checked = ask(&target, &report::script())?;
        eprint!(
            "{}",
            report::report(&asked.machine, &system, VERSION, &checked)
        );
        if report::claude_missing(&checked)
            && (asked.install_claude
                || crate::person::agrees(&format!(
                    "Install Claude Code on {} with its own installer?",
                    asked.machine
                ))?)
        {
            install_claude(&target, &asked.machine, &checked)?;
        }
    }
    if (asked.check || asked.install_claude) && asked.words.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    // The person's shell there, or their command, as ssh runs one.
    let command = if asked.words.is_empty() {
        progress(&format!("Opening a shell on {}…", asked.machine));
        login_shell(asked.dir.as_deref())
    } else {
        shell_command(asked.dir.as_deref(), &asked.words)
    };
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let error = std::process::Command::new("ssh")
        .args(ssh_args(
            How::Interactive,
            tty,
            &target,
            control_dir().as_deref(),
        ))
        .arg(command)
        .exec();
    Err(format!("could not run ssh: {error}").into())
}

/// How Muster gets onto a machine with `probe`'s system (`install`).
fn install_words(probe: &Probe) -> &'static str {
    if given_binary().is_some() {
        "the binary MUSTER_REMOTE_BINARY names, over ssh"
    } else if probe.same_build(std::env::consts::OS, std::env::consts::ARCH) {
        "a copy of this one, over ssh"
    } else if probe.release_target().is_some() {
        "its release for that system, downloaded here and checked, then sent over ssh"
    } else {
        "its installer, run there, which builds it from source"
    }
}

/// The binary the person asks to be put on the machine instead of this one
/// or a release (`MUSTER_REMOTE_BINARY`): a build for its system, such as
/// one of a branch not released yet.
fn given_binary() -> Option<std::path::PathBuf> {
    std::env::var_os("MUSTER_REMOTE_BINARY")
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from)
}

/// Runs `script` with `sh` on the machine, given on its standard input so
/// no shell there reads it first, and returns what it printed. ssh may ask
/// the person (a password, a new host key): this is where they meet it.
fn ask(target: &str, script: &str) -> Result<String, Box<dyn std::error::Error>> {
    let mut child = ssh(target)
        .arg("sh -s")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("could not run ssh: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(script.as_bytes())?;
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!(
            "ssh to {target} failed ({}): check you can `ssh {target}`",
            output.status
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `ssh` to `target` as the person runs it, at their terminal.
fn ssh(target: &str) -> std::process::Command {
    let mut command = std::process::Command::new("ssh");
    command.args(ssh_args(
        How::Interactive,
        false,
        target,
        control_dir().as_deref(),
    ));
    command
}

/// Installs this version on the machine as a binary sent over ssh and
/// checked there by its SHA-256: the one `MUSTER_REMOTE_BINARY` names, a copy
/// of this one where it runs the same build, else this version's release
/// for its system, downloaded and checked here, so the machine needs no
/// internet or toolchain. Only a system Muster has no release for runs the
/// release's installer there, which builds from source. Then its daemon
/// is moved onto it if one runs (`PRD-09`).
async fn install(target: &str, probe: &Probe) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = if let Some(given) = given_binary() {
        read_binary(&given)?
    } else if probe.same_build(std::env::consts::OS, std::env::consts::ARCH) {
        read_binary(&std::env::current_exe()?)?
    } else if let Some(release) = probe.release_target() {
        progress(&format!(
            "Downloading muster {VERSION} for {release} and checking it…"
        ));
        crate::upgrade::release_binary(VERSION, &release).await?
    } else {
        progress("Running Muster's installer there…");
        let script = format!(
            "curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/v{VERSION}/install.sh | MUSTER_VERSION=v{VERSION} sh -s -- --no-app"
        );
        let status = ssh(target)
            .arg(format!("sh -c {}", quoted(&script)))
            .stdin(Stdio::null())
            .status()?;
        if !status.success() {
            return Err("its installer failed there; what it said is above".into());
        }
        reloaded(target);
        return Ok(());
    };
    progress(&format!(
        "Sending muster {VERSION} there ({} MB)…",
        bytes.len().div_ceil(1_048_576)
    ));
    put(target, &bytes)?;
    reloaded(target);
    Ok(())
}

fn read_binary(path: &std::path::Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()).into())
}

/// Puts `bytes` on the machine as its muster, through ssh: copied beside
/// it, checked to be them and to run there, then put in its place, so a
/// copy changed on its way, or one the machine cannot run, replaces nothing.
fn put(target: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let ours = muster_core::hex::encode(&sha2::Sha256::digest(bytes));
    let part = format!("{REMOTE_BIN}.part");
    let copy = format!("mkdir -p \"$HOME/.local/bin\" && cat > \"{part}\" && chmod 755 \"{part}\"");
    let mut child = ssh(target)
        .arg(format!("sh -c {}", quoted(&copy)))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("no way to send it")?
        .write_all(bytes)?;
    if !child.wait()?.success() {
        return Err("could not copy muster there".into());
    }
    let said = ask(
        target,
        &format!(
            "s=$(shasum -a 256 \"{part}\" 2>/dev/null || sha256sum \"{part}\"); \
             echo \"muster-sha256:${{s%% *}}\"; \
             echo \"muster-runs:$(\"{part}\" --version 2>/dev/null)\"\n"
        ),
    )?;
    let line = |mark: &str| {
        said.lines()
            .find_map(|line| line.trim().strip_prefix(mark).map(str::to_owned))
    };
    progress("Checking the copy there…");
    if line("muster-sha256:").as_deref() != Some(ours.as_str()) {
        return Err("the copy there is not this muster: it was changed on its way".into());
    }
    // A build of the person's own may be of another version.
    if !line("muster-runs:").is_some_and(|version| version.starts_with("muster ")) {
        return Err(
            "the copy there does not run on that machine, so nothing was replaced: is it the \
             machine's system and processor?"
                .into(),
        );
    }
    ask(target, &format!("mv \"{part}\" \"{REMOTE_BIN}\"\n"))?;
    Ok(())
}

/// Moves a daemon running there onto the new muster in place; none
/// running is fine.
fn reloaded(target: &str) {
    let reloaded = ssh(target)
        .arg(format!(
            "sh -c {}",
            quoted(&format!("\"{REMOTE_BIN}\" reload"))
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if reloaded {
        eprintln!("Installed muster {VERSION} there.");
    } else {
        eprintln!(
            "Installed muster {VERSION} there, but its daemon could not be moved onto it: \
             `muster ssh {target} muster reload` tries again."
        );
    }
}

/// Says a step of `muster ssh` at the person's terminal, so the wait for
/// ssh, a download or a copy is never silent: one plain line each, as ssh
/// may ask for a password or a host key between them. Nothing where
/// standard error is no terminal (a script, a log).
fn progress(text: &str) {
    if std::io::stderr().is_terminal() {
        eprintln!("· {text}");
    }
}

/// What the probe found there, in words.
fn found(probe: &Probe, system: &str) -> String {
    match &probe.version {
        Some(version) => format!("Connected ({system}): muster {version} is there."),
        None if probe.muster.is_some() => format!("Connected ({system}): muster is there."),
        None => format!("Connected ({system}): Muster is not there yet."),
    }
}

/// Says who ssh signs in as where the target names no user and ssh's
/// settings name none either: the person's own name here, which a
/// machine of theirs often does not have, so a password asked for next
/// would never be right.
fn say_user(target: &str) {
    let mine = std::env::var("USER").unwrap_or_default();
    if let Some(user) = user_for(target).filter(|user| !mine.is_empty() && *user == mine) {
        eprintln!(
            "Signing in to {target} as {user}, your name here: for another, `muster ssh \
             you@{target}` (or a `User` for it in ~/.ssh/config)."
        );
    }
}

/// The official installer Claude Code is installed with.
const CLAUDE_INSTALLER: &str = "https://claude.ai/install.sh";

/// Installs Claude Code on the machine with its own installer, run there
/// in the person's terminal, then says how to sign it in there: Muster
/// copies no sign-in (`PRD-06`).
fn install_claude(
    target: &str,
    machine: &str,
    checked: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if !report::has(checked, "curl") {
        return Err(format!(
            "Claude Code's installer needs curl, which is not on {machine}: install it there \
             (`muster ssh {machine} sudo apt install curl`), then ask again"
        )
        .into());
    }
    let tty = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let status = std::process::Command::new("ssh")
        .args(ssh_args(
            How::Interactive,
            tty,
            target,
            control_dir().as_deref(),
        ))
        .arg(shell_command(
            None,
            &[format!("curl -fsSL {CLAUDE_INSTALLER} | bash")],
        ))
        .status()
        .map_err(|error| format!("could not run ssh: {error}"))?;
    if !status.success() {
        return Err(
            format!("Claude Code's installer on {machine} failed; what it said is above").into(),
        );
    }
    eprintln!(
        "Claude Code is on {machine}. Sign it in there once: `muster ssh {machine} claude auth \
         login` (open its link here, paste the code back)."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_the_probe_found_is_said_in_words_arc_27() {
        let probe = |muster: Option<&str>, version: Option<&str>| Probe {
            muster: muster.map(str::to_owned),
            version: version.map(str::to_owned),
            system: None,
        };
        let there = found(
            &probe(Some("/usr/bin/muster"), Some("0.7.1")),
            "Linux x86_64",
        );
        assert_eq!(there, "Connected (Linux x86_64): muster 0.7.1 is there.");
        assert!(found(&probe(Some("/m"), None), "?").ends_with("muster is there."));
        assert!(found(&probe(None, None), "?").ends_with("Muster is not there yet."));
    }
}
