//! A project's settings for runs and actions (`ARCHITECTURE.md` §12):
//! `muster.toml` in the repository, with the person's local overrides
//! merged over it key by key. Hands-on sessions need none of it.
//!
//! Read strictly: an unknown key is an error, not ignored, since a
//! misspelt `comands` would leave runs with no checks, and nothing
//! unverified may reach a person (`PRD-02`). Parsing does no IO; the
//! daemon reads the files.

use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

pub mod agents;
pub mod edit;
pub mod preferences;
mod services;

pub use services::{HostKind, PullRequests, Tickets, TrackerKind};

/// The smallest memory reservation accepted: below it no agent runs.
const MIN_MEMORY: u64 = 512 * 1024 * 1024;

/// What a project asks of runs.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectConfig {
    /// The branch runs start from and pull requests target; `None` for the
    /// repository's default branch.
    pub base_branch: Option<String>,
    /// The image recipe, relative to the repository; `None` for Muster's
    /// base image.
    pub dockerfile: Option<PathBuf>,
    /// A run's memory reservation and its container's hard limit.
    pub memory_bytes: u64,
    /// The container's CPU share, in thousandths of a CPU.
    pub millicpus: u32,
    pub checks: Checks,
    pub agents: Agents,
    pub limits: RunLimits,
    pub intake: Intake,
    /// Where its tickets are, if not where its git remote says.
    pub tickets: Tickets,
    /// Where its pull requests are opened, if not where its remote says.
    pub pull_requests: PullRequests,
    /// How Muster names the branches it makes.
    pub branches: Branches,
}

/// How Muster names the branches it makes for the person's work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Branches {
    /// Whether they start with `muster/` (`[branches] muster_prefix`): a
    /// ticket's `muster/issue-42-fix-the-login`, or without,
    /// `issue-42-fix-the-login`.
    pub muster_prefix: bool,
}

impl Default for Branches {
    fn default() -> Self {
        Self {
            muster_prefix: true,
        }
    }
}

/// The commands that decide whether work is good enough to show a person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checks {
    pub commands: Vec<String>,
    pub timeout: Duration,
}

/// Which harnesses build and review, and what they are told to read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Agents {
    pub builder: String,
    pub reviewer: String,
    /// Files in the repository with the project's rules, for both.
    pub instructions: Vec<PathBuf>,
}

/// What one run may spend (`PRD-12`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RunLimits {
    pub attempts: u32,
    pub run_timeout: Duration,
    pub max_cost_usd: Option<f64>,
}

/// Which tickets runs start on by themselves (`PRD-07`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intake {
    /// The label that starts a run, if any does.
    pub label: Option<String>,
    /// Who may apply it.
    pub allowed_accounts: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            base_branch: None,
            dockerfile: None,
            memory_bytes: 4 * 1024 * 1024 * 1024,
            millicpus: 2_000,
            checks: Checks {
                commands: Vec::new(),
                timeout: Duration::from_mins(30),
            },
            agents: Agents {
                builder: "claude".to_owned(),
                reviewer: "claude".to_owned(),
                instructions: Vec::new(),
            },
            limits: RunLimits {
                attempts: 3,
                run_timeout: Duration::from_mins(90),
                max_cost_usd: None,
            },
            intake: Intake {
                label: None,
                allowed_accounts: Vec::new(),
            },
            tickets: Tickets::default(),
            pull_requests: PullRequests::default(),
            branches: Branches::default(),
        }
    }
}

/// Runs need checks, and the settings have none.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("runs need checks: add `[checks] commands = [\"…\"]` to muster.toml")]
pub struct NoChecks;

/// Why the settings could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("{file} is not valid TOML: {message}")]
    Syntax { file: String, message: String },
    #[error("{file}: {message}")]
    Invalid { file: String, message: String },
    /// A coding agent the person added does not read: the error names its
    /// key.
    #[error("{file}: {error}")]
    Agent {
        file: String,
        error: agents::AgentError,
    },
}

impl ProjectConfig {
    /// Reads the repository's `muster.toml` and the person's overrides,
    /// either of which may be absent. `names` say which files they are, for
    /// messages.
    pub fn parse(
        repository: Option<&str>,
        local: Option<&str>,
        names: (&str, &str),
    ) -> Result<Self, ConfigError> {
        let mut merged = toml::Table::new();
        for (text, name) in [(repository, names.0), (local, names.1)] {
            let Some(text) = text else { continue };
            let table: toml::Table = toml::from_str(text).map_err(|error| ConfigError::Syntax {
                file: name.to_owned(),
                message: error.message().to_owned(),
            })?;
            merge(&mut merged, table);
        }
        let file = match (repository, local) {
            (Some(_), Some(_)) => format!("{} with {}", names.0, names.1),
            (None, Some(_)) => names.1.to_owned(),
            _ => names.0.to_owned(),
        };
        let invalid = |message: String| ConfigError::Invalid {
            file: file.clone(),
            message,
        };
        let raw: Raw = toml::Value::Table(merged)
            .try_into()
            .map_err(|error: toml::de::Error| invalid(error.message().to_owned()))?;
        raw.validate().map_err(invalid)
    }

    /// Whether runs can use these settings: they need checks, since a run
    /// shows the person only what passed them (`PRD-02`).
    pub fn ready_for_runs(&self) -> Result<(), NoChecks> {
        if self.checks.commands.is_empty() {
            return Err(NoChecks);
        }
        Ok(())
    }
}

/// Merges `over` into `base`: tables key by key, anything else replaced.
fn merge(base: &mut toml::Table, over: toml::Table) {
    for (key, value) in over {
        match (base.get_mut(&key), value) {
            (Some(toml::Value::Table(inner)), toml::Value::Table(more)) => merge(inner, more),
            (_, value) => {
                base.insert(key, value);
            }
        }
    }
}

// The file as written.

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct Raw {
    project: RawProject,
    checks: RawChecks,
    agents: RawAgents,
    limits: RawLimits,
    intake: RawIntake,
    tickets: services::RawTickets,
    pull_requests: services::RawPullRequests,
    branches: RawBranches,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawBranches {
    muster_prefix: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawProject {
    base_branch: Option<String>,
    dockerfile: Option<String>,
    memory: Option<String>,
    cpus: Option<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawChecks {
    commands: Vec<String>,
    timeout: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawAgents {
    builder: Option<String>,
    reviewer: Option<String>,
    instructions: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawLimits {
    attempts: Option<i64>,
    run_timeout: Option<String>,
    max_cost_usd: Option<toml::Value>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
struct RawIntake {
    label: Option<String>,
    allowed_accounts: Vec<String>,
}

impl Raw {
    fn validate(self) -> Result<ProjectConfig, String> {
        let defaults = ProjectConfig::default();
        let base_branch = match self.project.base_branch {
            Some(branch) if branch.trim().is_empty() || branch.contains(char::is_control) => {
                return Err("project.base_branch must name a branch".to_owned());
            }
            other => other,
        };
        let dockerfile = self
            .project
            .dockerfile
            .map(|path| inside_repository("project.dockerfile", &path))
            .transpose()?;
        let memory_bytes = match self.project.memory {
            None => defaults.memory_bytes,
            Some(text) => memory(&text)?,
        };
        let millicpus = match self.project.cpus {
            None => defaults.millicpus,
            Some(value) => millicpus(&value)?,
        };
        Ok(ProjectConfig {
            base_branch,
            dockerfile,
            memory_bytes,
            millicpus,
            checks: self.checks.validate(&defaults.checks)?,
            agents: self.agents.validate(defaults.agents)?,
            limits: self.limits.validate(&defaults.limits)?,
            intake: self.intake.validate()?,
            tickets: self.tickets.validate()?,
            pull_requests: self.pull_requests.validate()?,
            branches: Branches {
                muster_prefix: self
                    .branches
                    .muster_prefix
                    .unwrap_or(defaults.branches.muster_prefix),
            },
        })
    }
}

/// `project.memory`, at least what an agent needs.
fn memory(text: &str) -> Result<u64, String> {
    let bytes = parse_size(text)
        .ok_or_else(|| format!("project.memory {text:?} is not a size like \"6g\" or \"512m\""))?;
    if bytes < MIN_MEMORY {
        return Err(format!(
            "project.memory {text:?} is below the 512m an agent needs"
        ));
    }
    Ok(bytes)
}

/// `project.cpus`, in thousandths of a CPU.
fn millicpus(value: &toml::Value) -> Result<u32, String> {
    let cpus = value
        .as_float()
        .or_else(|| value.as_integer().map(crate::num::fraction_signed))
        .filter(|cpus| (0.1..=256.0).contains(cpus))
        .ok_or_else(|| {
            format!(
                "project.cpus {} must be a number from 0.1 to 256",
                shown(value)
            )
        })?;
    Ok(crate::num::round_u32(cpus * 1000.0))
}

impl RawChecks {
    fn validate(self, defaults: &Checks) -> Result<Checks, String> {
        let commands: Vec<String> = self.commands.iter().map(|c| c.trim().to_owned()).collect();
        if commands.iter().any(String::is_empty) {
            return Err("checks.commands has an empty command".to_owned());
        }
        Ok(Checks {
            commands,
            timeout: duration("checks.timeout", self.timeout, defaults.timeout)?,
        })
    }
}

impl RawAgents {
    fn validate(self, defaults: Agents) -> Result<Agents, String> {
        let harness = |key: &str, value: Option<String>, default: String| match value {
            Some(id) if id.trim().is_empty() => Err(format!("agents.{key} must name a harness")),
            Some(id) => Ok(id.trim().to_owned()),
            None => Ok(default),
        };
        Ok(Agents {
            builder: harness("builder", self.builder, defaults.builder)?,
            reviewer: harness("reviewer", self.reviewer, defaults.reviewer)?,
            instructions: self
                .instructions
                .iter()
                .map(|path| inside_repository("agents.instructions", path))
                .collect::<Result<_, _>>()?,
        })
    }
}

impl RawLimits {
    fn validate(self, defaults: &RunLimits) -> Result<RunLimits, String> {
        let attempts = match self.attempts {
            None => defaults.attempts,
            Some(n @ 1..=20) => u32::try_from(n).unwrap_or(1),
            Some(n) => return Err(format!("limits.attempts {n} must be from 1 to 20")),
        };
        let max_cost_usd = match self.max_cost_usd {
            None => None,
            Some(value) => Some(
                value
                    .as_float()
                    .or_else(|| value.as_integer().map(crate::num::fraction_signed))
                    .filter(|usd| *usd > 0.0 && usd.is_finite())
                    .ok_or_else(|| {
                        format!(
                            "limits.max_cost_usd {} must be a positive number",
                            shown(&value)
                        )
                    })?,
            ),
        };
        Ok(RunLimits {
            attempts,
            run_timeout: duration("limits.run_timeout", self.run_timeout, defaults.run_timeout)?,
            max_cost_usd,
        })
    }
}

impl RawIntake {
    fn validate(self) -> Result<Intake, String> {
        let label = self.label.map(|label| label.trim().to_owned());
        if label.as_deref() == Some("") {
            return Err("intake.label must not be empty".to_owned());
        }
        let allowed_accounts: Vec<String> = self
            .allowed_accounts
            .iter()
            .map(|account| account.trim().to_owned())
            .filter(|account| !account.is_empty())
            .collect();
        if label.is_some() && allowed_accounts.is_empty() {
            return Err(
                "intake.label is set but intake.allowed_accounts is empty: anyone can label an \
                 issue, so say whose label starts a run"
                    .to_owned(),
            );
        }
        Ok(Intake {
            label,
            allowed_accounts,
        })
    }
}

/// A value as written, for messages.
fn shown(value: &toml::Value) -> String {
    match value {
        toml::Value::String(text) => format!("{text:?}"),
        toml::Value::Integer(number) => number.to_string(),
        toml::Value::Float(number) => number.to_string(),
        toml::Value::Boolean(flag) => flag.to_string(),
        toml::Value::Datetime(_) => "(a date)".to_owned(),
        toml::Value::Array(_) => "(a list)".to_owned(),
        toml::Value::Table(_) => "(a table)".to_owned(),
    }
}

/// A path that stays inside the repository: relative, and never climbing
/// out of it.
fn inside_repository(key: &str, path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path.trim());
    let escapes = path.components().any(|part| {
        matches!(
            part,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    });
    if path.as_os_str().is_empty() || escapes {
        return Err(format!(
            "{key} {:?} must be a path inside the repository",
            path.display().to_string()
        ));
    }
    Ok(path.to_owned())
}

fn duration(key: &str, text: Option<String>, default: Duration) -> Result<Duration, String> {
    match text {
        None => Ok(default),
        Some(text) => parse_duration(&text)
            .filter(|duration| !duration.is_zero())
            .ok_or_else(|| format!("{key} {text:?} is not a duration like \"30m\" or \"1h30m\"")),
    }
}

/// `"90m"`, `"1h30m"`, `"45s"`: whole numbers each followed by `h`, `m`
/// or `s`.
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let mut total: u64 = 0;
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let unit = match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return None,
        };
        let value: u64 = digits.parse().ok()?;
        total = total.checked_add(value.checked_mul(unit)?)?;
        digits.clear();
    }
    digits.is_empty().then(|| Duration::from_secs(total))
}

/// `"6g"`, `"512m"`, `"1.5g"`, or a plain byte count.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim().to_ascii_lowercase();
    let text = text.strip_suffix('b').unwrap_or(&text);
    let (number, unit) = match text.chars().last()? {
        'k' => (&text[..text.len() - 1], 1u64 << 10),
        'm' => (&text[..text.len() - 1], 1 << 20),
        'g' => (&text[..text.len() - 1], 1 << 30),
        't' => (&text[..text.len() - 1], 1 << 40),
        _ => (text, 1),
    };
    let value: f64 = number.trim().parse().ok()?;
    let bytes = value * crate::num::fraction(unit);
    (value.is_finite() && value > 0.0 && bytes < crate::num::fraction(u64::MAX))
        .then(|| crate::num::round_u64(bytes))
}

#[cfg(test)]
mod tests;
