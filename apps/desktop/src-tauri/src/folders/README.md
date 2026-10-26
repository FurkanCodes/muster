# folders

## Overview

The submodule of `../folders.rs` that lists a folder's folders for the path
field's folder browser. It stands in for the Mac's own folder panel in builds
made for the end-to-end tests (`super::PickedFolder::Unavailable`), as the
TUI's picker does. In a browser (`pnpm dev:mock`) the webview's mock
(`/apps/desktop/src/ipc/mock/folders.ts`) stands in for it instead.

## Interface

- `Folder` (browse.rs) — one folder: `name`, whole `path` (as `AddProject` takes it), `git` (it has a `.git` of its own). Exported to TypeScript by ts-rs (`desktop/`).
- `Folders` (browse.rs) — the folder listed (`~` expanded) and its folders sorted by name, case ignored; hidden ones included.
- `FolderError` (browse.rs) — why a folder could not be listed: `Cannot read {path}: {reason}`.
- `list_folders(path)` (browse.rs) — Tauri command, compiled only with the `e2e` feature; lists off the webview's thread (`spawn_blocking`).

## Invariants

- Only the names of folders browsed to leave the module; files, non-UTF-8 names and names holding control characters are left out (`ARC-24`).
- A typed path is `~`, `~/…` or absolute; anything else is refused ("start it with / or ~/"). `~other/…` is refused.
- A folder that cannot be read is refused with an error, never shown as empty.
- On macOS the home folder's `Desktop`, `Documents` and `Downloads` are never looked into to mark them as repositories, so listing home raises no permission prompts.
- Links to folders count as folders.

## Side Effects

- Reads the filesystem (`read_dir`, `.git` existence checks) on a blocking thread.
- Reads `HOME` from the environment.

## Verification

- Run (in `apps/desktop/src-tauri`): `cargo test --features e2e folders`
- Tests are named after the rule they prove (`arc24_*`).
