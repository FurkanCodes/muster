# keys

## Overview

A TypeScript port of the core's `muster_core::keys`, so a key means the same in
the window as in the terminal UI (`ARCHITECTURE.md` §5a, "The keys are the
person's"). `chord.ts` reads and writes the `[keys]` chord syntax. `keymap.ts`
lays the person's `preferences.keys` over Muster's table and adds plugin panel
actions, refusing what the core refuses. `useKeymap.ts` binds action keys per
screen. `panelKey.ts` is the prefix ("panel key") mode.

## Interface

- `Chord`, `Key` (chord.ts) — a key plus ctrl/alt, in `[keys]` form (a capital means Shift).
- `parseChord`, `writtenChord`, `sameChord` (chord.ts) — parse, write and compare `[keys]` text exactly as the core does.
- `chordLabel`, `domChord` (chord.ts) — the Mac key-cap label (⌃⌥⇧⏎) and the window listener's chord name.
- `pressedChord`, `charOfCode` (chord.ts) — a `KeyboardEvent` as a chord (null for ⌘ or a lone modifier), using `event.code` for Ctrl/Alt presses.
- `untyped`, `inTab` (chord.ts) — whether a key may live in a session (ctrl, alt or a capital) or in a terminal tab (ctrl/alt plus a lower-case letter).
- `Keymap`, `keymapOf`, `defaultKeymap`, `keptAs`, `KeysWarning` (keymap.ts) — the person's keys over the defaults plus plugin panels, with `problem` and `dropped` warnings.
- `ACTIONS`, `SCREEN_TITLES`, `KeyScreen`, `KeyAction`, `actionNamed` (keymap.ts) — the action table, generated from the core's `keys.json`, and its screens.
- `checkKey`, `Refusal`, `refusalText` (keymap.ts) — check a proposed key for an action or panel, and say why not.
- `chordsOf`, `labelOf`, `labelsOf`, `isSet`, `whatOf`, `actionAt`, `screenKeys` (keymap.ts) — key queries and the per-screen chord-to-action map.
- `panelsOf`, `panelName`, `panelWhat`, `isPanelName`, `PanelAction`, `warningsOf`, `warningText` (keymap.ts) — plugin panel actions (`plugin.<id>.<panel>`) and their warnings.
- `keymapFor`, `useKeymap`, `useKeyLabel`, `useActionKeys`, `useBorrowedKeys`, `KEY_SCREENS` (useKeymap.ts) — the memoised keymap and the hooks that bind a screen's keys.
- `takePanelKey`, `windowTakes`, `readKeymapFrom`, `stopPanelKeys`, `answerPanelKeys`, `panelKeyWaiting`, `waitingHint`, `PanelKeyHandler` (panelKey.ts) — the prefix-key state machine: one more key is read, arrows focus or swap, Esc gives up, and the panel key pressed again goes on to the terminal.

## Invariants

- ⌘ never appears in a chord; ⌘ keys belong to the window.
- The written form must match `muster_core::keys::Chord`, so keys round-trip with the TUI.
- A key the person sets is refused when every screen keeps it, when it could be typed into a session, when an in-tab key is not ctrl/alt plus a lower-case letter, or when another action holds it on a shared screen. On any refusal or parse error the whole keymap falls back to Muster's defaults, and `problem` says why.
- Panel keys the person set bind before plugin-suggested ones, so a later plugin never takes a key the person gave another panel. A key that cannot be bound is dropped and warned of.
- Plugin panel keys apply only on the session lists (`sessions`, `needs_you`).
- `keymapFor` returns the same object while the keys and plugins are unchanged.

## Side Effects

- No IPC. Reads `preferences.keys` and `plugins` from the daemon store.
- `panelKey.ts` keeps module-level state (keymap, handler, waiting, a told callback) and calls `preventDefault()` on the panel key and the key after it.
- Module-level caches: `last` in `useKeymap.ts`, a `WeakMap` for `screenKeys`.
- `useActionKeys` and `useBorrowedKeys` register window key handlers via `app/keys`.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/keys`
