#!/usr/bin/env python3
"""A Claude Code PreToolUse hook: asks the person before the whole test
suite runs (CLAUDE.md, *Which tests to run*).

A `cargo test` or `cargo nextest run` in the core's workspace with no
`-p`/`--package` (or with `--workspace`/`--all`) is the whole suite; the
desktop app's own workspace (`apps/desktop/src-tauri`) and builds with
`--no-run` are not. Anything else passes without a word.
"""
import json
import re
import shlex
import sys

TEST = re.compile(r"(?:^|[\s/])cargo\s+(?:\+\S+\s+)?(?:test|nextest\s+run)\b")


def whole_suite(command, cwd):
    if "apps/desktop" in command or "apps/desktop" in cwd:
        return False
    for segment in re.split(r"&&|\|\||;|\||\n", command):
        if not TEST.search(segment):
            continue
        try:
            words = shlex.split(segment)
        except ValueError:
            words = segment.split()
        if "--no-run" in words:
            continue
        if "--workspace" in words or "--all" in words:
            return True
        scoped = any(
            w in ("-p", "--package", "--manifest-path")
            or w.startswith(("--package=", "--manifest-path="))
            or (w.startswith("-p") and not w.startswith("--"))
            for w in words
        )
        if not scoped:
            return True
    return False


def main():
    try:
        event = json.load(sys.stdin)
    except ValueError:
        return
    command = (event.get("tool_input") or {}).get("command") or ""
    if not whole_suite(command, event.get("cwd") or ""):
        return
    json.dump(
        {
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "ask",
                "permissionDecisionReason": (
                    "This runs the whole test suite. CLAUDE.md (Which tests to run) keeps it "
                    "for a protocol change, an update, a release or when asked; run the "
                    "changed crate's tests and its end-to-end files instead."
                ),
            }
        },
        sys.stdout,
    )


main()
