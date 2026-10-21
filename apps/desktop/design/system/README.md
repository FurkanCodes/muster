Muster is a cockpit for agents working on your tickets. The desktop app is its window on a Mac: a developer runs several coding agents across projects and worktrees, looks away to their editor, and comes back to see who needs them. The first screen is **Needs you**, a queue of what waits on the person, oldest first, with the selected session's terminal beside it. Everything here serves that glance: what is asking, what finished, what is still at work.

Dark is the first theme. Light follows the same rules value for value. A session's terminal keeps its program's own colours in either.

## Using the files

- Load `tokens.css`, then `components/bundle.css`, in that order, on any page, a Design canvas included. `bundle.css` fetches the two fonts from Google Fonts.
- Components are plain CSS classes prefixed `mu-`. Put `mu-root` on the outermost element to set the ground, the type and box sizing.
- Set `data-theme="light"` on any element to give it and what it holds the Light theme; Dark is the default.

## Voice

Muster speaks plainly about what happened, in the words the person would use.

- Say what the agent is doing, lower case, no subject: "wants to run cargo build", "asks which cache folder to use", "editing view/sessions.rs", "finished".
- Name things by what the person knows: *Needs you*, *Not seen*, *Working*, *Tickets*, *Settings*; a session is named by its first prompt ("Sketch the Tauri shell").
- Sentence case everywhere. No exclamation marks, no emoji, no "Oops". Kep's bark (`▲•ᴥ•▲!`) is the one exclamation.
- Times are short and relative: `1m`, `12m`, `3h`; a duration under a minute is `32s`.
- A control says exactly what it does, and its result says it back: **Put back** → "Put back 3 files as before turn 3"; **Close ticket** → "Closed #212".
- Errors and refusals say what went wrong and what to do: "This session has no transcript yet", "Changed since you approved it. Update it, or link it again."
- Keys are named as the person set them, never as the defaults: every hint, menu and tooltip reads the keymap.
- Muster's own copy spells *colours* the British way, as its Settings do.

## Colour

Use the surfaces as a ladder, darkest outside to the work in the middle: `surface-rail` frames the window (title bar, sidebar, status bar), `surface-ground` holds the queue, `surface-panel` holds the detail pane, and `surface-term` is reserved for a program's own text. Show depth by shade, not by shadow.

- Set text in `text`; set what says less in `text-quiet`. Both pass 4.5:1 on every surface in both themes, `surface-lit` included. Never use a status or project colour for running text.
- Show selection with `surface-lit` while the list has the keys and `surface-tint` while the keys are elsewhere, so the person always sees where typing goes. Hover is `surface-raised`.
- Spend `accent` on what has the keys and nothing else: the primary button, the terminal's frame while it is typed in, the focus ring (`focus-ring`), the current tab. Text on an accent fill is `on-accent`.
- Each status has a colour, a mark and a word, and never relies on the colour alone:

| Status | Colour | Mark | Word |
|---|---|---|---|
| Working | `working` | ⠋ spinner | "editing view.rs", "running cargo nextest" |
| Needs you | `waiting` | ▲ | "wants to run…", "asks…" |
| Finished | `done` | ✓ | "finished" |
| Not seen | `news` | • dot | the *Not seen* section |
| Error | `waiting` | ✗ | "exited with an error" |
| Ended | `still` | ○ | "ended" |

- Put a status colour's soft partner (`waiting-soft`, `working-soft`, `done-soft`, `news-soft`) behind text in that status colour, never behind `text`.
- Diffs read `diff-add` and `diff-del`; their lines' backgrounds are `diff-add-soft` and `diff-del-soft`.
- Give each project a colour by its place in the list, `project-1` to `project-6`, then round again. Use it for the project's name and swatch only; it never fills a row.
- The `agent-*` colours are the agents' own and belong to their marks only, where a logo is not drawn.
- Overlaps and conflicts between workspaces borrow the statuses: an overlap is `working`, a conflict is `waiting`. A merged pull request is `merged`.

## Type

Two families: **Instrument Sans** for the interface and **JetBrains Mono** for anything a program wrote or a person would type. Both load from Google Fonts with system fallbacks (`font-sans`, `font-mono`).

- Set interface text in the Interface styles: `title-window` for a screen's title, `title-pane` for the selected session's name, `title-section` for dialogs and panel titles, `body` for everything else, `label` for controls, `meta` for what is said under a name, `caption` only inside counts.
- Use `body-strong` for a session that asks the person something; nothing else in a list is bold.
- Set running text someone wrote, such as a ticket's body, in `prose` (14px on 22), within `layout-measure`. It is the one place the interface reads like a page.
- Ticket text, labels and authors come from trackers anyone can write to. Show them as plain text in `text` and `text-quiet`: never in a tracker's own colours, and never as links Muster did not make.
- Set branches, paths and commands in `code`, terminals in `terminal`, previews of a program's output in `terminal-compact`. Never set labels, counts or times in mono.
- Turn on tabular figures (`font-variant-numeric: tabular-nums`) wherever a number can change in place: counts, ages, diff stats, token use.
- Keep running text, as a ticket's body, to `layout-measure`.
- The scale steps by about 1.125 from `meta` (12px) to `title-window` (18px). Nothing in the interface is smaller than 11px.

## Space and size

Lay everything out on the 4px grid in `spacing`. Token names are their pixel values: `space-8` is 8px. The half steps `space-2`, `space-6` and `space-10` exist for tight places inside controls and rows; between blocks, use 4px multiples.

- Controls come in three heights: `size-control-sm` (24px), `size-control-md` (28px, the default) and `size-control-lg` (32px). Rows are `size-row` (32px, one line) or `size-row-two` (44px, two lines). Nothing the pointer must hit is smaller than 24 by 24.
- Build a row the same way everywhere: `space-12` sides, then a `size-mark` column for the status mark, `space-8`, the agent's logo at `size-logo`, `space-8`, the name taking the rest, the age at the end. Second lines start at `layout-indent-queue` (46px), under the name.
- Separate a group's members with `space-4` (rows) or `space-8` (buttons), groups from each other with `space-16`, and the queue's sections with `space-24`.
- Pad columns `space-20` at their sides, terminals `space-16`, dialogs `space-24`.
- See the section **Window layout** for the regions, their widths and how they give way as the window narrows.

## Shape

- Radii grow with the size of the thing: `radius-xs` for key caps and logos, `radius-sm` for small controls, `radius-md` for controls and rows, `radius-lg` for queue items and terminals, `radius-xl` for what floats. Counts, dots and switches are round (`radius-full`).
- A nested shape takes its parent's radius less the gap between them: a menu (`radius-xl`, 12px) with a `space-4` inset gives its items `radius-md` (8px).
- Every border is `stroke-hairline` in `line`. Use `line-strong` for key caps and hovered quiet buttons, `line-control` where the border alone identifies a control (text fields, an unchecked switch).
- Only floating things cast a shadow: `shadow-popover` for menus and the command palette, `shadow-dialog` for dialogs over `scrim`, `shadow-drag` for something being dragged.

## States

Every interactive element has the same vocabulary:

| State | Treatment |
|---|---|
| Hover | fill `surface-raised` (a primary button: `accent-hover`) over `timing-fast` |
| Pressed | fill one step further (`accent-pressed`, `surface-tint`) |
| Focus | a `stroke-focus` ring in `focus-ring`, `stroke-focus-offset` away; keyboard only (`:focus-visible`) |
| Selected, list has the keys | fill `surface-lit` |
| Selected, keys elsewhere | fill `surface-tint` |
| Has the keys (a terminal, a panel) | frame in `accent` |
| Disabled | `opacity-disabled`, no hover, and a tooltip saying why |
| Busy | the spinner in place of the icon, the label saying what is happening ("Starting…"), no second press |
| Error | `waiting` border or text, with words saying what to do |

## Motion

Motion shows what changed, in 120 to 240ms, easing out (`ease-out`) with no bounce.

- A row whose status changes flashes its status's soft colour for `timing-flash`, then fades for as long again. A snapshot after reconnecting flashes nothing.
- The working spinner steps every `timing-spinner`. Kep blinks while something works.
- A note in the status bar dims after `timing-note` and goes a second later.
- Under *Reduce motion* nothing moves: marks are still, flashes become an instant tint, menus appear without sliding.

## Iconography

- **Status marks** are glyphs set in `font-mono` in their status colour: ⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ (working, stepping), ▲ (needs you), ✓ (finished), ✗ (error), ○ (ended), and the `news` dot. ⎇ marks a branch, ❯ a shell.
- **Agents** are shown by their own logos (the *Agents* assets) at `size-logo-sm`, `size-logo` or `size-logo-lg`. Where a logo cannot be drawn, show the agent's mark in its `agent-*` colour: ✻ Claude Code, ◎ Codex, ✦ Gemini CLI, ▣ OpenCode, ◉ Copilot CLI, • an agent the person added.
- **Interface icons** are line icons at `size-icon` (14px) in buttons and `size-icon-lg` (16px) alone, `stroke-icon` wide, in `currentColor`. Use Lucide's set. An icon-only button always has an `aria-label` and a tooltip with its key.
- **Kep**, Muster's sheepdog, sits in the title bar and says how the flock stands, in `font-mono`: `▼•ᴥ•▼` watching while something works, `▲•ᴥ•▲!` barking while something asks (in `waiting`), `▼^ᴥ^▼` glad for three seconds after a finish (in `done`), `▼-ᴥ-▼ᶻ` asleep while nothing works or waits, `▼;ᴥ;▼` while the daemon is out of reach. Kep is the first thing to go when the title bar runs short of room. Large, on the welcome and on screens with nothing on them yet, Kep is the *Kep* asset: the terminal UI's own 24 by 18 pixel portrait, drawn at `size-kep` with its pixels kept square.
- No emoji anywhere.

## Keys

Muster is driven from the keyboard first. Every action has a key, and every key can be changed in Settings › Keys.

- Show keys with the **Kbd** component, in hints (`⏎ answer`), menus (right-aligned) and tooltips ("Full screen (f)").
- The status bar lists the selected thing's actions with their keys and ends with `h` for every key.
- In a terminal every key goes to the program, except the few that leave it, change screen or fold the sidebar.
