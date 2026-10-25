# Repository agent instructions

- Read [PRODUCT.md](PRODUCT.md) (`PRD-*`) and [ARCHITECTURE.md](ARCHITECTURE.md) (`ARC-*`) before planning, writing or reviewing code. Cite the rule IDs a change touches in plans, commit messages, reviews and test names.
- When a change conflicts with a rule, stop and say so. Change the rule deliberately, or change the plan — never let code and rules drift.
- `muster-core`, `muster-state` and `muster-model` do no IO (`ARC-01`). Docker, GitHub, ticket sources, harnesses, PTYs and the clock are reached only through their traits (`ARC-03`).
- No source file over 1,000 lines and no struct over 40 fields (`ARC-08`). Split by responsibility before you get close.
- Every behaviour change comes with tests: unit tests for the state machines, parsers and scheduler; end-to-end tests against the real daemon binary with fakes for external systems.
- Before finishing: `cargo fmt --check`, `CARGO_BUILD_WARNINGS=deny cargo clippy --workspace --all-targets`, and the tests of what changed (see *Which tests to run*); CI runs them all.
- Never put a real credential in code, fixtures, logs or test output (`PRD-06`). Recorded agent fixtures are scrubbed before they are committed.
- After you create or rewrite a Markdown (`.md`) file, read it back in full before you call it done.
- Nebula (MIT) informed the design. If its code is ever copied, add its notice to `NOTICE` in the same change.

## Which tests to run

Compile wide, test narrow: clippy over the whole workspace catches a changed interface breaking a crate above it, so the tests can stay with what changed. Never run the whole suite for one change.

- **While working:** only the test being written or fixed: `cargo nextest run -p <crate> <name filter>`.
- **Before finishing:** the changed crate's tests, and the end-to-end files in `crates/muster/tests/` for the area touched (each starts the real daemon, so pick by name: several `--test` flags or a glob):

  | Changed | Run |
  |---|---|
  | `muster-tui`, `muster-model` | `-p muster-tui -p muster-model`, then `-p muster --test 'tui*'` |
  | `muster-daemon`, `muster-state` | `-p muster-daemon -p muster-state`, then `-p muster --test <area>` (`hooks`, `runs`, `worktrees`, …) |
  | `muster-notch` | `-p muster-notch`, then `-p muster --test notch` |
  | `muster-client` | `-p muster-client`, then `-p muster --test lifecycle --test detach --test slow_client`; every client talks through it, so a change to framing or the handshake runs the whole suite |
  | `muster-core` | `-p muster-core`, then the crates and end-to-end files that use what changed |
  | `crates/muster` (CLI) | the end-to-end files for the commands touched |
  | `apps/desktop` webview | `pnpm lint`, `pnpm typecheck`, `pnpm test`; `pnpm e2e` for a screen's flow |
  | `apps/desktop/src-tauri` | `cargo test` there; `pnpm bindings` if a type the webview uses changed |

- **The whole suite** (`cargo nextest run --workspace`) only for a protocol version change (`ARC-09`), a dependency or toolchain update, a release, or when asked.
- **Before a release only,** as CI does not run them: `pnpm e2e:app` (the real app, on a Mac) and the real-Docker suite (`MUSTER_DOCKER_TESTS=1 cargo test -p muster-daemon`).
- **Don't repeat work:** a test that passed is not run again unless its code changed since; after a failure, rerun only the failing test until it passes, then the step's set once.
- **Compile first:** `cargo check -p <crate>` before a test run, so a compile error never costs a test build.
- **Keep a failure whole:** never cut test, build or clippy output with `| tail`, `| grep` or `> /dev/null`; when a run fails, read its first failure in full before running anything again.
- Claude Code asks the person before a whole-suite run (`.claude/settings.json`, `scripts/hooks/whole-suite.py`).

## Working efficiently

Measured on this repository's own agent sessions (`scripts/agent-sessions.py`, 2026-10-22); rerun it to see what a change to this file did.

- **Read code with the agent's own read and search tools** (CodeGraph's `codegraph_explore` where `.codegraph/` exists), not `cat`, `sed`, `grep` or `find` in a shell: a shell call costs seconds before it starts, a read a few hundredths of one. Say so in a subagent's brief too.
- **Read `ARCHITECTURE.md` and `PRODUCT.md` by section** (search for the rule ID or heading, then read those lines), never whole: they are 200 KB and 50 KB.
- **Wait in the background:** a build, a CI run or a daemon is waited on by a background command or a watcher, never a foreground `sleep`; every loop that waits has a limit.
- **Subagents for wide reading or research only,** never for a lookup of a known file or symbol; they cost many times the tokens of doing it directly.

## Desktop app (`apps/desktop`)

- The plan, branches and who owns what: [apps/desktop/PLAN.md](apps/desktop/PLAN.md). The app is `PRODUCT.md` §6a and `ARCHITECTURE.md` §5c (`ARC-24`, `ARC-25`, `ARC-26`).
- Screens compose the components in `src/ui` (and `src/app`'s `Page` and `Sidebar`); a component's `mu-*` classes are written only in the file that defines it (`ARC-26`); the table in `apps/desktop/design/system/README.md` (Components) says what exists. A list the keyboard moves through is a `List`.
- Every component is built on Base UI (`@base-ui/react`, its docs in `node_modules/@base-ui/react/docs`): an interactive one on its part, the rest on `usePart` (`src/ui/part.ts`, Base UI's `useRender`); a Base UI button takes `render={nativeButton}`. No Radix, no hand-made `<button>`, `<input>` or `<dialog>` in a screen. Before and after a component change, compare the window's states pixel for pixel; the design never moves with it.
- `pnpm install` in `apps/desktop`, then `pnpm dev:mock` (the window over a pretend daemon, in a browser), `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm e2e`; `pnpm bindings` after changing a Rust type the webview uses; `pnpm tauri dev` for the real app.
- React 19 with the React Compiler and Zustand: follow the `vercel-react-best-practices` and `vercel-composition-patterns` skills. Types come from Rust (`src/ipc/generated`), never by hand.
- `apps/desktop/src-tauri` is a Cargo workspace of its own: run cargo there for it, and from the root for the core.
