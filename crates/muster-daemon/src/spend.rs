//! What each session's agent has used (`PRD-12`), read from the agent's
//! own records, which Muster only reads (`PRD-13`): Claude Code's
//! transcripts, Codex's rollouts and Gemini CLI's chat recordings. Every
//! few seconds the reader asks the Supervisor which sessions to read, reads
//! what each record has gained since, off the Supervisor's loop, and hands
//! back the totals that changed. `OpenCode`'s plugin reports its own, cost
//! included (`harness::opencode`); Copilot CLI writes its usage only as it
//! exits, so its sessions say none.
//!
//! Totals are kept nowhere: after a restart each record is read again from
//! its start.

mod claude;
mod codex;
mod gemini;
mod tail;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use muster_core::{SessionId, Spend};

use crate::supervisor::SupervisorHandle;

/// A session to read: its harness and the agent's own id for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Target {
    pub(crate) session: SessionId,
    pub(crate) harness: String,
    pub(crate) agent_session: String,
}

/// Whose records Muster can read.
pub(crate) fn readable(harness: &str) -> bool {
    matches!(harness, "claude" | "review" | "codex" | "gemini")
}

/// Where the agents keep their records.
#[derive(Clone, Debug, Default)]
pub(crate) struct Places {
    /// Claude Code's `projects`: `$CLAUDE_CONFIG_DIR/projects`, or
    /// `~/.claude/projects`.
    pub(crate) claude_projects: Option<PathBuf>,
    /// Codex's `sessions`: `$CODEX_HOME/sessions`, or `~/.codex/sessions`.
    pub(crate) codex_sessions: Option<PathBuf>,
    /// Gemini CLI's `tmp`: `~/.gemini/tmp`.
    pub(crate) gemini_tmp: Option<PathBuf>,
}

impl Places {
    /// As the daemon's environment says, which sessions' agents share.
    pub(crate) fn from_env() -> Self {
        let var = |key| std::env::var_os(key).filter(|value| !value.is_empty());
        let home = var("HOME").map(PathBuf::from);
        Self {
            claude_projects: crate::harness::transcript::projects_dir(),
            codex_sessions: var("CODEX_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|home| home.join(".codex")))
                .map(|codex| codex.join("sessions")),
            gemini_tmp: home.map(|home| home.join(".gemini").join("tmp")),
        }
    }
}

#[derive(Debug)]
enum Reader {
    Claude(claude::Claude),
    Codex(codex::Codex),
    Gemini(gemini::Gemini),
}

/// Every session's reader, kept from one read to the next.
#[derive(Debug)]
pub(crate) struct Readers {
    places: Places,
    open: HashMap<SessionId, (String, Reader)>,
    told: HashMap<SessionId, u64>,
}

impl Readers {
    pub(crate) fn new(places: Places) -> Self {
        Self {
            places,
            open: HashMap::new(),
            told: HashMap::new(),
        }
    }

    /// Reads each target's records, and says the totals that changed since
    /// they were last told. Sessions no longer targeted are let go.
    pub(crate) fn read(&mut self, targets: &[Target], now: Instant) -> Vec<(SessionId, Spend)> {
        self.open
            .retain(|session, _| targets.iter().any(|target| target.session == *session));
        self.told
            .retain(|session, _| self.open.contains_key(session));
        let mut changed = Vec::new();
        for target in targets {
            let fresh = || match target.harness.as_str() {
                "claude" | "review" => Some(Reader::Claude(claude::Claude::default())),
                "codex" => Some(Reader::Codex(codex::Codex::default())),
                "gemini" => Some(Reader::Gemini(gemini::Gemini::default())),
                _ => None,
            };
            // A new agent session, as after a resume that could not, has
            // records of its own.
            if self
                .open
                .get(&target.session)
                .is_none_or(|(agent_session, _)| *agent_session != target.agent_session)
            {
                let Some(reader) = fresh() else { continue };
                self.open
                    .insert(target.session, (target.agent_session.clone(), reader));
            }
            let Some((_, reader)) = self.open.get_mut(&target.session) else {
                continue;
            };
            let id = target.agent_session.as_str();
            let tokens = match reader {
                Reader::Claude(claude) => self
                    .places
                    .claude_projects
                    .as_deref()
                    .and_then(|projects| claude.read(projects, id)),
                Reader::Codex(codex) => self
                    .places
                    .codex_sessions
                    .as_deref()
                    .and_then(|sessions| codex.read(sessions, id, now)),
                Reader::Gemini(gemini) => self
                    .places
                    .gemini_tmp
                    .as_deref()
                    .and_then(|tmp| gemini.read(tmp, id, now)),
            };
            if let Some(tokens) = tokens
                && self.told.insert(target.session, tokens) != Some(tokens)
            {
                changed.push((
                    target.session,
                    Spend {
                        tokens,
                        cost_micros: None,
                    },
                ));
            }
        }
        changed
    }
}

/// Reads every `interval` until the Supervisor stops.
pub(crate) async fn run(supervisor: SupervisorHandle, interval: Duration, places: Places) {
    let mut readers = Some(Readers::new(places));
    let mut ticks = tokio::time::interval(interval);
    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        ticks.tick().await;
        let Some(targets) = supervisor.spend_targets().await else {
            return;
        };
        let Some(mut taken) = readers.take() else {
            return;
        };
        let read = tokio::task::spawn_blocking(move || {
            let changed = taken.read(&targets, crate::clock::monotonic());
            (taken, changed)
        })
        .await;
        let Ok((returned, changed)) = read else {
            tracing::error!("reading sessions' spend failed");
            return;
        };
        readers = Some(returned);
        if !changed.is_empty() && !supervisor.record_spends(changed).await {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(session: SessionId, harness: &str, agent_session: &str) -> Target {
        Target {
            session,
            harness: harness.to_owned(),
            agent_session: agent_session.to_owned(),
        }
    }

    #[test]
    fn each_agent_s_records_are_read_and_only_changes_told() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("claude/projects/-w-app");
        std::fs::create_dir_all(&projects).unwrap();
        std::fs::write(
            projects.join("c1.jsonl"),
            claude::tests::assistant("m1", 1, 0, 9) + "\n",
        )
        .unwrap();
        let day = dir.path().join("codex/sessions/2026/10/15");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join("rollout-2026-10-15T09-00-00-t1.jsonl"),
            codex::tests::token_count(Some(700)) + "\n",
        )
        .unwrap();
        let mut readers = Readers::new(Places {
            claude_projects: Some(dir.path().join("claude/projects")),
            codex_sessions: Some(dir.path().join("codex/sessions")),
            gemini_tmp: Some(dir.path().join("gemini/tmp")),
        });
        let (claude, codex, gemini, shell) = (
            SessionId::new(),
            SessionId::new(),
            SessionId::new(),
            SessionId::new(),
        );
        let targets = [
            target(claude, "claude", "c1"),
            target(codex, "codex", "t1"),
            target(gemini, "gemini", "dea9457f-1315-439d-b94b-3ca4c8293f70"),
            target(shell, "shell", "x"),
        ];
        let spend = |tokens| Spend {
            tokens,
            cost_micros: None,
        };
        let now = crate::clock::monotonic();
        assert_eq!(
            readers.read(&targets, now),
            [(claude, spend(110)), (codex, spend(700))]
        );
        assert!(readers.read(&targets, now).is_empty(), "nothing changed");
        // Resumed as a new conversation: its own transcript.
        std::fs::write(
            projects.join("c2.jsonl"),
            claude::tests::assistant("m7", 5, 0, 15) + "\n",
        )
        .unwrap();
        assert_eq!(
            readers.read(&[target(claude, "claude", "c2")], now),
            [(claude, spend(120))]
        );
        assert!(readers.open.len() == 1, "the others let go");
        assert!(!readable("shell") && !readable("copilot") && readable("review"));
    }

    /// `MUSTER_LIVE_SPEND=claude:<session id>` (or `codex:<thread id>`,
    /// `gemini:<session id>`): what the person's own records say, totals
    /// only.
    #[test]
    #[ignore = "reads the person's own agents' records"]
    fn live_spend_of_the_person_s_records() {
        let Ok(wanted) = std::env::var("MUSTER_LIVE_SPEND") else {
            return;
        };
        let (harness, id) = wanted.split_once(':').unwrap();
        let session = SessionId::new();
        let mut readers = Readers::new(Places::from_env());
        let read = readers.read(&[target(session, harness, id)], crate::clock::monotonic());
        println!("{harness}: {read:?}");
        assert_eq!(read.len(), 1, "its records were found and read");
    }
}
