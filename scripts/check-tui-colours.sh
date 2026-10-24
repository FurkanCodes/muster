#!/usr/bin/env bash
# Fails where the TUI names a colour of its own (ARC-29): it draws in its
# theme's roles (`view/theme.rs`), so Muster's palette and the terminal's
# both hold. Clippy refuses the `Stylize` colours (`clippy.toml`); this
# finds a `Color::` named outside the theme, Kep's picture and the agents'
# logos, whose colours are their own. `Color::Reset`, the terminal's own,
# is not a colour of Muster's. Tests are left out.
set -euo pipefail

status=0
while IFS= read -r -d '' file; do
    case "$file" in
        */view/theme.rs | */view/kep.rs | */view/logos.rs | *_tests.rs) continue ;;
    esac
    found=$(awk '/^#\[cfg\(test\)\]/ { exit } /Color::[A-Z]/ && !/Color::Reset/ { print FILENAME ":" FNR ": " $0 }' "$file")
    if [[ -n "$found" ]]; then
        echo "$found" >&2
        status=1
    fi
done < <(find crates/muster-tui/src -name '*.rs' -print0)
if (( status )); then
    echo "Draw in the theme's roles (view/theme.rs), not a colour of its own (ARC-29)." >&2
fi
exit "$status"
