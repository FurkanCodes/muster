# ThisTurn

Under the terminal: what the agent's latest turn changed, a key from its diff and a key from putting it back.

**Anatomy.** `mu-turn`: `space-12` by `space-16` padding, `radius-lg`, `surface-ground` with a `line` frame, its two lines `space-10` apart.
1. The head (`mu-turn-head`): "This turn" in 600, what it was in `meta` ("Codex, turn 3, changed 3 files"), the **DiffStat**, a spacer, then quiet small **Button**s *Put back* (`z`) and *See the diff* (`d`).
2. The files (`mu-turn-files`, `space-6` apart): one pill each (`mu-pill`, `size-control-sm`, `radius-sm`, `surface-raised`, the file's name in `code` and its stat). A pill opens that file's diff.

**States.** While the agent works there, *Put back* is disabled with the tooltip "Wait for its turn to end". A turn that changed nothing shows "changed no files" and no pills. *Put back* always asks first, saying what goes with it and that the agent still remembers the turn.

**The consumer provides** the newest turn from the workspace's timeline.
