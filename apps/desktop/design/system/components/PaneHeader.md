# PaneHeader

The top of the detail pane: which session this is, where it runs, and what can be done with it.

**Anatomy.** `mu-pane-head`, `space-12` top, `space-10` bottom, `space-20` sides, its two lines `space-8` apart.
1. The title row: the agent's logo at `size-logo-lg`, `space-10`, the session's name in `title-pane` (cut at its end; the full name in a tooltip), and the actions at the right, `space-8` apart: quiet medium **Button**s with an icon and a word ("Shell", "Changes"), then **IconButton**s for full screen and more.
2. The meta line (`mu-pane-meta`), in `meta`, its parts `space-16` apart: the **Branch**, the agent's name, what it has used ("182k tokens", or "$0.42, 40k tokens" where the agent reports a cost), memory and CPU, and the **PullRequest**.

**The consumer provides** the session. *Rename* (`R`) and *Close* (`x`) live in the more menu, with their keys.
