# ui

## Overview

The desktop app's component library (`ARC-26`), built on Base UI
(`@base-ui/react`). Screens compose these parts and never hand-make a
`<button>`, `<input>` or `<dialog>`. The design, tokens and the table of
components are in the design system's own doc,
[/apps/desktop/design/system/README.md](/apps/desktop/design/system/README.md).

Simple parts are thin functions over `usePart(tag, props, own)`, which wraps
Base UI's `useRender` and `mergeProps`. Interactive parts are built on Base
UI's own parts. Floating layers portal into `.mu-root`, so they carry the design
system's fonts, colours and box sizing.

## Interface

- `usePart`, `PartProps` (part.ts) — draw a tag with the part's own props merged under the caller's: classes joined, both handlers called (the caller's last), and `render` to swap the element.
- `nativeButton` (part.ts) — the `render` for a Base UI button: a native `<button>`, with no forced `tabindex=0` and `type="button"` by default.
- `portalRoot` (portal.ts) — the `.mu-root` element floating layers portal into.
- `listOpened`, `listClosed`, `listTakesEscape` (layers.ts) — a count of open floating lists, so Escape closes only the top layer.
- `Button`, `Pressable`, `IconButton`, `Kbd`, `Hint`, `Spinner`, `Segmented`, `Segment`, `Spacer` (controls.tsx) — buttons (variants primary, secondary, quiet, danger; busy state), key caps and hints, a CSS-only spinner, a one-pressed toggle group, a gap (optionally a window drag region).
- `List`, `ListItem`, `ListItemFlash`, `ListItemText`, `ListItemTitle`, `ListItemMeta`, `ListItemEnd`, `RowButton` (List.tsx) — a keyboard list: a focusable listbox or tree whose lines never take focus.
- `Modal`, `Dialog`, `DialogText`, `DialogAnswers` (Dialog.tsx) — an always-modal layer and the confirm dialog on it.
- `ActionMenu`, `MenuItem`, `keysOf` (Menu.tsx) — a menu opened from a trigger or at a point (right-click), with each item's own key.
- `Tooltip` (Tooltip.tsx), `Select`, `SelectOption` (Select.tsx), `Switch` (Switch.tsx), `Checkbox` (Checkbox.tsx), `TextField`, `TextArea`, `Input`, `Field`, `PlainInput`, `PlainTextArea` (TextField.tsx) — form controls.
- `Panel`, `PanelHead`, `PanelTitle`, `PanelClose` (Panel.tsx) — a framed terminal panel.
- `Icon`, `IconName` (Icon.tsx) — inline SVG line icons in `currentColor`.
- `DiffView`, `DiffFrame`, `DiffHead`, `DiffWaiting` (DiffView.tsx) and `parseDiff`, `lineAt`, `isBinary`, `LINE`, `HUNK`, `DiffLine` (diff.ts) — diffs.
- `FileRow`, `fileLineId`, `fileKind`, `FileKind` (FileRow.tsx), `TurnRow`, `turnLineId` (TurnRow.tsx), `AgentChoice`, `AgentOption` (AgentChoice.tsx), `FolderOption` (FolderOption.tsx), `KepFace`, `KepPortrait`, `Mood` (Kep.tsx) — app-specific rows and marks.
- `Heading*`, `Section`, `SectionTitle`, `EmptyState*`, `Banner*` (with `BannerTone`), `NavItem`, `NavCount`, `NavLabel`, `DetailList`, `Detail`, `Label`, `Labels`, `Loading` — layout and text parts. `status.tsx` holds `StatusMark`, `MarkSpace`, `Badge`, `PullRequest`, `pullRequestWord`, `DiffStat`, `Branch`, `BranchName`, `AgentLogo`, `AgentMark`, `Found`, `Missing`, `Version`, `ErrorText`, `NewsDot`, `Swatch` and `Clash`.
- `useMediaQuery` (useMediaQuery.ts) — a breakpoint hook on a `matchMedia` change listener.

## Invariants

- `ARC-26`: a part's `mu-*` classes are written only in the file that defines it. Callers add their own `className` or swap the element with `render`; they never restate `mu-*` classes.
- Every Base UI button, toggle and select trigger takes `render={nativeButton}`.
- Variants are closed string unions, never stacks of booleans.
- `List` keeps focus on its container and names the selection with `aria-activedescendant`. A line never takes focus. A row with controls inside it is not a `ListItem`.
- Every floating layer portals to `portalRoot()`. Escape closes only the top layer: `Modal` ignores an Escape while `listTakesEscape()` holds.
- `Modal` is always modal and never closes on an outside click; it restores focus itself. `Dialog` stops key events, so the window's key listener never sees them. It loops Tab, steps between answers with ←/→, and accepts on ⏎/⌘⏎.
- `ActionMenu`: an item's own key wins over first-letter typeahead. Keys other than ⌘ shortcuts never reach the screen behind.
- `Segmented` always has exactly one pressed, and leaves the arrow keys to the screen.
- `ARC-25`: the spinner is CSS-only (or `still`). Breakpoints are subscribed to, not computed from resize streams.
- A component change is compared pixel for pixel before and after; the design never moves with it.

## Side Effects

- Portals into `.mu-root` (Modal with backdrop, ActionMenu, Tooltip, Select's list).
- Focus: Modal focuses its `initial` element and restores the previous focus on unmount. `Dialog` sets `keysTo("dialog")` and restores the previous `keysIn` on unmount. A selected `ListItem` scrolls into view.
- Module state in `layers.ts` (an open count, and when the last Escape closed a list).
- `matchMedia` listeners; `data-tauri-drag-region` on `Spacer dragsWindow` and panel heads; CSS imported per part.

## Verification

- Run (in `apps/desktop`): `pnpm lint`, `pnpm typecheck`, `pnpm exec vitest run src/ui`
