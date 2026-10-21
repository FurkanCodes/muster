#!/bin/sh
# Writes the TypeScript bindings the webview is typed by: the daemon's
# protocol (muster-core, feature `ts`) and the app's own contract
# (src-tauri/src/contract.rs). CI runs it and fails if the files change.
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
out="$here/src/ipc/generated"
rm -rf "$out"
mkdir -p "$out"
export TS_RS_EXPORT_DIR="$out" TS_RS_LARGE_INT=number
(cd "$here/../.." && cargo test --quiet --locked -p muster-core --features ts --lib export_bindings >/dev/null)
(cd "$here/src-tauri" && cargo test --quiet --lib export_bindings >/dev/null)
echo "bindings: $(find "$out" -name '*.ts' | wc -l | tr -d ' ') files in src/ipc/generated"
