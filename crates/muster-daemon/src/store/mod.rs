//! The SQLite store (`ARCHITECTURE.md` §4). Synchronous; the daemon reaches it
//! only through [`StoreHandle`], which runs it on its own thread.

mod handle;
mod migrations;
mod rows;
mod runs;

fn insert_workspace_row(conn: &Connection, workspace: &Workspace) -> Result<()> {
    let columns = rows::workspace_columns(workspace)?;
    conn.execute(
        "INSERT INTO workspaces (id, project_id, kind, path, branch, created_at, run_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            id_text(workspace.id),
            id_text(workspace.project),
            columns.kind,
            columns.path,
            columns.branch,
            workspace.created_at.as_millis(),
            columns.run,
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;

use std::path::Path;

use muster_core::protocol::Snapshot;
use muster_core::{Project, Session, SessionId, SessionStatus, Timestamp, Workspace, WorkspaceId};
use rusqlite::{Connection, OptionalExtension, params};

pub use handle::{StoreClosed, StoreHandle};
pub use rows::{ResourceSample, StatusChange};
pub use runs::{RunProgress, RunUpdate, StoredRun};

use rows::{id_text, path_text, status_json};

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(
        "the database is at schema version {found}, newer than this Muster knows ({known}); upgrade Muster"
    )]
    NewerSchema { found: u32, known: u32 },
    #[error("{0} already exists")]
    Conflict(String),
    #[error("{0} was not found")]
    NotFound(String),
    #[error("invalid value: {0}")]
    Invalid(String),
}

pub type Result<T, E = StoreError> = std::result::Result<T, E>;

/// The daemon's lasting state.
#[derive(Debug)]
pub struct Store {
    conn: Connection,
}

impl Store {
    /// Opens or creates the database at `path` and migrates it. The parent
    /// directory must exist.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::configure(conn)
    }

    /// A private database that lives as long as the store; for tests.
    pub fn open_in_memory() -> Result<Self> {
        Self::configure(Connection::open_in_memory()?)
    }

    fn configure(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        migrations::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// The schema version of the open database.
    pub fn schema_version(&self) -> Result<u32> {
        migrations::schema_version(&self.conn)
    }

    pub fn insert_project(&mut self, project: &Project) -> Result<()> {
        let root = path_text(&project.root)?;
        if self.project_by_root(&project.root)?.is_some() {
            return Err(StoreError::Conflict(format!("a project at {root}")));
        }
        self.conn.execute(
            "INSERT INTO projects (id, name, root, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                id_text(project.id),
                project.name,
                root,
                project.created_at.as_millis()
            ],
        )?;
        Ok(())
    }

    /// Stores a new project together with its own checkout's workspace, in
    /// one transaction.
    pub fn add_project(&mut self, project: &Project, checkout: &Workspace) -> Result<()> {
        let root = path_text(&project.root)?;
        if self.project_by_root(&project.root)?.is_some() {
            return Err(StoreError::Conflict(format!("a project at {root}")));
        }
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO projects (id, name, root, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                id_text(project.id),
                project.name,
                root,
                project.created_at.as_millis()
            ],
        )?;
        insert_workspace_row(&tx, checkout)?;
        tx.commit()?;
        Ok(())
    }

    /// Forgets a project and everything stored about it: its workspaces,
    /// their sessions and those sessions' history.
    pub fn delete_project(&mut self, project: muster_core::ProjectId) -> Result<()> {
        match self
            .conn
            .execute("DELETE FROM projects WHERE id = ?1", [id_text(project)])?
        {
            0 => Err(StoreError::NotFound(project.to_string())),
            _ => Ok(()),
        }
    }

    /// Records what a workspace now has checked out.
    pub fn update_branch(&mut self, workspace: WorkspaceId, branch: &str) -> Result<()> {
        match self.conn.execute(
            "UPDATE workspaces SET branch = ?2 WHERE id = ?1",
            params![id_text(workspace), branch],
        )? {
            0 => Err(StoreError::NotFound(workspace.to_string())),
            _ => Ok(()),
        }
    }

    pub fn project_by_root(&self, root: &Path) -> Result<Option<Project>> {
        self.conn
            .query_row(
                "SELECT id, name, root, created_at FROM projects WHERE root = ?1",
                [path_text(root)?],
                rows::project,
            )
            .optional()?
            .transpose()
    }

    pub fn insert_workspace(&mut self, workspace: &Workspace) -> Result<()> {
        insert_workspace_row(&self.conn, workspace)
    }

    /// Marks a workspace removed. Its sessions and their history are kept.
    pub fn remove_workspace(&mut self, workspace: WorkspaceId, at: Timestamp) -> Result<()> {
        match self.conn.execute(
            "UPDATE workspaces SET removed_at = ?2 WHERE id = ?1 AND removed_at IS NULL",
            params![id_text(workspace), at.as_millis()],
        )? {
            0 => Err(StoreError::NotFound(workspace.to_string())),
            _ => Ok(()),
        }
    }

    pub fn insert_session(&mut self, session: &Session) -> Result<()> {
        self.conn.execute(
            "INSERT INTO sessions
                (id, workspace_id, harness, title, status, unseen, resume_id, created_at, ended_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                id_text(session.id),
                id_text(session.workspace),
                session.harness,
                session.title,
                status_json(session.status)?,
                session.unseen,
                session.resume_id,
                session.created_at.as_millis(),
                session.ended_at.map(Timestamp::as_millis),
            ],
        )?;
        Ok(())
    }

    /// Stores a session's new status and appends it to its history, together.
    pub fn record_status(
        &mut self,
        session: SessionId,
        status: SessionStatus,
        unseen: bool,
        at: Timestamp,
    ) -> Result<()> {
        let status = status_json(status)?;
        let tx = self.conn.transaction()?;
        let updated = tx.execute(
            "UPDATE sessions SET status = ?2, unseen = ?3 WHERE id = ?1",
            params![id_text(session), status, unseen],
        )?;
        if updated == 0 {
            return Err(StoreError::NotFound(session.to_string()));
        }
        tx.execute(
            "INSERT INTO status_changes (session_id, status, unseen, at) VALUES (?1, ?2, ?3, ?4)",
            params![id_text(session), status, unseen, at.as_millis()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn save_resume_id(&mut self, session: SessionId, resume_id: &str) -> Result<()> {
        self.update_session(
            session,
            "UPDATE sessions SET resume_id = ?2 WHERE id = ?1",
            resume_id,
        )
    }

    /// Clears a session's end: its program was started again.
    pub fn reopen_session(&mut self, session: SessionId) -> Result<()> {
        match self.conn.execute(
            "UPDATE sessions SET ended_at = NULL WHERE id = ?1",
            [id_text(session)],
        )? {
            0 => Err(StoreError::NotFound(session.to_string())),
            _ => Ok(()),
        }
    }

    /// Marks a session removed from the list. Its history is kept.
    pub fn remove_session(&mut self, session: SessionId, at: Timestamp) -> Result<()> {
        match self.conn.execute(
            "UPDATE sessions SET removed_at = ?2 WHERE id = ?1 AND removed_at IS NULL",
            params![id_text(session), at.as_millis()],
        )? {
            0 => Err(StoreError::NotFound(session.to_string())),
            _ => Ok(()),
        }
    }

    pub fn end_session(&mut self, session: SessionId, at: Timestamp) -> Result<()> {
        self.update_session(
            session,
            "UPDATE sessions SET ended_at = ?2 WHERE id = ?1",
            at.as_millis(),
        )
    }

    fn update_session(
        &mut self,
        session: SessionId,
        sql: &str,
        value: impl rusqlite::ToSql,
    ) -> Result<()> {
        match self.conn.execute(sql, params![id_text(session), value])? {
            0 => Err(StoreError::NotFound(session.to_string())),
            _ => Ok(()),
        }
    }

    /// A session's status changes, oldest first.
    pub fn status_history(&self, session: SessionId) -> Result<Vec<StatusChange>> {
        let mut statement = self.conn.prepare(
            "SELECT status, unseen, at FROM status_changes WHERE session_id = ?1 ORDER BY seq",
        )?;
        let changes = statement.query_map([id_text(session)], rows::status_change)?;
        changes.map(|change| change?).collect()
    }

    pub fn record_sample(&mut self, sample: &ResourceSample) -> Result<()> {
        let memory = i64::try_from(sample.memory_bytes)
            .map_err(|_| StoreError::Invalid(format!("memory {}", sample.memory_bytes)))?;
        self.conn.execute(
            "INSERT OR REPLACE INTO resource_samples (session_id, at, cpu_permille, memory_bytes)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                id_text(sample.session),
                sample.at.as_millis(),
                sample.cpu_permille,
                memory
            ],
        )?;
        Ok(())
    }

    /// A session's samples taken at or after `since`, oldest first.
    pub fn samples(&self, session: SessionId, since: Timestamp) -> Result<Vec<ResourceSample>> {
        let mut statement = self.conn.prepare(
            "SELECT session_id, at, cpu_permille, memory_bytes FROM resource_samples
             WHERE session_id = ?1 AND at >= ?2 ORDER BY at",
        )?;
        let samples =
            statement.query_map(params![id_text(session), since.as_millis()], rows::sample)?;
        samples.map(|sample| sample?).collect()
    }

    /// Everything a client needs: all projects, and the workspaces still in
    /// use with the sessions not removed from them.
    pub fn snapshot(&self) -> Result<Snapshot> {
        let projects = self.collect(
            "SELECT id, name, root, created_at FROM projects ORDER BY created_at, id",
            rows::project,
        )?;
        let workspaces = self.collect(
            "SELECT id, project_id, kind, path, branch, created_at, run_id FROM workspaces
             WHERE removed_at IS NULL ORDER BY created_at, id",
            rows::workspace,
        )?;
        let sessions = self.collect(
            "SELECT s.id, s.workspace_id, s.harness, s.title, s.status, s.unseen, s.resume_id,
                    s.created_at, s.ended_at
             FROM sessions s JOIN workspaces w ON w.id = s.workspace_id
             WHERE w.removed_at IS NULL AND s.removed_at IS NULL
             ORDER BY s.created_at, s.id",
            rows::session,
        )?;
        let runs = self.runs()?.into_iter().map(|stored| stored.run).collect();
        Ok(Snapshot {
            projects,
            workspaces,
            sessions,
            runs,
            resources: None,
        })
    }

    fn collect<T>(
        &self,
        sql: &str,
        row: fn(&rusqlite::Row<'_>) -> rusqlite::Result<Result<T>>,
    ) -> Result<Vec<T>> {
        let mut statement = self.conn.prepare(sql)?;
        let items = statement.query_map([], row)?;
        items.map(|item| item?).collect()
    }
}
