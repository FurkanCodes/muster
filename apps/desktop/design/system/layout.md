# Window layout

The window, measured: one sidebar holding the list, the selected session beside it. Every number is a token; the window is built from these regions and nothing else.

```
┌──────────────────────────┬────────────────────────────────────────────────────────────┐
│ ● ● ●  ▲•ᴥ•▲!          ◧ │ ◎ Sketch the Tauri shell  ⎇ feature/… in muster +300 −6  ▣ ± ⋯ │  layout-titlebar 44
│ + New session        ⌘N │ ┌ ▲ Needs you: wants to run cargo build ──── 1m ⏎ answer ┐ │
│ ⌕ Search             ⌘K │ └─────────────────────────────────────────────────────────┘ │
│ ▭ Tickets               │                                                            │
│ [ Needs you 2 │ All 8 ] │   terminal, on the window's own ground (surface-term),     │
│ Asking you              │   no frame of its own                                      │
│ ▲ ◎ Sketch the Tau… 1m  │                                                            │
│     wants to run cargo… │                                                            │
│ Finished, not seen      │                                                            │
│ ✓ ✻ Resize settles… 12m │                                                            │
│ Working                 │                                                            │
│ ⠹ ◘ Upgrade AGP to… 8m  │                                                            │
│ ⚙ Settings           ⌘, │                                                            │
├──────────────────────────┴────────────────────────────────────────────────────────────┤
│ ⏎ answer  d see changes  z put back      Codex · 182k tokens · 212 MB  ⠿4 ▲2 ✓2 ▮▮▮▯ │  layout-statusbar 28
└───────────────────────────────────────────────────────────────────────────────────────┘
   layout-list 288                         the rest, at least layout-detail-min 520
```

## Regions

| Region | Size | Surface | Padding | Holds |
|---|---|---|---|---|
| Sidebar | `layout-list` 288 wide; away on ⌘\ | `surface-ground` | top row `layout-titlebar` 44 high, left `layout-traffic-lights` 78; `space-8` sides | Kep and the button that puts it away; New session, Search and Tickets (`size-control-lg` 32 high); the lists' switch; the list; Settings at the foot |
| Session | the rest, at least `layout-detail-min` 520 | `surface-term`, `line` at its left | head `layout-titlebar` 44 high, `space-20` left, `space-12` right; banner `space-20` left, `space-8` below | its head (logo, name, where it runs, what its work comes to, the shell, changes and more icons), the needs-you banner, the terminal or the changes |
| Status bar | `layout-statusbar` 28 high | `surface-rail`, `line` above | `space-12` sides, gaps `space-16` | the selected thing's keys, a note, then the session's agent and what it uses, the flock, the memory meter, `h` every key, the version |

*Tickets* and *Settings* take the session's place with their own two columns, the sidebar staying.

## As the window narrows

The window is never narrower than `layout-window-min-width` (960) or shorter than `layout-window-min-height` (600): the sidebar's 288 leaves the session at least 672, so neither gives way. In the session's head, where it runs is cut first; the icons stay. In the status bar, the session's facts are cut first.

With the sidebar away, the head makes room for the window's buttons (`layout-traffic-lights`) and shows the button that brings the sidebar back.

## A list row, measured

Every row of the list, in *Needs you* and in *All*, is a two-line row (`size-row-two`), or one line where the person says: its status mark, its agent's logo, its name over what it is doing (in the waiting colour while it asks), and its age. A finished row not seen says its branch, its diff stat and its pull request under its name. Both lists are sessions by project: a project's line in *All*, and its heading in *Needs you*, is its swatch and its name in `body-strong` and `text`, a place one goes to (its page), its `+` there to the pointer; over the list, *Projects* in `label`, quiet, with the `+` that adds one. A worktree with no session in it has one quiet line of its own.

## A project's page, measured

In the session's place while a project is selected: its head (swatch, name in `title-pane`, its folder in `code`, then *New worktree* and *Tickets*, `size-control-sm`); then, within `layout-measure`, the box that starts work (`surface-panel`, `radius-lg`, `space-12` padding, its border `accent` while typed in: the prompt in `prose`, then where, which agent and the primary button), and *Where its work is*: a card for the checkout and each worktree (`surface-panel`, `radius-lg`), its branch, diff stat, overlap and pull request on its first line with `+` and remove at its end, its sessions under it as one-line rows.

## The session, measured

- **Head:** the agent's logo, the name in `title-pane` at 14, where it runs in `meta`, then the shell (`terminal`), changes (`changes`) and more icons, `size-control-md`, `space-2` apart, each with its key in its tooltip.
- **Needs-you banner:** `space-8` by `space-12` padding, `radius-md`, `waiting-soft`. Shown only while the session needs the person.
- **Terminal:** fills the height left, on the window's own ground, no frame; beside a shell or a plugin's panel, each is a framed panel, the one with the keys framed in `accent`.
