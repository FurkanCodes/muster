# Repository agent instructions

- Read [PRODUCT.md](PRODUCT.md) (`PRD-*`) and [ARCHITECTURE.md](ARCHITECTURE.md) (`ARC-*`) before planning, writing or reviewing code. Cite the rule IDs a change touches in plans, commit messages, reviews and test names.
- When a change conflicts with a rule, stop and say so. Change the rule deliberately, or change the plan — never let code and rules drift.
- `muster-core` and `muster-state` do no IO (`ARC-01`). Docker, GitHub, ticket sources, harnesses, PTYs and the clock are reached only through their traits (`ARC-03`).
- No source file over 1,000 lines (`ARC-08`). Split by responsibility before you get close.
- Every behaviour change comes with tests: unit tests for the state machines, parsers and scheduler; end-to-end tests against the real daemon binary with fakes for external systems.
- Before finishing: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
- Never put a real credential in code, fixtures, logs or test output (`PRD-06`). Recorded agent fixtures are scrubbed before they are committed.
- Nebula (MIT) is reference material in `docs/research/`. Learn from it; if code is ever copied, add its notice to `NOTICE` in the same change.
