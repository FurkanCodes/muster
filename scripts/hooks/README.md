# scripts/hooks

## Overview

Claude Code hooks for agents working in this repository. Registered in
`.claude/settings.json` as a `PreToolUse` hook on the `Bash` tool:
`python3 -I "$CLAUDE_PROJECT_DIR"/scripts/hooks/whole-suite.py`.

## Interface

- `whole-suite.py` — reads the hook event JSON on stdin; if the Bash command would run the core workspace's whole test suite, prints a `hookSpecificOutput` with `permissionDecision: "ask"` and a reason pointing at CLAUDE.md's *Which tests to run*; otherwise prints nothing.
- `whole_suite(command, cwd)` (whole-suite.py) — the decision: a `cargo test` / `cargo nextest run` segment with `--workspace` / `--all`, or with no `-p` / `--package` / `--manifest-path`, is the whole suite.

## Invariants

- Only asks, never denies: the person decides.
- Not the whole suite: anything in or about `apps/desktop` (its own Cargo workspace), any segment with `--no-run`, and commands that are not cargo test runs.
- The command is split on `&&`, `||`, `;`, `|` and newlines; each segment is judged on its own.
- Malformed stdin JSON is ignored (no output, exit 0).

## Side Effects

- Reads stdin, writes the decision JSON to stdout. No files, no network.

## Verification

- No test runner. Try it by hand: `echo '{"tool_input":{"command":"cargo nextest run"},"cwd":"."}' | python3 -I scripts/hooks/whole-suite.py` (prints an `ask` decision); with `-p muster-state` it prints nothing.
