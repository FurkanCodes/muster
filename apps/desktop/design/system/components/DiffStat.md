# DiffStat

What a workspace's work, or one turn, or one file, comes to in lines.

**Anatomy.** `mu-diff`, in `meta` with tabular figures: `+120` in `diff-add`, then `space-4`, then `−30` in `diff-del`. The minus is the true minus sign (U+2212), so the two numbers line up. Where it helps, follow it in `text-quiet` with how many files ("in 5 files"). Either part is left out when it is zero; a change of no lines reads "no changes".

**Clashes.** Where two workspaces change the same files, put `overlap` (`mu-clash-overlap`, `working`) or `conflict` (`mu-clash-conflict`, `waiting`) before the stat, and keep it there as the rest of the line gives way.

- Don't: colour the file count, or round the numbers.
