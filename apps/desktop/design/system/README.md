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

The interface is grey, and colour is news. The neutrals are one warm grey (near hue 70, almost no chroma), stepped by lightness only; nothing is tinted toward blue or purple. Calm is the point: the sidebar recedes, the work leads, and a colour on screen means something happened.

Three grounds and no more: `surface-rail` and `surface-ground` (the same grey) frame the window, the sidebar and the status bar; `surface-term` is the darker work area beside them, a session's terminal and every page that takes its place; `surface-panel` is for what floats or is typed into (menus, dialogs, the palette, a composer). Show depth by shade, not by shadow.

- Set text in `text`; set what says less in `text-quiet`. Both pass 4.5:1 on every surface in both themes, `surface-lit` included. Never use a status or project colour for running text.
- Show selection with `surface-lit` while the list has the keys and `surface-tint` while the keys are elsewhere, so the person always sees where typing goes. Hover is `surface-raised`. Selection is always grey: no colour ever fills a row.
- `accent` is the grey at its lightest (darkest in Light): the primary button and a switch that is on, and nothing else. Text on it is `on-accent`. One primary button per view at most.
- `keys` is the Mac's blue and says where the keys are, and nothing else: the focus ring (`focus-ring`), a field or select being typed in, a terminal or panel that has the keys, a line being dragged, the status bar's "Typing in…". Chosen text is `text-selection`, the same blue, faint.
- Each status has a mark and a word, and never relies on the colour alone. Only three statuses take a colour:

| Status | Colour | Mark | Word |
|---|---|---|---|
| Working | `text-quiet` | turning ring | "editing view.rs", "running cargo nextest" |
| Needs you | `waiting` | filled dot with a halo | "wants to run…", "asks…" |
| Finished | `done` | filled dot | "finished" |
| Not seen | `news` | small dot at the line's end | the *Not seen* section |
| Error | `waiting` | ring | "exited with an error" |
| Ended | `still` | ring | "ended" |

- What a session asks or does is said in `text-quiet`; its mark carries the colour. Section headings are `text-quiet` too.
- Put a status colour's soft partner (`waiting-soft`, `done-soft`, `news-soft`) behind text in that status colour, never behind `text`; the Needs you banner is the one fill a status gets.
- Diffs read `diff-add` and `diff-del`; their lines' backgrounds are `diff-add-soft` and `diff-del-soft`. A changed (`M`) or renamed file is `text-quiet`: only added and deleted are coloured.
- Give each project a colour by its place in the list, `project-1` to `project-6`, then round again. It belongs to the project's swatch, a round dot, and nothing else: the name is `text`.
- The `agent-*` colours are the agents' own and belong to their marks only, where a logo is not drawn.
- An overlap between workspaces is `working`, a conflict is `waiting`, a merged pull request `merged`, changes asked for `waiting`, approved `done`; every other pull request word and a running check are `text-quiet`.

## Type

Two families: **Instrument Sans** for the interface and **JetBrains Mono** for anything a program wrote or a person would type. Both load from Google Fonts with system fallbacks (`font-sans`, `font-mono`).

- Set interface text in the Interface styles: `title-window` for a screen's title, `title-pane` for the selected session's name, `title-section` for dialogs and panel titles, `body` for everything else, `label` for controls, `meta` for what is said under a name, `caption` only inside counts.
- Weights are 400 and 500. A session that asks the person something is 500, as are names in the queue and project headings; 600 is kept for titles (`title-window`, `title-pane`, `title-section`).
- Set running text someone wrote, such as a ticket's body, in `prose` (14px on 22), within `layout-measure`. It is the one place the interface reads like a page.
- Ticket text, labels and authors come from trackers anyone can write to. Show them as plain text in `text` and `text-quiet`: never in a tracker's own colours, and never as links Muster did not make.
- Mono is for what a program wrote or a person would type at one: terminals in `terminal`, previews of a program's output in `terminal-compact`, file paths, diffs and commands in `code`. A branch, a ticket's key and a key cap are names, set in the interface's face. Never set labels, counts or times in mono, and never switch faces in the middle of a line for a name.
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

- Two radii: 6px for everything in the window (`radius-sm`, `radius-md` and `radius-lg` are all 6px) and 10px (`radius-xl`) for what floats. `radius-xs` (4px) is for key caps, labels and logos. Counts, dots, swatches and switches are round (`radius-full`).
- Structure is felt, not seen. Group with space and a quiet heading, part groups with one hairline at most; never box a group that is not a thing in itself. A box is for what can be typed into (a field, a composer) or what floats. No dashed borders.
- Every border is `stroke-hairline` in `line`. Fields and selects are framed in `line-strong` on `surface-term`. Key caps and labels have no frame: a `surface-raised` fill.
- Only floating things cast a shadow: `shadow-popover` for menus and the command palette, `shadow-dialog` for dialogs over `scrim`, `shadow-drag` for something being dragged.

## States

Every interactive element has the same vocabulary:

| State | Treatment |
|---|---|
| Hover | fill `surface-raised` (a primary button: `accent-hover`) over `timing-fast` |
| Pressed | fill one step further (`accent-pressed`, `surface-tint`) |
| Focus, a control | a `stroke-focus` ring in `focus-ring` on the control's own edge (inset, so it follows the radius and never spills); a primary button and a switch ring just outside; keyboard only (`:focus-visible`) |
| Focus, a line of a list | no ring: the line takes `surface-lit`, as a click would. A list itself never rings; its selected line, lit rather than tinted, says it has the keys |
| Selected, list has the keys | fill `surface-lit` |
| Selected, keys elsewhere | fill `surface-tint` |
| Has the keys (a terminal, a panel, a field) | frame in `keys` |
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

- **Status marks** are shapes the stylesheet draws, not glyphs, each with its word as its label: a ring turning in eight steps (working), a filled dot with a soft halo (needs you), a filled dot (finished), a ring (error, ended), and the small `news` dot. The flock in the status bar uses the same marks. ⎇ marks a branch, ❯ a shell.
- **Agents** are shown by their own logos (the *Agents* assets) at `size-logo-sm`, `size-logo` or `size-logo-lg`. Where a logo cannot be drawn, show the agent's mark in its `agent-*` colour: ✻ Claude Code, ◎ Codex, ✦ Gemini CLI, ▣ OpenCode, ◉ Copilot CLI, • an agent the person added.
- **Interface icons** are line icons at `size-icon` (14px) in buttons and `size-icon-lg` (16px) alone, `stroke-icon` wide, in `currentColor`. Use Lucide's set. An icon-only button always has an `aria-label` and a tooltip with its key.
- **Kep**, Muster's sheepdog, sits in the title bar and says how the flock stands, in `font-mono`: in `text-quiet`: `▼•ᴥ•▼` watching while something works, `▲•ᴥ•▲!` barking while something asks (in `waiting`, Kep's one colour), `▼^ᴥ^▼` glad for three seconds after a finish (in `text`), `▼-ᴥ-▼ᶻ` asleep while nothing works or waits, `▼;ᴥ;▼` while the daemon is out of reach. Kep is the first thing to go when the title bar runs short of room. Large, on the welcome and on screens with nothing on them yet, Kep is the *Kep* asset: the terminal UI's own 24 by 18 pixel portrait, drawn at `size-kep` with its pixels kept square.
- No emoji anywhere.

## Keys

Muster is driven from the keyboard first. Every action has a key, and every key can be changed in Settings › Keys.

- Show keys with the **Kbd** component, in hints (`⏎ answer`), menus (right-aligned) and tooltips ("Full screen (f)"). Keys are shown where they are asked for, not everywhere: a sidebar button's key appears while the pointer or the keys are on it.
- The status bar lists the selected thing's first three actions with their keys and ends with `h` for every key; the rest are in every key and the palette.
- In a terminal every key goes to the program, except the few that leave it, change screen or fold the sidebar.
