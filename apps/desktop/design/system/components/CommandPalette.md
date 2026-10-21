# CommandPalette

⌘K, from anywhere: type to go to a session, a ticket or a setting, or to do an action on what is selected. `h` opens it showing every key of the screen in view.

**Anatomy.** `mu-palette`: `size-palette` (640px) wide, `radius-xl`, `surface-panel`, `shadow-dialog`, on `z-menu`, about a fifth of the way down the window over a `scrim`.
- **Field** (`mu-palette-field`): `size-palette-input` (48px) high, a search icon, and the input at 16px. The placeholder says what it finds ("Go to a session, ticket or setting, or do something").
- **Results** (`mu-palette-list`, at most 420px tall): grouped, each group under a `meta-strong` label (*Actions on Sketch the Tauri shell*, *Sessions*, *Tickets*, *Settings*). Items are **Menu** items `size-row` (32px) tall, with a logo, mark or swatch first and their key at the end. The typed letters are lit in `accent` (`mark`).
- **Foot** (`mu-palette-foot`): ↑↓ move, ⏎ do it, ⎋ close.

**Order.** Actions on the selected thing first, then sessions (those asking first), tickets, and settings. Matching ignores case; letters at a word's start come first.
