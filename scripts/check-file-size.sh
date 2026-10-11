#!/usr/bin/env bash
# Fails when a Rust source file is longer than the limit (ARC-08).
set -euo pipefail

limit="${1:-1000}"
status=0
while IFS= read -r -d '' file; do
    lines=$(( $(wc -l < "$file") ))
    if (( lines > limit )); then
        echo "$file has $lines lines; the limit is $limit (ARC-08)." >&2
        status=1
    fi
done < <(find crates -name '*.rs' -not -path '*/target/*' -print0)
exit "$status"
