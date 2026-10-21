# SettingsPage

One page of Settings: a project's settings, or one of Muster's own sections. Every change applies at once; there is no Save.

**Anatomy.** `mu-settings`, at most `layout-settings` (720px) wide, with `space-24` top and `space-32` sides; its blocks are `space-24` apart.
1. The head (`mu-settings-head`): the page's title (`mu-settings-title`, `title-window`, with a project's swatch beside it), and under it one line in `meta` saying what the page covers.
2. Groups (`mu-settings-group`): a quiet title (`meta-strong`, `text-quiet`), then rows `space-2` apart.
3. A footnote (`mu-footnote`, `meta`) saying where these settings are saved, the path in `code`, and that a project's settings never go into its repository.

**Row** (`mu-setting`). At least `size-row-two` (44px) high, with `space-8` by `space-12` padding and `radius-md`; hover fills `surface-raised`. On the left, the setting's name in `body`, then a line in `meta` saying what it does now ("Lists every open ticket"), or what it costs. On the right, its control (`mu-setting-control`): a **Switch**, a **Select**, a quiet **Button** ("Put back as at first"), or a value. A setting that comes from somewhere other than the person's own file has a **Label** after its name ("from muster.toml"). A row that belongs to the one above it, such as each kind of notification under *Notifications*, is set in by `space-24` (`mu-setting--sub`) and disabled while its parent is off.

**Keys.** ↑↓ move between rows, ⏎ or space turn a switch or open a select, and ← → step a select. `r` reads the files again after an edit by hand. The status bar says what changed ("Notifications on").
