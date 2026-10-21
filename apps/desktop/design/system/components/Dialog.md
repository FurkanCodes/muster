# Dialog

A question Muster must ask before it acts: what will change, who is told, whether it can be undone. Use one only where the answer matters; everything else happens in place.

**Anatomy.** `mu-dialog`: `size-dialog` (440px) wide, `space-24` padding, `radius-xl`, `surface-panel`, `shadow-dialog`, on `z-dialog`, centred over `scrim`, its blocks `space-16` apart. The title (`mu-dialog-title`, `title-section`) is the question ("Close ticket #212?"). The text says what happens, in `body`, plainly: who is told, what is kept, what goes. The actions are at the bottom right, `space-8` apart, `space-8` above: the answer that does nothing first (secondary, "Keep it"), then the one that acts (primary, or danger where others are told or it cannot be undone), named for what it does ("Close ticket").

**Keys.** ⏎ does the acting answer, ⎋ the other; each button shows its key in its tooltip. Focus starts on the acting button, unless that is danger, when it starts on the other.

- Don't: open a dialog over a dialog, or ask "Are you sure?". Say what will happen.
