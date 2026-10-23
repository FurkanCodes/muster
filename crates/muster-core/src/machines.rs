//! The person's machines (`PRODUCT.md` §6b): computers of their own that
//! they reach with `ssh`, where Muster runs as it does on theirs. Saved in
//! Muster's own `machines.toml` as a label and an ssh target each, never a
//! key or a password; ssh's own settings and keys stay ssh's. Also what a
//! machine says when asked about itself, read. Parsing does no IO
//! (`ARC-01`).

use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;

/// The file's name in Muster's own settings folder.
pub const FILE: &str = "machines.toml";

/// What `muster bridge` says on a line of its own, on another machine,
/// before any frame of the protocol: what a login printed before it is no
/// part of it.
pub const BRIDGE_READY: &str = "muster-bridge-ready:1";

/// Muster's link, in its runtime folder, to the ssh agent the person's ssh
/// forwarded last, named to every session as `SSH_AUTH_SOCK`.
pub const AGENT_LINK: &str = "ssh-agent.sock";

/// Every machine saved, by label.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machines {
    #[serde(default)]
    pub machines: BTreeMap<String, Machine>,
}

/// One machine.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    /// What `ssh` is given: `user@host`, a host, or a name in the person's
    /// `~/.ssh/config`.
    pub target: String,
}

/// Why the machines file, a label or a target does not read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum MachinesError {
    #[error("{FILE} does not read: {0}")]
    File(String),
    #[error(
        "{0:?} cannot be a machine's name: letters, digits, `-` and `_`, up to 32, a letter first"
    )]
    Label(String),
    #[error("{0:?} is not an ssh target: `user@host`, a host, or a name from your ~/.ssh/config")]
    Target(String),
}

impl Machines {
    /// The file's text read; an empty text is no machines.
    pub fn parse(text: &str) -> Result<Self, MachinesError> {
        let machines: Self = toml::from_str(text)
            .map_err(|error| MachinesError::File(error.message().to_owned()))?;
        for (label, machine) in &machines.machines {
            check_label(label)?;
            check_target(&machine.target)?;
        }
        Ok(machines)
    }

    /// The file's text. Labels and targets are kept to characters TOML
    /// writes as they are, so each is written whole.
    pub fn to_text(&self) -> String {
        let mut text = String::from(
            "# Your machines for `muster ssh`: a name and what ssh is given, nothing else.\n",
        );
        for (label, machine) in &self.machines {
            let _ = write!(
                text,
                "\n[machines.{label}]\ntarget = \"{}\"\n",
                machine.target
            );
        }
        text
    }

    /// Saves `target` as `label`, over one of that label.
    pub fn add(&mut self, label: &str, target: &str) -> Result<(), MachinesError> {
        check_label(label)?;
        check_target(target)?;
        self.machines.insert(
            label.to_owned(),
            Machine {
                target: target.to_owned(),
            },
        );
        Ok(())
    }

    /// What `ssh` is given for `name`: a saved machine's target, else
    /// `name` itself, as the person's ssh may know it.
    pub fn target(&self, name: &str) -> Result<String, MachinesError> {
        if let Some(machine) = self.machines.get(name) {
            return Ok(machine.target.clone());
        }
        check_target(name)?;
        Ok(name.to_owned())
    }
}

fn check_label(label: &str) -> Result<(), MachinesError> {
    let fine = label.len() <= 32
        && label
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic())
        && label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    if fine {
        Ok(())
    } else {
        Err(MachinesError::Label(label.to_owned()))
    }
}

/// A target ssh takes as a host, and never as an option or a command: no
/// leading `-`, no spaces, nothing a shell would read; and never a
/// password (`user:secret@host`): only ssh keeps those.
fn check_target(target: &str) -> Result<(), MachinesError> {
    let password = target
        .rsplit_once('@')
        .is_some_and(|(user, _)| user.contains(':'));
    let fine = !target.is_empty()
        && !target.starts_with('-')
        && !password
        && target
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "@.-_:[]%".contains(c));
    if fine {
        Ok(())
    } else {
        Err(MachinesError::Target(target.to_owned()))
    }
}

/// What a machine said of itself when asked (`muster ssh`'s probe): where
/// its `muster` is and which version, and its system.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Probe {
    /// Where `muster` is on it, if anywhere.
    pub muster: Option<String>,
    /// Its version (`0.5.6`), where it said one.
    pub version: Option<String>,
    /// Its system and processor as `uname -sm` says (`Linux x86_64`).
    pub system: Option<String>,
}

/// The lines the probe prints, marked, so a shell's greeting before them
/// is no answer.
pub const PROBE_MARK: &str = "muster-probe:";

impl Probe {
    /// The probe's output read: lines `muster-probe:path=…`,
    /// `muster-probe:version=muster 0.5.6`, `muster-probe:system=…`.
    pub fn read(output: &str) -> Self {
        let mut probe = Self::default();
        for line in output.lines() {
            let Some(rest) = line.trim().strip_prefix(PROBE_MARK) else {
                continue;
            };
            let Some((key, value)) = rest.split_once('=') else {
                continue;
            };
            let value = value.trim();
            if value.is_empty() {
                continue;
            }
            match key {
                "path" => probe.muster = Some(value.to_owned()),
                "version" => {
                    probe.version = value
                        .split_whitespace()
                        .last()
                        .map(|version| version.trim_start_matches('v').to_owned());
                }
                "system" => probe.system = Some(value.to_owned()),
                _ => {}
            }
        }
        probe
    }

    /// Whether it runs the build of `os` and `arch` (as Rust names them:
    /// `linux`, `macos`; `x86_64`, `aarch64`), so a binary of that build
    /// runs there as it is.
    pub fn same_build(&self, os: &str, arch: &str) -> bool {
        let Some(system) = &self.system else {
            return false;
        };
        let mut words = system.split_whitespace();
        let (Some(kernel), Some(machine)) = (words.next(), words.next()) else {
            return false;
        };
        let kernel = match kernel {
            "Linux" => "linux",
            "Darwin" => "macos",
            _ => return false,
        };
        let machine = match machine {
            "x86_64" | "amd64" => "x86_64",
            "aarch64" | "arm64" => "aarch64",
            _ => return false,
        };
        kernel == os && machine == arch
    }

    /// Whether its `muster` is missing or older than `version`.
    pub fn needs(&self, version: &str) -> bool {
        match &self.version {
            Some(theirs) => older(theirs, version),
            None => true,
        }
    }
}

/// Whether version `a` comes before `b`, number by number (`0.5.10` after
/// `0.5.9`); one that does not read is older.
pub fn older(a: &str, b: &str) -> bool {
    let numbers = |text: &str| -> Option<Vec<u64>> {
        text.split(['.', '-'])
            .take(3)
            .map(|part| part.parse().ok())
            .collect()
    };
    match (numbers(a), numbers(b)) {
        (Some(a), Some(b)) => a < b,
        (None, Some(_)) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machines_are_a_label_and_a_target_each_prd_06() {
        let mut machines = Machines::parse("").unwrap();
        machines.add("vm", "furkan@203.0.113.7").unwrap();
        machines.add("box", "build-box").unwrap();
        let again = Machines::parse(&machines.to_text()).unwrap();
        assert_eq!(again, machines);
        assert_eq!(again.target("vm").unwrap(), "furkan@203.0.113.7");
        assert_eq!(again.target("other-host").unwrap(), "other-host");
        assert!(matches!(
            machines.add("9lives", "x"),
            Err(MachinesError::Label(_))
        ));
        machines.add("v6", "you@[2001:db8::1]").unwrap();
        for target in ["-oProxyCommand=sh", "a b", "x;rm", "", "you:secret@host"] {
            assert!(machines.add("vm", target).is_err(), "{target:?}");
            assert!(machines.target(target).is_err(), "{target:?}");
        }
        assert!(Machines::parse("[machines.vm]\ntarget = \"x\"\nkey = \"secret\"\n").is_err());
    }

    #[test]
    fn a_probe_is_read_past_a_shell_s_greeting() {
        let probe = Probe::read(
            "Welcome to Ubuntu\nmuster-probe:path=/home/f/.local/bin/muster\n\
             muster-probe:version=muster 0.5.6\nmuster-probe:system=Linux x86_64\n",
        );
        assert_eq!(probe.muster.as_deref(), Some("/home/f/.local/bin/muster"));
        assert_eq!(probe.version.as_deref(), Some("0.5.6"));
        assert!(probe.same_build("linux", "x86_64"));
        assert!(!probe.same_build("macos", "aarch64"));
        assert!(!probe.needs("0.5.6"));
        assert!(probe.needs("0.5.10"));
        let nothing = Probe::read("muster-probe:path=\nmuster-probe:system=Darwin arm64\n");
        assert_eq!(nothing.muster, None);
        assert!(nothing.needs("0.1.0"));
        assert!(nothing.same_build("macos", "aarch64"));
    }

    #[test]
    fn versions_compare_number_by_number() {
        assert!(older("0.5.9", "0.5.10"));
        assert!(!older("0.5.10", "0.5.9"));
        assert!(!older("0.5.6", "0.5.6"));
        assert!(older("dev", "0.5.6"));
    }
}
