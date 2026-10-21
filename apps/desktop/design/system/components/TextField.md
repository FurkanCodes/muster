# TextField

A labelled field for what the person types: a first prompt, a session's new name, a plugin's address, a folder.

**Anatomy.** `mu-field`, its parts `space-6` apart: the label (`mu-field-label`, `meta-strong`, in `text`), the input, and a hint (`mu-field-hint`, `meta`, `text-quiet`) when it helps.

**Input.** `size-control-lg` (32px) high, `space-10` sides, `radius-md`, `surface-ground`, `body` text, and a `line-control` border, which alone shows where the field is (3:1). Hover darkens the border to `text-quiet`; focus turns it `accent` with a 1px `accent` ring outside, 2px in all. The placeholder is `text-quiet`, and says what to type ("What should the agent do?"), never the label again. `mu-field-input--area` is a text area at least 88px high, `space-8` by `space-10` padding, growing downwards.

**Error.** `has-error` turns the border and the hint `waiting`, and the hint says what to do ("Name it with at least one letter").

**The consumer provides** a `label` tied to the input by `for` and `id`, the value, and the hint or error. A field never shows an error before the person has tried.
