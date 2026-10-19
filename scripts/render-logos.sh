#!/bin/sh
# Renders the agents' logos the terminal UI draws (crates/muster-tui/
# assets/logos, whose README says where each comes from) from their SVGs
# into 64-pixel PNG files. Needs librsvg's `rsvg-convert`.
set -eu

cd "$(dirname "$0")/../crates/muster-tui/assets/logos"
for logo in claude codex gemini opencode copilot; do
    rsvg-convert --width 64 --height 64 "$logo.svg" --output "$logo.png"
done
