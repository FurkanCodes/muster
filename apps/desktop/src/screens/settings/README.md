# settings

## Overview

The Settings screen (`PRODUCT.md` §6 Settings). It has two columns. On the left is a nav of Muster's own sections (General,
Notifications, Agents, Naming chats, Plugins, Keys) and then one section per project. On the right is the page shown. Every
section is one flat, ordered list of rows (`settingRows`, copied from the
TUI's `App::setting_rows` in `crates/muster-tui/src/app/settings.rs`).
↑↓/Home/End move through all rows, across pages. ⏎ and space go to the row's own
handlers, ← → step a row's choice, and the remappable action keys
(`read_settings`, `add_plugin`, `update_plugin`, `remove_plugin`, `back`) work
anywhere on the screen. Key rows also take ⌫ (put that key back) and ⇧R (put
every key back).

State lives in `SettingsProvider` (React state behind a context). The Zustand
stores are only read, apart from the selection and the status-bar hints.

## Interface

- `Screen` (Settings.tsx, default export) — the screen's entry: `SettingsScreen` wrapped in `SettingsProvider`.
- `SettingsProvider` (SettingsProvider.tsx) — owns the rows, selection, busy set, project reads and drafts, find text, key wait, plugin dialog and read-again count.
- `useSettings`, `SettingsContext`, `SettingsValue`, `SettingsState` (context.ts) — the context: `{ state, actions, meta: { followFocus, takesFocus, registry } }`; `useSettings` throws outside the provider.
- `SettingsActions` (context.ts) — `select`, `move`, `preference`, `setting`, `setDraft`, `readAgain`, `setFind`, `wait`, `add/update/remove/togglePlugin`, `setDialog`, `fetchPreview`, `installPreview`, `behaviour`.
- `RowBehaviour` (context.ts) — a row's `act(enter|space|click)`, `step(by)` and `back()` handlers, registered per row.
- `Read`, `Draft`, `PluginDialog` (context.ts) — a project's settings as last read (loading/loaded/failed), a tracker choice not yet kept, and the plugin dialog's steps.
- `Section`, `MUSTER_SECTIONS`, `Row`, `RowKind` (model.ts) — page ids (including `project:<id>`), the six Muster sections, and a row with its kind.
- `settingRows`, `keptRow`, `movedRow`, `firstRow` (model.ts) — build the rows. The others choose the row the selection stays on (as `keep_setting_selection` does), move by N rows clamped at both ends, and give a section's first row.
- `rowKey`, `rowName`, `sectionOf`, `projectSection`, `projectOf` (model.ts) — build and parse `section/row` keys and `project:<id>` sections.
- Pages: `General.tsx`, `Notifications.tsx`, `Agents.tsx`, `Naming.tsx`, `Plugins.tsx` (with `PluginDialogs.tsx`, `PluginFacts.tsx`), `Keys.tsx`, `Project.tsx`; shared `Row.tsx` (`SettingRow`, `SettingText`, `Control`, `Group`, `Page`, `Footnote`, `Unreadable`, `openSelect`). Helpers: `agentList.ts`, `agentsFound.ts`, `asks.ts`, `hear.ts`, `keyRows.ts`, `pluginWords.ts`, `trackers.ts`, `words.ts`.

## Invariants

- A busy row takes no second press until the daemon answers.
- A `ProjectSettings` answer is shown only if it answers the newest ask for that project; an older one is dropped.
- Only the newest plugin preview request may set the dialog.
- After a setting change is kept, its draft is cleared only if it is still the draft the change was made from.
- If a project's settings failed to read, a change reports the error and sends nothing; if they are not yet loaded, it does nothing.
- Selecting another row stops a key wait.
- Project rows (`tickets_from`, `field.*`, `only_mine`, `muster_prefix`) exist only once that project's settings are loaded; `remove` is always there. The `shared_builds` and `delegation` rows exist only while the daemon link is up.
- A built-in plugin cannot be updated or removed.
- `keepControlKeys` keeps a focused control's own ⏎ and space from reaching the screen's handlers.

## Side Effects

- IPC (`bridge.command`): `{Plugin: "List"}` on mount; `{ProjectSettings}` for each project not yet read; `{ChangePreference}` (including `"AllKeysBack"`); `{ChangeSetting}` (a refusal drops the cached read so it is read again); `"ReadPreferences"` on read-again (clears projects and drafts); plugin preview, install, update and remove commands.
- Store: `ui.select("settings", key)`, `view.setHints(...)` on each selection change (cleared on unmount), `ui.tell(text, isError)`, `view.show("all")` on back.
- DOM: reads `document.activeElement` to decide whether focus follows the selection.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/screens/settings`
