# The real app's end-to-end tests

`pnpm e2e:app` builds the app with its embedded WebDriver server (`--features e2e`) and runs every `*.spec.ts` here; `pnpm exec wdio run wdio.conf.ts --spec e2e-app/<area>.spec.ts` runs one against the app already built. Each spec file gets a sandbox of its own: a fresh daemon the app starts, its own folders, `HOME` (for WebKit's storage too, through `CFFIXED_USER_HOME`), and stand-ins on `PATH` for every agent, `gh`, the clipboard and `muster` itself. Nothing reaches the person's own Muster, and nothing is left running after a run. A failed test leaves a picture of the window in `test-results/e2e-app`.

A test does what the person does (keys, clicks) and asserts what the window shows **and** the real effect: the daemon's state, files, git, the stand-ins' records.

| `support/` | What it gives |
|---|---|
| `harness.ts` | `sandbox()` (its folders), `restartApp(extra, "killed" \| "quit")`, `quitApp()` (the person's ⌘Q), `keptState()` (the window's own state the app keeps), `appRuns()` |
| `muster.ts` | `muster(...args)`, `status()`, `session(title)`, `project(name)`, `until(read, what)`, `untilSession(title, "needs_you" \| "running" \| "finished")`, `settingsToml()`, `daemonLog()` |
| `agents.ts` | `startAgent(project, title, {agent, branch})`, `askingSession(...)`, and for a launch: `ask`, `work`, `finish`, `change(file, text)`, `show(text)`, `title(text)`, `hook(event)` |
| `repos.ts` | `repository(name)`, `commit`, `write`, `branch`, `branches`, `log`, `changes`, `tickets([...])`, `trackerDown(true)`, `ghCalls()`, `ticketsClosed()` |
| `window.ts` | `press("d", "⌘K", "⇧Tab", "Enter")`, `pressOnWindow(...)` (no field in focus), `type(selector, text)`, `typeInTerminal(line)`, `terminalText(id)`, `goTo("Tickets")`, `shownScreen()`, `untilFlock(working, waiting, done)` |
| `record.ts` | `opened()` (web addresses), `heard()` (sounds), `revealed()` (files shown), `daemonStarts()`, `holdDaemon(true)` |

Keys: send a capital as itself (`press("Q")`), never Shift and the letter. Text goes into a field with `type` and into a terminal with `typeInTerminal`: the embedded driver makes keys in the page itself, and xterm would take each letter twice. `muster` prints ids with their kind (`ses_…`); the window knows them bare (`bareId`).

A helper only one area needs goes in a file of its own here.
