# StatusBar

The window's bottom edge: what the selected thing can do, with its keys, and Muster's notes.

**Anatomy.** `layout-statusbar` (28px) high on `surface-rail`, a `line` hairline above, `space-12` sides, parts `space-16` apart, all in `meta`. From the left: the selected thing's actions as hints (**Kbd**), as many as fit whole; a note when there is one; a spacer; `h` every key; Muster's version.

**Notes.** A note (`mu-note`) says what just happened in `text` ("Put back 3 files as before turn 3"). It dims after `timing-note` and goes a second later, giving the hints back. An error (`mu-note--error`, in `waiting`) stays until the next key.

- Do: read every key from the keymap.
- Don't: use the status bar for anything the person must act on; that belongs in the queue.
