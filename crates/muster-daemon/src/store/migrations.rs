//! Schema migrations, applied in order and tracked by SQLite's `user_version`.
//!
//! Never edit a migration that has shipped; add a new one. Each runs in its own
//! transaction together with the version bump.

use rusqlite::Connection;

use super::StoreError;

/// Every migration, oldest first. The schema version is the number applied.
pub(super) const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5, V6];

/// Projects, workspaces, sessions, their status history and resource samples.
const V1: &str = r"
CREATE TABLE projects (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    root        TEXT NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE workspaces (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    kind        TEXT NOT NULL,
    path        TEXT NOT NULL,
    branch      TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    removed_at  INTEGER
) STRICT;

CREATE INDEX workspaces_by_project ON workspaces (project_id);

CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT NOT NULL REFERENCES workspaces (id) ON DELETE CASCADE,
    harness       TEXT NOT NULL,
    title         TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (json_valid(status)),
    unseen        INTEGER NOT NULL CHECK (unseen IN (0, 1)),
    resume_id     TEXT,
    created_at    INTEGER NOT NULL,
    ended_at      INTEGER
) STRICT;

CREATE INDEX sessions_by_workspace ON sessions (workspace_id);

CREATE TABLE status_changes (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    status      TEXT NOT NULL CHECK (json_valid(status)),
    unseen      INTEGER NOT NULL CHECK (unseen IN (0, 1)),
    at          INTEGER NOT NULL
) STRICT;

CREATE INDEX status_changes_by_session ON status_changes (session_id, seq);

CREATE TABLE resource_samples (
    session_id    TEXT NOT NULL REFERENCES sessions (id) ON DELETE CASCADE,
    at            INTEGER NOT NULL,
    cpu_permille  INTEGER NOT NULL CHECK (cpu_permille >= 0),
    memory_bytes  INTEGER NOT NULL CHECK (memory_bytes >= 0),
    PRIMARY KEY (session_id, at)
) STRICT, WITHOUT ROWID;
";

/// Sessions the person removed from the list. Like removed workspaces,
/// they keep their history.
const V2: &str = r"
ALTER TABLE sessions ADD COLUMN removed_at INTEGER;
";

/// Stored statuses may now say a session waits on the person before its
/// harness starts (`"reason":"startup"`), which an older Muster cannot read;
/// the version alone changes, so such a Muster refuses the database plainly
/// instead of failing on one session.
const V3: &str = "";

/// Runs: each one's ticket as picked up, where it stands, and its machine,
/// with the timeline of its transitions and their reasons.
const V4: &str = r"
CREATE TABLE runs (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    ticket      TEXT NOT NULL CHECK (json_valid(ticket)),
    branch      TEXT NOT NULL,
    stage       TEXT NOT NULL,
    reason      TEXT,
    pr          TEXT CHECK (pr IS NULL OR json_valid(pr)),
    machine     TEXT NOT NULL CHECK (json_valid(machine)),
    -- The building agent's harness session, to resume it or take it over.
    agent_session  TEXT,
    container   TEXT,
    created_at  INTEGER NOT NULL,
    ended_at    INTEGER
) STRICT;

CREATE INDEX runs_by_project ON runs (project_id);

CREATE TABLE run_transitions (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id      TEXT NOT NULL REFERENCES runs (id) ON DELETE CASCADE,
    from_stage  TEXT NOT NULL,
    to_stage    TEXT NOT NULL,
    reason      TEXT NOT NULL,
    at          INTEGER NOT NULL
) STRICT;

CREATE INDEX run_transitions_by_run ON run_transitions (run_id, seq);
";

/// Runs' containers as workspaces: the run each belongs to. Their path is
/// empty; their files are in the container only.
const V5: &str = r"
ALTER TABLE workspaces ADD COLUMN run_id TEXT REFERENCES runs (id) ON DELETE CASCADE;
";

/// Stacked work: the branch a workspace's branch is stacked on, and the
/// commit of it its work follows (`muster_core::stacks`).
const V6: &str = r"
ALTER TABLE workspaces ADD COLUMN stacked_on TEXT;
ALTER TABLE workspaces ADD COLUMN stacked_at TEXT;
";

/// The schema version this build creates.
pub(super) fn latest_version() -> u32 {
    u32::try_from(MIGRATIONS.len()).unwrap_or(u32::MAX)
}

pub(super) fn schema_version(conn: &Connection) -> Result<u32, StoreError> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

/// Brings the database up to the latest schema. Refuses a database written by
/// a newer Muster rather than guessing at it.
pub(super) fn migrate(conn: &mut Connection) -> Result<(), StoreError> {
    let current = schema_version(conn)?;
    let known = latest_version();
    if current > known {
        return Err(StoreError::NewerSchema {
            found: current,
            known,
        });
    }
    for (version, migration) in (1..=known).zip(MIGRATIONS).skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
    }
    Ok(())
}
