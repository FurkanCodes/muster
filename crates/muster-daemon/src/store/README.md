# store

## Overview

The submodules of `src/store.rs`, the SQLite store (`ARCHITECTURE.md` §4).
`Store` is synchronous; the daemon reaches it only through `StoreHandle`,
which runs it on a thread of its own so async code never blocks on SQLite.
Projects, workspaces, sessions and their status history, runs and their
transitions, and spaces are kept here. The schema is migrated forward by
numbered migrations tracked in SQLite's `user_version` (currently 12,
`V1`..`V12`), with a backup copy of the database taken first.

`store` is one of the crate's public modules (`pub mod store` in `lib.rs`).

## Interface

From the module root `src/store.rs`:

- `Store` (store.rs) — `open(path, now)`, `open_in_memory`, `schema_version`,
  `snapshot`, `project_by_root`, `status_history`, and the writes:
  `add_project`, `insert_project`, `delete_project`, `update_ticket`,
  `insert_workspace`, `remove_workspace`, `insert_session`, `record_status`,
  `save_resume_id`, `rename_session`, `end_session`, `reopen_session`,
  `file_session`, `remove_session`, `update_branch`, `update_stack`, ….
- `StoreError`, `Result` (store.rs) — the store's errors.

In this directory:

- `StoreHandle`, `StoreClosed` (handle.rs) — `spawn(store)` and
  `call(work)`: run a closure on the store's thread.
- `MIGRATIONS`, `migrate`, `latest_version`, `schema_version`
  (migrations.rs) — the schema, version by version.
- `copy`, `folder`, `name`, `KEEP` (backups.rs) — `VACUUM INTO` a copy before
  migrating an existing database; the newest 3 kept.
- `StatusChange` (rows.rs) and row converters — rows to entities and back.
- `StoredRun`, `RunProgress`, `RunUpdate` (runs.rs) — `insert_run`,
  `record_run_transition`, `save_run`, `runs`, `run`, `run_history`.
- `Store::spaces`, `insert_space`, `rename_space`, `delete_space`,
  `move_member` (spaces.rs) — the person's spaces and their members.

## Invariants

- Once the daemon runs, async code never touches SQLite directly; everything
  goes through `StoreHandle` on the store's own thread. The one exception is
  start-up: `Store::open` (migrations and the backup) runs in `run` before
  `StoreHandle::spawn`.
- A shipped migration is never edited; a new one is added. Each runs in its own
  transaction with its version bump.
- An existing database brought up from an older schema is first copied into
  `backups/` (readable by the person alone), so an upgrade never loses history
  (`PRD-09`); a new or current database is not copied.
- A value that does not parse is reported as `StoreError::Invalid`, never
  silently replaced.
- Every run transition is recorded with its reason (`PRODUCT.md` §5).
- A space membership is written in the same transaction as what it names.

## Side Effects

- Reads and writes the SQLite database file (WAL) in Muster's data directory.
- Writes backup copies into `backups/` beside it and deletes older ones.
- Runs one dedicated OS thread per store.

## Verification

- Run: `cargo check -p muster-daemon && cargo nextest run -p muster-daemon store`
- End to end: `cargo nextest run -p muster --test daemon --test spaces --test runs`
