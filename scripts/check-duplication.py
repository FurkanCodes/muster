#!/usr/bin/env python3
"""Fails where the same code is written twice (ARC-31): a run of WINDOW
lines that says the same thing in two places, of the core's Rust, the
desktop app's Rust and its TypeScript. Tests, generated code and what is
only punctuation, imports or comments are left out; lines are compared
with their spacing made one. What two places share goes in one, called
from both."""

import hashlib
import os
import re
import sys

WINDOW = int(sys.argv[1]) if len(sys.argv) > 1 else 12
ROOTS = [
    ("crates", (".rs",)),
    ("apps/desktop/src-tauri/src", (".rs",)),
    ("apps/desktop/src", (".ts", ".tsx")),
]
SKIPPED = re.compile(r"(_tests\.rs|/tests\.rs|/tests/|\.test\.tsx?|/generated/|/mock/|/target/)")
TRIVIAL = re.compile(r"^[\s{}()\[\];,.<>/]*$|^(use|import|export \{|//|/\*|\*|#\[)")


def lines_of(path):
    kept = []
    with open(path, encoding="utf-8", errors="replace") as text:
        for number, line in enumerate(text, 1):
            if path.endswith(".rs") and line.startswith("#[cfg(test)]"):
                break
            stripped = line.strip()
            if stripped and not TRIVIAL.match(stripped):
                kept.append((number, re.sub(r"\s+", " ", stripped)))
    return kept


def main():
    seen = {}
    found = []
    for root, endings in ROOTS:
        for folder, _, files in os.walk(root):
            for name in sorted(files):
                path = os.path.join(folder, name)
                if not name.endswith(endings) or SKIPPED.search(path):
                    continue
                kept = lines_of(path)
                # A run of windows each seen before is told once, at its start.
                running = False
                for at in range(len(kept) - WINDOW + 1):
                    window = "\n".join(line for _, line in kept[at : at + WINDOW])
                    key = hashlib.sha256(window.encode()).hexdigest()
                    first = seen.setdefault(key, (path, kept[at][0]))
                    again = first != (path, kept[at][0])
                    if again and not running:
                        found.append(f"{path}:{kept[at][0]} says what {first[0]}:{first[1]} says")
                    running = again
    for line in found:
        print(line, file=sys.stderr)
    if found:
        print(f"Write it once and call it from both (ARC-31, {WINDOW} lines).", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
