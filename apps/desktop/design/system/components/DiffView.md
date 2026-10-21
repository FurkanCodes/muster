# DiffView

One file's diff, as git gives it.

**Anatomy.** `mu-diffview`: `radius-lg`, a `line` frame, `surface-term`.
- **Head** (`mu-diffview-head`, on `surface-panel`): the file's kind letter, its path in `code`, its **DiffStat**, and at the end the part it is from ("Not committed", "On the branch", "Codex, turn 3").
- **Lines** (`mu-dl`), in `code` on 18px: the old and new line numbers in two `size-gutter` (40px) columns in `text-quiet`, the sign in a 20px column, then the line. Added lines sit on `diff-add-soft` with a `diff-add` sign; removed lines sit on `diff-del-soft` with a `diff-del` sign. Context lines are plain.
- **Hunk heads** (`mu-dl-hunk`): git's `@@ … @@` line in `news` on `surface-raised`, `space-6` above.

Tabs show as spaces; long lines scroll sideways inside the view, never wrap. A diff over 512 KiB is cut at a line's end and says so in its last line; a binary file says "binary" and shows nothing.

**Keys.** ↑↓, PgUp and PgDn, Home and End scroll; `n` and `p` go to the next and previous hunk; ⎋ goes back to the file list.

- Don't: syntax-colour the code in a way that hides the add and delete colours. The diff's own colours come first.
