//! Schema migrations, applied in order and tracked by SQLite's `user_version`.
//!
//! Never edit a migration that has shipped; add a new one. Each runs in its own
//! transaction together with the version bump.

use rusqlite::{Connection, OptionalExtension};

use super::StoreError;

/// Every migration, oldest first. The schema version is the number applied.
pub(super) const MIGRATIONS: &[&str] = &[V1, V2, V3, V4, V5, V6, V7, V8, V9, V10, V11];

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

/// Measurements are live state only. Their unused, unbounded history is
/// removed after `Store::open` has backed up the old schema (ARC-14, PRD-09).
const V7: &str = "DROP TABLE resource_samples;";

/// Plugins' panels: the chat each was opened beside, which a panel is shown
/// beside and told of again when it is started again (`PRD-15`).
const V8: &str = "ALTER TABLE sessions ADD COLUMN beside TEXT;";

/// The ticket a worktree was made for, so its agents stay the ticket's
/// whatever its branch is called later. Worktrees made before it have
/// theirs read once from their branch's name ([`tickets_from_branches`]).
const V9: &str = "ALTER TABLE workspaces ADD COLUMN ticket TEXT;";

/// The agent's session that started a session through the command line
/// (`PRD-01`), shown with it and counted against the person's limits.
const V10: &str = "ALTER TABLE sessions ADD COLUMN started_by TEXT;";

/// Free terminals, in no workspace (`ARC-33`), and the person's spaces
/// with what is in them. SQLite cannot let a column be empty in place, so
/// `sessions` is made again and its rows copied, with foreign keys off for
/// the copy ([`REBUILDS`]) so that dropping the old table removes no
/// session's history; they are checked before the change is kept.
const V11: &str = r"
CREATE TABLE sessions_v11 (
    id            TEXT PRIMARY KEY,
    workspace_id  TEXT REFERENCES workspaces (id) ON DELETE CASCADE,
    harness       TEXT NOT NULL,
    title         TEXT NOT NULL,
    status        TEXT NOT NULL CHECK (json_valid(status)),
    unseen        INTEGER NOT NULL CHECK (unseen IN (0, 1)),
    resume_id     TEXT,
    created_at    INTEGER NOT NULL,
    ended_at      INTEGER,
    removed_at    INTEGER,
    beside        TEXT,
    started_by    TEXT
) STRICT;

INSERT INTO sessions_v11
    (id, workspace_id, harness, title, status, unseen, resume_id, created_at, ended_at,
     removed_at, beside, started_by)
SELECT id, workspace_id, harness, title, status, unseen, resume_id, created_at, ended_at,
       removed_at, beside, started_by
FROM sessions;

DROP TABLE sessions;
ALTER TABLE sessions_v11 RENAME TO sessions;
CREATE INDEX sessions_by_workspace ON sessions (workspace_id);

CREATE TABLE spaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    created_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE space_members (
    space_id   TEXT NOT NULL REFERENCES spaces (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('project', 'session')),
    member_id  TEXT NOT NULL,
    PRIMARY KEY (space_id, kind, member_id)
) STRICT;

CREATE INDEX space_members_by_member ON space_members (kind, member_id);
";

/// The version whose migration fills in worktrees' tickets.
const TICKETS: u32 = 9;

/// The version whose migration makes a table again, with foreign keys off.
const REBUILDS: u32 = 11;

/// Each worktree's ticket, as its branch's name says (`muster/issue-42-…`):
/// how the ticket's agents were found before it was stored. Run once, in
/// the migration that adds it.
fn tickets_from_branches(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    let named: Vec<(String, String)> = tx
        .prepare("SELECT id, branch FROM workspaces WHERE kind = 'worktree'")?
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (id, branch) in named {
        if let Some(key) = muster_core::tickets::issue_of_branch(&branch) {
            tx.execute(
                "UPDATE workspaces SET ticket = ?2 WHERE id = ?1",
                rusqlite::params![id, key.as_str()],
            )?;
        }
    }
    Ok(())
}

/// Refuses a migration that left a row naming one that is not there.
fn check_foreign_keys(tx: &rusqlite::Transaction<'_>) -> Result<(), StoreError> {
    let broken: Option<String> = tx
        .query_row("PRAGMA foreign_key_check", [], |row| row.get(0))
        .optional()?;
    match broken {
        Some(table) => Err(StoreError::Invalid(format!(
            "a row of {table} names one that is not there"
        ))),
        None => Ok(()),
    }
}

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
        // Foreign keys are turned off outside a transaction only.
        let rebuild = version == REBUILDS;
        if rebuild {
            conn.pragma_update(None, "foreign_keys", "OFF")?;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(migration)?;
        if version == TICKETS {
            tickets_from_branches(&tx)?;
        }
        if rebuild {
            check_foreign_keys(&tx)?;
        }
        tx.pragma_update(None, "user_version", version)?;
        tx.commit()?;
        if rebuild {
            conn.pragma_update(None, "foreign_keys", "ON")?;
        }
    }
    Ok(())
}
