#!/usr/bin/env bash
# Fails where an end-to-end test sleeps (ARC-30): it waits on what happens,
# through its support's `wait` (`until`, `found`, `holds`, `passes`), never
# a guess at how long that takes. Clippy refuses a sleep everywhere else
# (`clippy.toml`); these tests allow the lint for their own timing and
# threads, so this finds a sleep there instead.
set -euo pipefail

found=$(grep -rn --include='*.rs' -E '(std::thread|tokio::time|thread|time)::sleep\(' \
    crates/muster/tests apps/desktop/src-tauri/tests 2>/dev/null \
    | grep -v '/support/wait.rs:' || true)
if [[ -n "$found" ]]; then
    echo "$found" >&2
    echo "Wait through the tests' support (support/wait.rs), not a sleep (ARC-30)." >&2
    exit 1
fi
