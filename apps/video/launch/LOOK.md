# Look

The clip looks like the app it shows: its colours are the desktop app's
tokens (`apps/desktop/src/styles/tokens.css`), its type the app's own faces.

- **Canvas:** `#171716` (the app's ground), with a faint lift of `#1d1d1c` in the middle. Nothing brighter behind the product.
- **Type:** Instrument Sans. Lines of the story at 120 px (the second line 84 px), weight 600, tracking -2 %; headlines 64 px across the top or bottom and 72 px in a column at a side, their second line 30 px in `#9b988f`. Code and commands in JetBrains Mono.
- **Where the text goes:** each scene's headline takes a side of the screen (top, bottom, left or right), never the same side twice in a row, and the window moves to the space it leaves; a soft band of the canvas colour fades the window's edge under the text. Nothing the scene shows or clicks is ever under it (`pnpm check`).
- **Text:** `#e8e6e1`; quiet `#9b988f`.
- **The accent** in a line (a word, or two that go together: "one list", "needs you") takes the colour of what it means, as the app's marks do: waiting `#ef7e6b`, working `#d4b06f`, done `#7fbf8f`, and the keys' blue `#4f8fff` for where work goes (a worktree, a server). One accent a line.
- **Cards:** a crop of the app on `#1d1d1c`, 18 px corners, a 1 px line of `rgba(255,255,255,0.08)`, and the app's dialog shadow (`0 24px 64px rgba(0,0,0,0.55)`).
- **The window:** the shot at 12 px corners with the same line and shadow, and the Mac's three buttons where the app leaves room for them.
- **Rings:** what the eye should find is ringed, 3 px, in the colour of what it means, 10 px clear of it.
- **The pointer:** the Mac's arrow, white with a black edge; a click shrinks it and leaves a ring.
- **Motion:** things arrive on a beat with a spring and settle with a small overshoot; they leave on the beat before the next thing arrives. The camera moves only between beats' events, one move at a time, and carries the window from one side to the other as the text changes sides.
- **Sound:** quiet. Typing where something is typed, a soft click where the pointer clicks or a row lands, and nothing else.
