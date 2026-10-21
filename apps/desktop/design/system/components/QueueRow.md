# QueueRow

A session that finished and has not been seen, or one still at work: a row, not an item, since it asks nothing.

**Two lines** (`mu-row mu-row--two`, `size-row-two`, 44px), for *Finished, not seen*: the ✓ mark, the logo, then the name over a line of `meta` (`mu-row-line`, parts `space-10` apart) with the **Branch**, the **DiffStat** and the **PullRequest**; the age, with the `news` dot, at the end.

**One line** (`mu-row`, `size-row`, 32px), for *Working*: the spinner, the logo, the name, what it is doing in `meta` (at most 40% of the row, cut at its end), and the age.

Both have `space-6` by `space-12` padding, `radius-md` and `space-8` gaps. They share the item's states: hover `surface-raised`, selected `surface-lit`, focus ring.

**As it narrows,** the row gives way in this order: the pull request's word, then the diff stat, then what it is doing, then the name is cut. The mark, the logo and the age always stay.

**The consumer provides** the session. ⏎ opens it beside the queue; opening it clears its news.
