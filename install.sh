#!/bin/sh
# Installs or updates Muster:
#
#   curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/main/install.sh | sh
#
# Downloads the prebuilt binary for this machine from the latest GitHub
# release, checks it against its published SHA-256 checksum, and puts it in
# ~/.local/bin (MUSTER_INSTALL_DIR to change that). Where there is no
# prebuilt binary, it builds Muster from source with cargo instead. Running
# it again updates Muster.
set -eu

REPO="FurkanCodes/muster"
INSTALL_DIR="${MUSTER_INSTALL_DIR:-$HOME/.local/bin}"

say() { printf '%s\n' "$*"; }
fail() {
    printf 'error: %s\n' "$*" >&2
    exit 1
}

# The release's name for this machine, or nothing if there is none.
target() {
    case "$(uname -s)/$(uname -m)" in
    Darwin/arm64) echo aarch64-apple-darwin ;;
    Darwin/x86_64) echo x86_64-apple-darwin ;;
    Linux/x86_64) echo x86_64-unknown-linux-gnu ;;
    Linux/aarch64 | Linux/arm64) echo aarch64-unknown-linux-gnu ;;
    Darwin/* | Linux/*) ;;
    *) fail "Muster runs on macOS and Linux only" ;;
    esac
}

# The SHA-256 of file $1.
checksum() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | cut -d ' ' -f 1
    else
        shasum -a 256 "$1" | cut -d ' ' -f 1
    fi
}

# Downloads, checks and installs the release's binary for target $1,
# setting `installed`; returns 1 if it cannot be downloaded.
from_release() {
    archive="muster-$1.tar.gz"
    base="https://github.com/$REPO/releases/latest/download"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $base/$archive"
    curl -fsSL "$base/$archive" -o "$tmp/$archive" || return 1
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" || return 1
    expected=$(cut -d ' ' -f 1 <"$tmp/$archive.sha256")
    [ "$(checksum "$tmp/$archive")" = "$expected" ] ||
        fail "the download does not match its published checksum; nothing was installed"
    tar -xzf "$tmp/$archive" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/muster-$1/muster" "$INSTALL_DIR/muster"
    installed="$INSTALL_DIR/muster"
}

from_source() {
    command -v cargo >/dev/null 2>&1 ||
        fail "no prebuilt binary for this machine, and cargo is not installed: get Rust from https://rustup.rs and run this again"
    say "building Muster from source (a few minutes)…"
    cargo install --locked --force --git "https://github.com/$REPO" muster
    installed="${CARGO_HOME:-$HOME/.cargo}/bin/muster"
}

main() {
    command -v curl >/dev/null 2>&1 || fail "curl is needed"
    installed=""
    platform=$(target)
    # A download that fails falls back to building; one that does not match
    # its checksum stops everything (`fail`).
    if [ -n "$platform" ]; then
        from_release "$platform" || installed=""
    fi
    if [ -z "$installed" ]; then
        say "no prebuilt binary to download; building from source instead"
        from_source
    fi
    say "installed $("$installed" --version) at $installed"

    case ":$PATH:" in
    *":$(dirname "$installed"):"*) ;;
    *) say "add it to your PATH: export PATH=\"$(dirname "$installed"):\$PATH\"" ;;
    esac
    if pgrep -f 'muster daemon' >/dev/null 2>&1; then
        say "a Muster daemon is running the old version: 'muster reload' moves it onto this one, every session running on."
    fi
}

main
