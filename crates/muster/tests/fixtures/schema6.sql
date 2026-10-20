-- Frozen Muster schema 6, before measurement history was removed.

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


ALTER TABLE sessions ADD COLUMN removed_at INTEGER;


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


ALTER TABLE workspaces ADD COLUMN run_id TEXT REFERENCES runs (id) ON DELETE CASCADE;


ALTER TABLE workspaces ADD COLUMN stacked_on TEXT;
ALTER TABLE workspaces ADD COLUMN stacked_at TEXT;

PRAGMA user_version = 6;
