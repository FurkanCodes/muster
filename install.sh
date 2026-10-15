#!/bin/sh
# Installs or updates Muster:
#
#   curl -fsSL https://raw.githubusercontent.com/FurkanCodes/muster/main/install.sh | sh
#
# Downloads the prebuilt binary for this machine from the latest GitHub
# release (MUSTER_VERSION=v0.3.15 for another), checks it against its
# published SHA-256 checksum and, where the GitHub CLI is signed in, against
# the attestation GitHub made as the release workflow built it, and puts it
# in ~/.local/bin (MUSTER_INSTALL_DIR to change that). Where there is no
# prebuilt binary, it builds that same release from source with cargo
# instead. Running it again updates Muster.
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

# The release to install: MUSTER_VERSION, or the tag the latest one has,
# so the download, its checksum and any build from source are of one
# release. Nothing if it cannot be told.
release() {
    if [ -n "${MUSTER_VERSION:-}" ]; then
        echo "$MUSTER_VERSION"
        return
    fi
    latest=$(curl -fsSLI -o /dev/null -w '%{url_effective}' \
        "https://github.com/$REPO/releases/latest" 2>/dev/null) || return 0
    case "$latest" in
    */releases/tag/v*) echo "${latest##*/}" ;;
    esac
}

# Checks archive $1 against the attestation GitHub made as the release
# workflow built it, where the GitHub CLI can: proof it was built from this
# repository, beyond the checksum, which comes from the same release.
attested() {
    if [ -n "${MUSTER_SKIP_ATTESTATION:-}" ] || ! command -v gh >/dev/null 2>&1 ||
        ! gh attestation verify --help >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
        say "not checked against its build attestation: the GitHub CLI (gh) is not installed, too old, or not signed in"
        return 0
    fi
    gh attestation verify "$1" --repo "$REPO" >/dev/null 2>&1 ||
        fail "the download could not be checked against the attestation GitHub made as it was built, so nothing was installed (MUSTER_SKIP_ATTESTATION=1 installs it with its checksum checked only)"
    say "checked: built by $REPO's release workflow"
}

# Downloads, checks and installs release $2's binary for target $1,
# setting `installed`; returns 1 if it cannot be downloaded.
from_release() {
    archive="muster-$1.tar.gz"
    base="https://github.com/$REPO/releases/download/$2"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    say "downloading $base/$archive"
    curl -fsSL "$base/$archive" -o "$tmp/$archive" || return 1
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" || return 1
    expected=$(cut -d ' ' -f 1 <"$tmp/$archive.sha256")
    [ "$(checksum "$tmp/$archive")" = "$expected" ] ||
        fail "the download does not match its published checksum; nothing was installed"
    attested "$tmp/$archive"
    tar -xzf "$tmp/$archive" -C "$tmp"
    mkdir -p "$INSTALL_DIR"
    install -m 755 "$tmp/muster-$1/muster" "$INSTALL_DIR/muster"
    installed="$INSTALL_DIR/muster"
}

# Builds release $1 (or, if none could be told, the default branch).
from_source() {
    command -v cargo >/dev/null 2>&1 ||
        fail "no prebuilt binary for this machine, and cargo is not installed: get Rust from https://rustup.rs and run this again"
    say "building Muster ${1:-from its default branch} from source (a few minutes)…"
    if [ -n "$1" ]; then
        cargo install --locked --force --git "https://github.com/$REPO" --tag "$1" muster
    else
        cargo install --locked --force --git "https://github.com/$REPO" muster
    fi
    installed="${CARGO_HOME:-$HOME/.cargo}/bin/muster"
}

main() {
    command -v curl >/dev/null 2>&1 || fail "curl is needed"
    installed=""
    platform=$(target)
    version=$(release)
    # A download that fails falls back to building; one that does not match
    # its checksum or its attestation stops everything (`fail`).
    if [ -n "$platform" ] && [ -n "$version" ]; then
        from_release "$platform" "$version" || installed=""
    fi
    if [ -z "$installed" ]; then
        say "no prebuilt binary to download; building from source instead"
        from_source "$version"
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
