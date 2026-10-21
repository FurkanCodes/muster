#!/usr/bin/env bash
# Fails when a source file is longer than the limit (ARC-08): the core's
# Rust, and the desktop app's Rust and TypeScript (bindings excluded, being
# generated).
set -euo pipefail

limit="${1:-1000}"
status=0
while IFS= read -r -d '' file; do
    lines=$(( $(wc -l < "$file") ))
    if (( lines > limit )); then
        echo "$file has $lines lines; the limit is $limit (ARC-08)." >&2
        status=1
    fi
done < <(
    find crates -name '*.rs' -not -path '*/target/*' -print0
    find apps/desktop/src-tauri/src -name '*.rs' -print0
    find apps/desktop/src apps/desktop/e2e \( -name '*.ts' -o -name '*.tsx' \) \
        -not -path '*/generated/*' -print0
)
exit "$status"
