# tui

## Overview

What the terminal UI's cut is made from: the real `muster` binary (the one
on `PATH`), its terminal UI in a real terminal, driven through the story by
key presses over a throwaway world. Every pixel of the terminal is the
terminal UI's own output, drawn by xterm.js, the renderer the desktop app's
terminals use, in their colours and font.

## Interface

- `record.mjs` (`pnpm record:tui`, `--text` to keep each screen as text too): builds the world, starts agents in it, opens the terminal UI and drives it: the list, a worktree and an agent started in it, an agent asking and answered from the queue, its turn and the turn put back, a ticket handed to an agent. Writes `shots/*.png` (2x), `shots/kep.png` (the app's icon) and `crops.js` (`shots`, `crops`; no `points`, as nothing is clicked), every crop measured from where its text or frame is on the terminal's grid.
- `world.mjs`: `makeWorld()`: a world in `/tmp/muster-demo` with its own daemon (`MUSTER_RUNTIME_DIR`, `MUSTER_DATA_DIR`, `MUSTER_WORKTREES_DIR`, `MUSTER_CONFIG_DIR`; notifications, the notch and the Keychain off), git repositories in `code/`, and on its `PATH` stand-ins for Claude Code and the GitHub CLI, as `crates/muster/tests/support` has. `muster(args)`, `repository`, `startDaemon`, `list`, `session`, `started`, `hook` (a Claude Code hook, posted as its hooks post one), `show` (what the agent prints), `turn` (a turn: the prompt, files written in its worktree, the turn's end), `stop` (removes it all).
- `terminal.mjs`: `openTerminal(world, folder)`: `muster` in a pseudo-terminal (`bridge.py`) shown by `term.html` in headless Chromium; `press`, `type`, `waitFor`, `waitGone`, `settle`, `text`, `box` (a cell range in pixels), `screenshot`, `close`.
- `bridge.py`: runs a program in a pseudo-terminal of a given size and relays its bytes both ways.
- `term.html`: the terminal: xterm.js in the desktop app's terminal colours, JetBrains Mono at 17 px; `feed`, `reply`, `size`, `find`, `screenText`.

## Invariants

- Nothing reaches a real agent, tracker, the person's own Muster or their screen: the daemon and its files are the world's, the agent and `gh` are stand-ins, and the world is removed when recording ends.
- Statuses come from the real daemon, told by hooks the way Claude Code's hooks tell it; a turn's changes are real files in a real worktree, so its diff and putting it back are the daemon's own.
- Keys are pressed as a person would; what shows is what the terminal UI draws.

## Side Effects

- Creates and removes `/tmp/muster-demo` (a run that stopped half way leaves it; the next clears it first).
- Starts a daemon there, serves this folder on a free port of `127.0.0.1`, and runs headless Chromium.
- Writes `shots/` and `crops.js`.

## Verification

- `pnpm record:tui --text`, then read `shots/*.txt` and look at the shots; `pnpm check tui`, `pnpm contact --cut tui`.
