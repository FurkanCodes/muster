# Select

One choice from a list too long for a segmented control: a notification's sound, which tracker a project's tickets come from, the agent that names chats, its model, what a worktree is made on top of.

**Closed** (`mu-select`). `size-control-md` (28px) high, at least 160 wide, with `space-10` on the left and `space-8` on the right. It has `radius-md`, `surface-ground` and a `line-control` border. Inside: the value (`mu-select-value`, which may start with a logo or a swatch), then a chevron at `size-icon` in `text-quiet`. Hover darkens the border; open (`aria-expanded="true"`) turns it `accent` with a 1px ring. `mu-select--lg` is `size-control-lg` for dialogs.

**Open.** The list is a **Menu** with `role="listbox"`, as wide as the select at least, opening just below it. The chosen option has a check in `accent` (`mu-option-check`) and `aria-selected="true"`. Options may carry a note in `text-quiet` at their end ("the Mac's", "last chosen").

**Keys.** ⏎ or space opens, ↑↓ move, ⏎ chooses, ⎋ closes. ← → step through the options without opening, as in the TUI. Choosing a sound plays it.

**The consumer provides** the options, the value, and a label naming what is chosen: the setting's row, or a `label` in a dialog.
