# QueueItem

A session that asks the person something, shown with its question, so they can decide without opening it.

**Anatomy** (see **Window layout** for the measured drawing). `mu-item`: `space-12` padding, `radius-lg`, its lines `space-8` apart.
1. The head: the ▲ mark, `space-8`, the agent's logo at `size-logo`, `space-8`, the name in `body-strong`, and the age in `meta` at the end.
2. Under the name, from `layout-indent-queue` (46px): the **Branch**.
3. The question (`mu-question`): the agent's own last lines, at most four, in `terminal-compact` on `surface-term`, with a `line` frame, `radius-md` and `space-8` by `space-12` padding. The line that asks is in `working` (`mu-question-ask`). Its control characters are removed before it is shown.
4. Its keys as hints: `⏎ answer`, `d see changes`.

**States.** Hover fills `surface-raised`. Selected while the queue has the keys, `aria-selected="true"` fills `surface-lit`; selected while the keys are in the terminal, add `is-resting` for `surface-tint`. Focus shows the ring. As a session newly needs the person, its item flashes `waiting-soft` (`mu-flash mu-flash--waiting`).

**The consumer provides** the session (name, agent, branch, project, age) and its question's lines. ⏎ opens it with the keys in its terminal, where the person answers the agent as they would in their own terminal. Muster never answers for them.

- Don't: show more than four lines, or put buttons for the agent's choices in the item.
