# Terminal

A session's live terminal: the program's own screen, which the person watches and types into.

**Anatomy.** `mu-term`: `surface-term`, a `line` frame, `radius-lg`, `space-16` padding, `terminal` type (JetBrains Mono 13px on 20px) unless the person set their own size. It fills the height the pane leaves it. The app draws it with xterm.js; the frame, padding and type here are the wrapper's.

**Keys.** While it has the keys (`has-keys`) its frame is `accent`, and every key goes to the program but the few that leave it. Click it, or press ⏎ on its item, to give it the keys; ⇧Q or a click on the queue takes them back.

**Colours.** The program's colours are its own, from the person's terminal theme. Muster's own touches are a selection in `surface-lit`, the cursor in `text`, and quiet text in `text-quiet` in previews.

**Compact** (`mu-term--compact`): `terminal-compact` type, `space-10` by `space-12` padding, `radius-md`, for the shell panel.

- Don't: put interface chrome inside the frame, or recolour the program's output.
