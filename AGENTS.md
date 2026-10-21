# Repository agent instructions

- Read [PRODUCT.md](PRODUCT.md) (`PRD-*`) and [ARCHITECTURE.md](ARCHITECTURE.md) (`ARC-*`) before planning, writing or reviewing code. Cite the rule IDs a change touches in plans, commit messages, reviews and test names.
- When a change conflicts with a rule, stop and say so. Change the rule deliberately, or change the plan — never let code and rules drift.
- `muster-core`, `muster-state` and `muster-model` do no IO (`ARC-01`). Docker, GitHub, ticket sources, harnesses, PTYs and the clock are reached only through their traits (`ARC-03`).
- No source file over 1,000 lines (`ARC-08`). Split by responsibility before you get close.
- Every behaviour change comes with tests: unit tests for the state machines, parsers and scheduler; end-to-end tests against the real daemon binary with fakes for external systems.
- Before finishing: `cargo fmt --check`, `CARGO_BUILD_WARNINGS=deny cargo clippy --all-targets`, and the tests of what changed (`cargo nextest run -p <crate>`; CI runs them all).
- Never put a real credential in code, fixtures, logs or test output (`PRD-06`). Recorded agent fixtures are scrubbed before they are committed.
- Nebula (MIT) informed the design. If its code is ever copied, add its notice to `NOTICE` in the same change.

## Desktop app (`apps/desktop`)

- The plan, branches and who owns what: [apps/desktop/PLAN.md](apps/desktop/PLAN.md). The app is `PRODUCT.md` §6a and `ARCHITECTURE.md` §5c (`ARC-24`, `ARC-25`).
- `pnpm install` in `apps/desktop`, then `pnpm dev:mock` (the window over a pretend daemon, in a browser), `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm e2e`; `pnpm bindings` after changing a Rust type the webview uses; `pnpm tauri dev` for the real app.
- React 19 with the React Compiler and Zustand: follow the `vercel-react-best-practices` and `vercel-composition-patterns` skills. Types come from Rust (`src/ipc/generated`), never by hand.
- `apps/desktop/src-tauri` is a Cargo workspace of its own: run cargo there for it, and from the root for the core.
