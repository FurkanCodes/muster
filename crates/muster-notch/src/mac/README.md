# muster-notch/src/mac

## Overview

Submodules of `mac.rs` (macOS only): the AppKit / Core Animation /
UserNotifications pieces the notch's screen is drawn and driven with. All
items are `pub(super)` — private to `mac.rs` — and everything runs on the
main thread. This directory, with `mac.rs`, holds the crate's only `unsafe`
code (`ARC-12`); each `unsafe` block carries a `SAFETY` note.

## Interface

- `now` / `redraw` (events.rs) — the screen's one monotonic clock (`ARC-03`); redraw the screen.
- `Mouse` (events.rs) — global + local `NSEvent` monitors for pointer moves and drags; removed on drop.
- `Motion` (events.rs) — observers for screen-parameter and accessibility (Reduce Motion) changes; removed on drop.
- `Timer` (events.rs) — a single one-shot `NSTimer`, `set(start, at)` in 100 ms ticks; re-armed only when the deadline changes, invalidated on drop.
- `List` / `Drawn` (list.rs) — the open list: two lines per session (logo, name, duration; project, activity), a rule between waiting and working, the hovered line lit, "N more"; `reveal`, `conceal`, `draw`.
- `Colour` (look.rs) — the UI's colours on a dark terminal, as `NSColor`.
- `Label` (look.rs) — a text field updated only when its text changes; `show`, `fit`, `centre`, `width`, `hide`, `say` (accessibility text).
- `Logo` / `logo(harness)` / `shape` (look.rs) — an agent's logo PNG by harness; small dot/rule marks.
- `Notifier` (notify.rs) — Mac notifications when the program runs as Muster's app: `start(said)` asks for permission once, `post(posted)` posts one notification per session (thread = project, chosen sound); a click asks for the session to be shown.
- `Shape` / `Motion` (shape.rs) — the panel's mask from `geometry`'s outline, animated open/close/resize from the on-screen shape; `reduce_motion`.
- `appear` / `disappear` / `pulse` / `breathe` / `still` / `moving` (shape.rs) — Core Animation fades, the question pulse and the idle breath.

## Invariants

- Nothing polls the pointer: AppKit event monitors and at most one one-shot timer drive redraws, and hidden panels keep neither.
- Every `unsafe` block is confined here or in `mac.rs` (`ARC-12`) and documented with a `SAFETY` comment.
- Under Reduce Motion the shape changes at once, the list only fades, and nothing pulses or breathes; a Reduce Motion change removes a running breath.
- A shape change always starts from the shape on screen now, so a change of mind halfway never jumps.
- A label's text and colour, and a logo's picture, are set only when they change; their frame and visibility are set on every `show`.
- Without a bundle identifier (not running as Muster's app) there are no notifications; the link is told `CanNotify(false)`.

## Side Effects

- Installs AppKit global/local event monitors and `NSWorkspace` / `NSNotificationCenter` observers.
- Schedules `NSTimer`s on the main run loop; adds Core Animation animations to layers.
- Requests notification authorization and posts `UNNotificationRequest`s (sounds from the system or `~/Library/Sounds`).
- Sends `FromScreen` messages to the link over a tokio mpsc channel.

## Verification

- Run: `cargo check -p muster-notch`, then `cargo nextest run -p muster-notch` (on a Mac)
- End-to-end: `cargo nextest run -p muster --test notch`
