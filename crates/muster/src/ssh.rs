//! Muster on a machine of the person's own, reached with their `ssh`
//! (`PRODUCT.md` §6b): `muster machine add vm furkan@203.0.113.7` saves
//! one, and `muster ssh vm` asks it what it has, installs Muster there
//! after asking (a copy of this one where it runs the same build, else
//! this version's release for it, downloaded here and sent over ssh, so
//! the machine needs only ssh), says what is missing
//! for agents to work there, and opens its Muster through ssh: its UI, or
//! one command (`muster ssh vm list agents`). Agents there keep working
//! while the person's own computer sleeps.
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

use self::args::{How, REMOTE_PATH, control_dir, quoted, remote_command, ssh_args};
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
    pub(crate) dir: Option<String>,
    pub(crate) words: Vec<String>,
}

/// `muster ssh MACHINE [COMMAND…]`.
pub(crate) async fn run(
    paths: &Paths,
    asked: &Asked,
) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let target = read_machines(paths)?.target(&asked.machine)?;
    let probe = Probe::read(&ask(&target, &probe_script())?);
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
            install(&target, &probe).await?;
            remote = Some(REMOTE_BIN.to_owned());
            installed = true;
        }
    }
    let Some(remote) = remote else {
        return Err(format!("Muster is not on {}", asked.machine).into());
    };
    if installed || asked.check {
        let said = report::report(
            &asked.machine,
            &system,
            VERSION,
            &ask(&target, &report::script())?,
        );
        eprint!("{said}");
    }
    if asked.check && asked.words.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    let command = remote_command(&remote, asked.dir.as_deref(), &asked.words);
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
        crate::upgrade::release_binary(VERSION, &release).await?
    } else {
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
    put(target, &bytes)?;
    reloaded(target);
    Ok(())
}

fn read_binary(path: &std::path::Path) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    std::fs::read(path).map_err(|error| format!("cannot read {}: {error}", path.display()).into())
}

/// Puts `bytes` on the machine as its muster, through ssh, and checks the
/// copy there is them.
fn put(target: &str, bytes: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let ours = muster_core::hex::encode(&sha2::Sha256::digest(bytes));
    let put = format!(
        "mkdir -p \"$HOME/.local/bin\" && cat > \"{REMOTE_BIN}.part\" && chmod 755 \"{REMOTE_BIN}.part\" && mv \"{REMOTE_BIN}.part\" \"{REMOTE_BIN}\""
    );
    let mut child = ssh(target)
        .arg(format!("sh -c {}", quoted(&put)))
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
    let theirs = ask(
        target,
        &format!(
            "s=$(shasum -a 256 \"{REMOTE_BIN}\" 2>/dev/null || sha256sum \"{REMOTE_BIN}\"); \
             echo \"muster-sha256:${{s%% *}}\"\n"
        ),
    )?;
    let theirs = theirs
        .lines()
        .find_map(|line| line.trim().strip_prefix("muster-sha256:"));
    if theirs != Some(ours.as_str()) {
        return Err("the copy there is not this muster: it was changed on its way".into());
    }
    Ok(())
}

/// Moves a daemon running there onto the new muster in place; none
/// running is fine.
fn reloaded(target: &str) {
    let _ = ssh(target)
        .arg(format!(
            "sh -c {}",
            quoted(&format!("\"{REMOTE_BIN}\" reload"))
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    eprintln!("Installed muster {VERSION} there.");
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
