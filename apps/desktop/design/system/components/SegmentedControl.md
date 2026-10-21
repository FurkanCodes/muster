# SegmentedControl

Two to four views of the same thing, one shown at a time: List or Board, Not committed or On the branch.

**Anatomy.** A `line` frame with `radius-md` and a `space-2` inset holds the segments. Each segment is `size-control-sm` (24px) high with `space-10` sides, `radius-sm` and `meta-strong` text. The chosen segment (`aria-pressed="true"`) fills `surface-raised` in `text`; the others are `text-quiet`.

**The consumer provides** the segments' labels and which one is chosen, and a key that goes round them.

- Do: use it to switch how one thing is shown.
- Don't: use it to move between screens (that is the sidebar), or for a setting that stays on (that is a **Switch**).
