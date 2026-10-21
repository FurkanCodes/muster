# TurnRow

One entry of a workspace's timeline: an agent's turn, the edits between turns, or something put back.

**Anatomy.** `mu-turnrow`: `space-6` by `space-8` padding, `radius-sm`. It holds:
- the entry's number (`mu-turnrow-num`), in `code` and `text-quiet`;
- the agent's logo at `size-logo-sm`, or ✎ for edits between turns (the person's own, or another program's), or ↺ for what was put back;
- what it was, in `body` ("Codex, turn 3", "Your edits between turns", "Put back before turn 2");
- under it, in `meta`, what it changed: the files and the **DiffStat**, or "changed no files";
- at the end, how long ago it ended.

States are the usual: hover, selected `surface-lit`, and focus. ⏎ shows its diff. `z` asks whether to put the files back as they were before it.

The store keeps a workspace's newest 100 entries, and says so at the end of the list.
