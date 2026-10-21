# Kbd

A key cap, showing the key that does something, as the person has set it.

**Anatomy.** `size-kbd` (18px) high and at least as wide, `space-4` sides, `radius-xs`, a `line-strong` hairline on `surface-panel`, with the key in `kbd` (JetBrains Mono, 11px) in `text-quiet`. Use Mac symbols for modifiers and special keys: ⌘ ⌥ ⌃ ⇧ ⏎ ⇥ ⎋ ⌫ ← ↑ → ↓. A chord is one cap (`⌘K`); a sequence is two caps.

**Hint.** `mu-hint` pairs a cap with what it does, `space-6` apart, in `meta`. A row of hints (`mu-hints`) spaces them `space-12`.

**The consumer provides** the key from the keymap, never a default written in by hand, since every key can be changed in Settings › Keys.

- Do: put keys in hints, menus (right-aligned) and tooltips ("Full screen (f)").
- Don't: show a key that does nothing on the screen in view.
