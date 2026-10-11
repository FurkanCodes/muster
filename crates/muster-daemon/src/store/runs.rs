//! Runs in the store: each one's ticket as picked up, where it stands, its
//! machine, and the timeline of its transitions (`PRODUCT.md` §5: every
//! transition is recorded with its reason).

use muster_core::{PullRequest, Run, RunId, RunStage, RunTransition, Ticket, Timestamp};
use muster_state::run::RunMachine;
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::rows::id_text;
use super::{Result, Store, StoreError};

/// A run and its machine, as stored.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredRun {
    pub run: Run,
    pub machine: RunMachine,
}

/// What changes in a run with a transition.
#[derive(Clone, Debug)]
pub struct RunUpdate<'a> {
    pub stage: RunStage,
    pub reason: Option<&'a str>,
    pub pr: Option<&'a PullRequest>,
    /// The machine as saved at the transition.
    pub machine: &'a RunMachine,
    pub ended_at: Option<Timestamp>,
}

impl Store {
    /// Stores a run just picked up, with its machine.
    pub fn insert_run(&mut self, run: &Run, machine: &RunMachine) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runs (id, project_id, ticket, branch, stage, reason, pr, machine,
                               created_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id_text(run.id),
                id_text(run.project),
                json(&run.ticket)?,
                run.branch,
                json_text(&run.stage)?,
                run.reason,
                run.pr.as_ref().map(json).transpose()?,
                json(machine)?,
                run.created_at.as_millis(),
                run.ended_at.map(Timestamp::as_millis),
            ],
        )?;
        Ok(())
    }

    /// Stores a run's new state and the transition that led to it,
    /// together, before any of its work starts (`ARC-10`).
    pub fn record_run_transition(
        &mut self,
        run: RunId,
        transition: &RunTransition,
        update: &RunUpdate<'_>,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE runs SET stage = ?2, reason = ?3, pr = ?4, machine = ?5, ended_at = ?6
             WHERE id = ?1",
            params![
                id_text(run),
                json_text(&update.stage)?,
                update.reason,
                update.pr.map(json).transpose()?,
                json(update.machine)?,
                update.ended_at.map(Timestamp::as_millis),
            ],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound(run.to_string()));
        }
        tx.execute(
            "INSERT INTO run_transitions (run_id, from_stage, to_stage, reason, at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id_text(run),
                json_text(&transition.from)?,
                json_text(&transition.to)?,
                transition.reason,
                transition.at.as_millis(),
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Stores a run's machine and pull request when they change without a
    /// transition: a cost reported, a pull request found.
    pub fn save_run(
        &mut self,
        run: RunId,
        machine: &RunMachine,
        pr: Option<&PullRequest>,
    ) -> Result<()> {
        match self.conn.execute(
            "UPDATE runs SET machine = ?2, pr = ?3 WHERE id = ?1",
            params![id_text(run), json(machine)?, pr.map(json).transpose()?],
        )? {
            0 => Err(StoreError::NotFound(run.to_string())),
            _ => Ok(()),
        }
    }

    /// Every run, oldest first.
    pub fn runs(&self) -> Result<Vec<StoredRun>> {
        let mut statement = self.conn.prepare(
            "SELECT id, project_id, ticket, branch, stage, reason, pr, machine, created_at,
                    ended_at
             FROM runs ORDER BY created_at, id",
        )?;
        let rows = statement.query_map([], stored_run)?;
        rows.map(|row| row?).collect()
    }

    /// One run, if stored.
    pub fn run(&self, run: RunId) -> Result<Option<StoredRun>> {
        self.conn
            .query_row(
                "SELECT id, project_id, ticket, branch, stage, reason, pr, machine, created_at,
                        ended_at
                 FROM runs WHERE id = ?1",
                [id_text(run)],
                stored_run,
            )
            .optional()?
            .transpose()
    }

    /// A run's timeline, oldest first.
    pub fn run_history(&self, run: RunId) -> Result<Vec<RunTransition>> {
        let mut statement = self.conn.prepare(
            "SELECT from_stage, to_stage, reason, at FROM run_transitions
             WHERE run_id = ?1 ORDER BY seq",
        )?;
        let rows = statement.query_map([id_text(run)], |row| {
            let (from, to): (String, String) = (row.get(0)?, row.get(1)?);
            let (reason, at): (String, i64) = (row.get(2)?, row.get(3)?);
            Ok((|| {
                Ok(RunTransition {
                    from: parse_text(&from)?,
                    to: parse_text(&to)?,
                    reason,
                    at: Timestamp::from_millis(at),
                })
            })())
        })?;
        rows.map(|row| row?).collect()
    }
}

fn stored_run(row: &Row<'_>) -> rusqlite::Result<Result<StoredRun>> {
    let (id, project, ticket): (String, String, String) = (row.get(0)?, row.get(1)?, row.get(2)?);
    let (branch, stage, reason): (String, String, Option<String>) =
        (row.get(3)?, row.get(4)?, row.get(5)?);
    let (pr, machine): (Option<String>, String) = (row.get(6)?, row.get(7)?);
    let created_at = Timestamp::from_millis(row.get(8)?);
    let ended_at = row.get::<_, Option<i64>>(9)?.map(Timestamp::from_millis);
    Ok((|| {
        let machine: RunMachine = parse(&machine)?;
        let ticket: Ticket = parse(&ticket)?;
        Ok(StoredRun {
            run: Run {
                id: id.parse().map_err(|error: muster_core::ParseIdError| {
                    StoreError::Invalid(error.to_string())
                })?,
                project: project
                    .parse()
                    .map_err(|error: muster_core::ParseIdError| {
                        StoreError::Invalid(error.to_string())
                    })?,
                ticket,
                branch,
                stage: parse_text(&stage)?,
                attempt: machine.attempt(),
                max_attempts: machine.limits().attempts,
                reason,
                pr: pr.map(|pr| parse(&pr)).transpose()?,
                cost_micros: Some(machine.cost_micros()).filter(|cost| *cost > 0),
                created_at,
                ended_at,
            },
            machine,
        })
    })())
}

fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|error| StoreError::Invalid(error.to_string()))
}

/// An enum stored as its bare name, `"building"` as `building`.
fn json_text(value: &impl Serialize) -> Result<String> {
    Ok(json(value)?.trim_matches('"').to_owned())
}

fn parse<T: DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|error| StoreError::Invalid(format!("{text}: {error}")))
}

fn parse_text<T: DeserializeOwned>(text: &str) -> Result<T> {
    parse(&format!("\"{text}\""))
}
