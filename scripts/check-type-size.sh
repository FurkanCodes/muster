#!/usr/bin/env bash
# Fails when a struct holds more fields than the limit (ARC-08): a type
# that keeps everything is split by what goes together, as the
# Supervisor's parts and the TUI's screens are, before it gets close.
set -euo pipefail

limit="${1:-50}"
status=0
while IFS= read -r -d '' file; do
    while IFS= read -r found; do
        echo "$found" >&2
        status=1
    done < <(awk -v limit="$limit" -v file="$file" '
        /^[[:space:]]*(pub(\([a-z]+\))? )?struct [A-Za-z0-9_]+(<[^>]*>)? *\{[[:space:]]*$/ {
            name = $0; sub(/^.*struct /, "", name); sub(/[<{ ].*/, "", name)
            depth = 1; fields = 0; inside = 1; next
        }
        inside {
            if (depth == 1 && $0 ~ /^[[:space:]]*(pub(\([a-z]+\))? )?[a-z_][a-z0-9_]*: /) fields++
            depth += gsub(/\{/, "{") - gsub(/\}/, "}")
            if (depth <= 0) {
                if (fields > limit) printf "%s: struct %s has %d fields; the limit is %d (ARC-08).\n", file, name, fields, limit
                inside = 0
            }
        }' "$file")
done < <(
    find crates -name '*.rs' -not -path '*/target/*' -print0
    find apps/desktop/src-tauri/src -name '*.rs' -print0
)
exit "$status"
