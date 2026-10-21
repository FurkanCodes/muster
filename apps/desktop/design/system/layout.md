# Window layout

The Needs you window, measured. Every number is a token; the window is built from these regions and nothing else.

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ ● ● ●  ▲•ᴥ•▲!        ⌕ Go to a session or run a command   ⌘K       ⠹ 3  ▲ 2  ✓ 4  ▮▮▮▮▮▯▯▯ │  layout-titlebar 44
├──────────────┬─────────────────────────────┬────────────────────────────────────────────┤
│ ▲ Needs you 2│ Needs you                   │ ◎ Sketch the Tauri shell      ❯ Shell  ⋯   │
│ • Not seen  3│ Oldest first. Opening one…  │ ⎇ feature/desktop-app in muster   Codex    │
│ ⠹ Working   3│                             │ ┌────────────────────────────────────────┐ │
│ ≡ All      11│ Asking you                  │ │                                        │ │
│              │ ▲ ◎ Sketch the Tauri sh… 1m │ │  terminal (surface-term)               │ │
│ Projects     │      ⎇ feature/… in muster  │ │                                        │ │
│ ■ muster    7│      ┌ question preview ──┐ │ │                                        │ │
│ ■ lamphaus  4│      └────────────────────┘ │ └────────────────────────────────────────┘ │
│              │      ⏎ answer  d changes    │ ┌ This turn ─────────────────────────────┐ │
│              │                             │ │ link.rs +118 −4  main.rs +61  …        │ │
│ # Tickets    │ Finished, not seen          │ └────────────────────────────────────────┘ │
│ ⚙ Settings   │ ✓ ✻ Resize settles…    12m │                                            │
├──────────────┴─────────────────────────────┴────────────────────────────────────────────┤
│ ⏎ answer   d changes   T shell   z put back                         h every key   0.4.1 │  layout-statusbar 28
└───────────────────────────────────────────────────────────────────────────────────────┘
   layout-sidebar 220      layout-queue 420              the rest, at least layout-detail-min 520
```

## Regions

| Region | Size | Surface | Padding | Holds |
|---|---|---|---|---|
| Title bar | `layout-titlebar` 44 high | `surface-rail`, `line` below | left `layout-traffic-lights` 78, right `space-12`, gaps `space-12` | Kep, the command field (up to `size-command` 420), the flock count, the memory meter |
| Sidebar | `layout-sidebar` 220 wide; folded `layout-sidebar-folded` 52 | `surface-rail`, `line` right | `space-12` top and bottom, `space-8` sides | nav items (`size-control-lg` 32 high, `space-2` apart), the Projects label, Tickets and Settings at the foot |
| Queue | `layout-queue` 420 wide, dragged between `layout-queue-min` 360 and `layout-queue-max` 560 | `surface-ground`, `line` right | header `space-16` top, `space-20` sides; list `space-8` sides | the screen's title, its line of help, the sections and their items |
| Detail | the rest, at least `layout-detail-min` 520 | `surface-panel` | header `space-12` top, `space-20` sides; terminal `space-20` sides; This turn `space-12` above, `space-16` below | the pane header, the needs-you banner, the terminal, the This turn strip |
| Status bar | `layout-statusbar` 28 high | `surface-rail`, `line` above | `space-12` sides, gaps `space-16` | the selected thing's keys, a note, `h` every key, the version |

## As the window narrows

The window is never narrower than `layout-window-min-width` (960) or shorter than `layout-window-min-height` (600). As it narrows, in this order:

1. The queue gives up its width down to `layout-queue-min` (360).
2. The sidebar folds to `layout-sidebar-folded` (52): marks and counts only, each item's name in a tooltip. ⌘\ folds and unfolds it at any width.
3. The detail pane keeps at least `layout-detail-min` (520), about 64 terminal columns.

The command field shrinks before anything else in the title bar goes; then Kep goes, then the memory meter.

## A queue item, measured

```
 ┌──────────────────────────────────────────────────────┐
 │ 12                                                    │
 │ ▲  8  [◎]  8  Sketch the Tauri shell             1m  │  body-strong; age in meta
 │ 12    18                                              │
 │ ├──── layout-indent-queue 46 ────┤ ⎇ feature/desktop-app in muster      │  meta, code
 │                                  ┌ question preview ┐ │  terminal-compact, surface-term
 │                                  │ 8 / 12 padding   │ │  radius-md, at most 4 lines
 │                                  └──────────────────┘ │
 │                                  ⏎ answer   d changes │  hints, space-12 apart
 │ 12                                                    │
 └──────────────────────────────────────────────────────┘
   radius-lg, space-8 between its lines, space-4 between items
```

- An item that asks something shows its question, at most four lines of the agent's own text, cut with `…` beyond.
- A finished item is a two-line row (`size-row-two`): name, then branch, diff stat and pull request.
- A working item is a one-line row (`size-row`): name, what it is doing, age.

## The detail pane, measured

- **Header:** the agent's logo at `size-logo-lg`, the name in `title-pane`, actions on the right (medium quiet buttons, `space-8` apart). Under it, `space-8` lower, the meta line in `meta`, its parts `space-16` apart.
- **Needs-you banner:** `space-8` by `space-12` padding, `radius-md`, `waiting-soft`. Shown only while the session needs the person.
- **Terminal:** fills the height left, `radius-lg`, `space-16` padding, framed in `line`, or in `accent` while it has the keys.
- **This turn:** `space-12` by `space-16` padding, `radius-lg`, `surface-ground` with a `line` frame. Its files are small pills, `space-6` apart.
