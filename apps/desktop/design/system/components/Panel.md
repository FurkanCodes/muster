# Panel

A live terminal shown in the detail pane: the session's agent, the shell in its workspace, or a plugin's panel. Panels share the pane side by side or one above the other.

**Anatomy.** `mu-panel`: `radius-lg`, a `line` frame, `surface-term`.
- **Head** (`mu-panel-head`): `size-control-lg` (32px) high on `surface-panel`, with a `line` hairline below. It holds the panel's title in 600 ("Codex", "❯ Shell", "Session graph"), what runs in it in `meta`, and an **IconButton** to put it away. The head is the handle that drags the panel.
- **Body** (`mu-panel-body`): the program's text in `terminal-compact`, or in `terminal` for the agent's own panel (`mu-panel-body--terminal`).

**Arranging.** `mu-panels` lays panels in a row, or a column with `mu-panels--column`, `space-8` apart; that gap is the line between two panels, and dragging it resizes them. Dragged by its head, a panel goes to the left, the right or the bottom of the pane, or beside or under another, and the place lights up as it would land. No panel gets narrower than `layout-panel-min` (320px). Where they do not fit, the shell opens over the agent. The layout is saved in the person's settings, shared with the terminal UI.

**Keys.** The panel with the keys has an `accent` frame and title (`has-keys`). A click gives a panel the keys; ⇧Q gives them back to the list. `T` opens or puts away the shell, and a plugin's key its panel.
