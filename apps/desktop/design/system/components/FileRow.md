# FileRow

One changed file in a list of changes: its kind, its path, its lines.

**Anatomy.** `mu-file`: `size-control-sm` (24px) at least, `space-4` by `space-8` padding, `radius-sm`, in `code`. Its parts, `space-8` apart:
- the kind, git's letter in a `size-mark` column: `M` modified (`working`), `A` added (`done`), `D` deleted (`waiting`), `R` renamed (`news`), `?` untracked (`text-quiet`);
- the path: its folder in `text-quiet`, its name in `text`, cut from the start when short of room, so the name stays;
- the **DiffStat**, or `binary` in `text-quiet`.

**States.** Hover `surface-raised`, selected `surface-lit`, focus ring. ⏎ or a click shows its diff.

**The consumer provides** the file from the workspace's changes, grouped under *Not committed* and *On the branch* by a `meta-strong` label.
