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
                timeout: Duration::from_secs(30 * 60),
            },
            agents: Agents {
                builder: "claude".to_owned(),
                reviewer: "claude".to_owned(),
                instructions: Vec::new(),
            },
            limits: RunLimits {
                attempts: 3,
                run_timeout: Duration::from_secs(90 * 60),
                max_cost_usd: None,
            },
            intake: Intake {
                label: None,
                allowed_accounts: Vec::new(),
            },
            tickets: Tickets::default(),
            pull_requests: PullRequests::default(),
        }
    }
}

/// Why the settings could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("{file} is not valid TOML: {message}")]
    Syntax { file: String, message: String },
    #[error("{file}: {message}")]
    Invalid { file: String, message: String },
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
    pub fn ready_for_runs(&self) -> Result<(), String> {
        if self.checks.commands.is_empty() {
            return Err(
                "runs need checks: add `[checks] commands = [\"…\"]` to muster.toml".to_owned(),
            );
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
            Some(text) => {
                let bytes = parse_size(&text).ok_or_else(|| {
                    format!("project.memory {text:?} is not a size like \"6g\" or \"512m\"")
                })?;
                if bytes < MIN_MEMORY {
                    return Err(format!(
                        "project.memory {text:?} is below the 512m an agent needs"
                    ));
                }
                bytes
            }
        };
        let millicpus = match self.project.cpus {
            None => defaults.millicpus,
            Some(value) => {
                let cpus = value
                    .as_float()
                    .or_else(|| value.as_integer().map(|n| n as f64))
                    .filter(|cpus| (0.1..=256.0).contains(cpus))
                    .ok_or_else(|| {
                        format!(
                            "project.cpus {} must be a number from 0.1 to 256",
                            shown(&value)
                        )
                    })?;
                (cpus * 1000.0).round() as u32
            }
        };

        let commands: Vec<String> = self
            .checks
            .commands
            .iter()
            .map(|c| c.trim().to_owned())
            .collect();
        if commands.iter().any(String::is_empty) {
            return Err("checks.commands has an empty command".to_owned());
        }
        let checks = Checks {
            commands,
            timeout: duration(
                "checks.timeout",
                self.checks.timeout,
                defaults.checks.timeout,
            )?,
        };

        let harness = |key: &str, value: Option<String>, default: String| match value {
            Some(id) if id.trim().is_empty() => Err(format!("agents.{key} must name a harness")),
            Some(id) => Ok(id.trim().to_owned()),
            None => Ok(default),
        };
        let agents = Agents {
            builder: harness("builder", self.agents.builder, defaults.agents.builder)?,
            reviewer: harness("reviewer", self.agents.reviewer, defaults.agents.reviewer)?,
            instructions: self
                .agents
                .instructions
                .iter()
                .map(|path| inside_repository("agents.instructions", path))
                .collect::<Result<_, _>>()?,
        };

        let attempts = match self.limits.attempts {
            None => defaults.limits.attempts,
            Some(n @ 1..=20) => u32::try_from(n).unwrap_or(1),
            Some(n) => return Err(format!("limits.attempts {n} must be from 1 to 20")),
        };
        let max_cost_usd = match self.limits.max_cost_usd {
            None => None,
            Some(value) => Some(
                value
                    .as_float()
                    .or_else(|| value.as_integer().map(|n| n as f64))
                    .filter(|usd| *usd > 0.0 && usd.is_finite())
                    .ok_or_else(|| {
                        format!(
                            "limits.max_cost_usd {} must be a positive number",
                            shown(&value)
                        )
                    })?,
            ),
        };
        let limits = RunLimits {
            attempts,
            run_timeout: duration(
                "limits.run_timeout",
                self.limits.run_timeout,
                defaults.limits.run_timeout,
            )?,
            max_cost_usd,
        };

        let label = self.intake.label.map(|label| label.trim().to_owned());
        if label.as_deref() == Some("") {
            return Err("intake.label must not be empty".to_owned());
        }
        let allowed_accounts: Vec<String> = self
            .intake
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
        Ok(ProjectConfig {
            base_branch,
            dockerfile,
            memory_bytes,
            millicpus,
            checks,
            agents,
            limits,
            intake: Intake {
                label,
                allowed_accounts,
            },
            tickets: self.tickets.validate()?,
            pull_requests: self.pull_requests.validate()?,
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
    let bytes = value * unit as f64;
    (value.is_finite() && value > 0.0 && bytes < u64::MAX as f64).then(|| bytes.round() as u64)
}

#[cfg(test)]
mod tests;
