#!/bin/sh
# Fails if any tracked file holds something shaped like a real credential
# (`ARC-20`, `PRD-06`): recorded fixtures, test data and code alike. The
# patterns are the providers' own token formats, so a made-up `ghp_test`
# passes and a pasted real token does not.
set -eu

PATTERNS='gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{50,}|glpat-[A-Za-z0-9_-]{20}|sk-ant-[A-Za-z0-9_-]{20,}|sk-(proj-)?[A-Za-z0-9]{40,}|xox[abposr]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY-----|ATATT[A-Za-z0-9_=-]{20,}|lin_api_[A-Za-z0-9]{40}|ATBB[A-Za-z0-9]{20,}'

# The redactor that hides credentials from screens and prompts: its own
# patterns, and the made-up credentials its tests hide.
ALLOWED='crates/muster-core/src/credentials.rs'

found=$(git ls-files -z | xargs -0 grep -nIE "$PATTERNS" -- 2>/dev/null |
    grep -v "^$ALLOWED:" || true)
if [ -n "$found" ]; then
    printf 'Something shaped like a real credential is in the repository:\n%s\n' "$found" >&2
    exit 1
fi
