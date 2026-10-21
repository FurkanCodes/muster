# Queue

The Needs you screen's column: everything waiting on the person, oldest first, in three sections.

**Anatomy.** `layout-queue` (420px) wide on `surface-ground`. The head (`mu-queue-head`, `space-16` top, `space-20` sides) holds the title in `title-window` and one line of help in `meta`. The list (`mu-queue`, `space-8` sides, items `space-4` apart) holds, in order:
1. *Asking you* (`mu-section--asking`, `waiting`): **QueueItem**s.
2. *Finished, not seen* (`mu-section--news`, `news`): two-line **QueueRow**s.
3. *Working* (`mu-section--working`, `working`): one-line **QueueRow**s.

Section labels are `meta-strong` in their status colour, `space-24` above (`space-8` for the first) and `space-4` below. A section with nothing in it is left out.

**Empty.** With nothing asking and nothing unseen, the column says *All clear* in `title-section`, with how many work under it ("3 agents at work. Muster will show them here when they need you."), `space-48` from the top, and the *Working* section below it.

**Keys.** ↑↓ move the selection; ⏎ opens; `l` goes to the one that needed the person last. Selection holds its place when an item leaves, so the list never jumps under the pointer.
