# Badge

A count beside what it counts: sessions needing the person, news not seen, sessions in a project.

**Anatomy.** `size-badge` (18px) high and at least as wide, `space-6` sides, `radius-full`, `caption` text (11px, 600). Neutral counts are `text-quiet` on `surface-raised`. `mu-badge--waiting` is `waiting` on `waiting-soft`, for *Needs you*. `mu-badge--news` is `news` on `news-soft`, for *Not seen*.

**The consumer provides** the number, and an `aria-label` that says it in words ("2 need you"). A count of zero is not shown; past 99 it reads `99+`.

- Don't: use a badge for words; that is the pull request label or the row's second line.
