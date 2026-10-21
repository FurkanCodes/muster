# NeedsYouBanner

One line over the terminal saying what the session waits for, while it waits.

**Anatomy.** `mu-banner`: `space-8` by `space-12` padding, `radius-md`, `waiting-soft`. Inside, `space-10` apart: the ▲ mark, a lead in `waiting` at 600 ("Needs you:"), what it waits for in `text` (a command in `code`), how long in `meta`, and the key that answers (`⏎ answer in the terminal`).

**Overlap.** The same shape in `working-soft` with a `working` lead (`mu-banner--overlap`) says another workspace changes the same files ("Overlap with ⎇ fix/resize-under-load: crates/muster-client/src/connection.rs").

**The consumer provides** the words, which come from the agent's own report ("wants to run cargo build", "asks which cache folder to use"). The banner goes the moment the session stops waiting. It never offers the agent's choices as buttons: the person answers in the terminal.
