# SessionTree

The *All sessions* screen's list: projects, their worktrees, and the sessions in each, as the terminal UI lists them.

**Rows** (`mu-tree-row`, `size-control-md` at least, `space-4` by `space-8` padding, `radius-md`):
- **Project:** a fold triangle (▾ open, ▸ folded) in a `size-mark` column, its swatch, its name in 600 in its project colour, and at the end its folder in `code`.
- **Worktree or checkout:** the fold, ⎇ and the branch in `code`. At the end come the **DiffStat**, the clash word if any, and the **PullRequest**. Folded, it shows the mark of its most urgent session and how many it hides ("✓ 2 folded"). Urgency goes: asking first, then news not seen, then working.
- **Session:** a two-line **QueueRow** with the mark, logo, name, what it is doing and the age.
- **Shell** (`mu-tree-shell`): one line in `code` and `text-quiet`, `└ ❯` and the program in front. It sits under its worktree's chat, or after the chats where several share the workspace.
- **Plugin panel:** one line like a shell, with its plugin's name.

Children sit in `mu-tree-children`: indented to the parent's fold, with a `line` hairline down their left. Projects are `space-16` apart.

**Keys.** ↑↓ move, ← → fold and unfold (`e` too), ⏎ opens. A session selected for the person, such as one picked in the notch, unfolds whatever hides it.
