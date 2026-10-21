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
# A step under way: said before it starts, so a slow one is never silent.
step() { printf '%s… ' "$*"; }
# How a step ended, on its line.
done_() { printf '%s\n' "$*"; }
fail() {
    printf '\nerror: %s\n' "$*" >&2
    exit 1
}

# $1 bytes in words: "8.4 MB".
size() {
    awk -v bytes="$1" 'BEGIN {
        if (bytes >= 1048576) printf "%.1f MB", bytes / 1048576
        else if (bytes >= 1024) printf "%.0f KB", bytes / 1024
        else printf "%d bytes", bytes
    }'
}

# The size of what URL $1 downloads, after its redirects, or nothing.
remote_size() {
    curl -fsSLI "$1" 2>/dev/null | tr -d '\r' |
        awk '/^HTTP\// { status = $2; length_ = "" }
            tolower($1) == "content-length:" { length_ = $2 }
            END { if (status == 200 && length_ > 0) print length_ }'
}

# Downloads URL $1 to $2: with a progress bar on a terminal, quietly
# otherwise (a log, a CI run).
fetch() {
    if [ -t 2 ]; then
        curl -fL --progress-bar "$1" -o "$2"
    else
        curl -fsSL "$1" -o "$2"
    fi
}

# What target $1 is, in words.
machine() {
    case "$1" in
    aarch64-apple-darwin) echo "macOS on Apple silicon" ;;
    x86_64-apple-darwin) echo "macOS on Intel" ;;
    x86_64-unknown-linux-gnu) echo "Linux on x86-64" ;;
    aarch64-unknown-linux-gnu) echo "Linux on ARM64" ;;
    *) echo "$(uname -s) on $(uname -m)" ;;
    esac
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
        done_ "skipped: the GitHub CLI (gh) is not installed, too old, or not signed in"
        return 0
    fi
    gh attestation verify "$1" --repo "$REPO" >/dev/null 2>&1 ||
        fail "the download could not be checked against the attestation GitHub made as it was built, so nothing was installed (MUSTER_SKIP_ATTESTATION=1 installs it with its checksum checked only)"
    done_ "built by $REPO's release workflow"
}

# Downloads, checks and installs release $2's binary for target $1,
# setting `installed`; returns 1 if it cannot be downloaded.
from_release() {
    archive="muster-$1.tar.gz"
    base="https://github.com/$REPO/releases/download/$2"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    bytes=$(remote_size "$base/$archive")
    say "Downloading $archive${bytes:+ ($(size "$bytes"))} from $base"
    if ! fetch "$base/$archive" "$tmp/$archive"; then
        say "The download failed."
        return 1
    fi
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" || {
        say "Its checksum could not be downloaded."
        return 1
    }
    step "Checking its SHA-256"
    expected=$(cut -d ' ' -f 1 <"$tmp/$archive.sha256")
    [ "$(checksum "$tmp/$archive")" = "$expected" ] ||
        fail "the download does not match its published checksum; nothing was installed"
    done_ "matches"
    step "Checking its build attestation"
    attested "$tmp/$archive"
    step "Unpacking"
    tar -xzf "$tmp/$archive" -C "$tmp"
    done_ "done"
    step "Installing it as $INSTALL_DIR/muster"
    mkdir -p "$INSTALL_DIR"
    # Into place under another name, then renamed over the old one: a
    # running daemon or UI keeps the file it started from.
    install -m 755 "$tmp/muster-$1/muster" "$INSTALL_DIR/.muster.new"
    mv -f "$INSTALL_DIR/.muster.new" "$INSTALL_DIR/muster"
    done_ "done"
    installed="$INSTALL_DIR/muster"
}

# Builds release $1 (or, if none could be told, the default branch).
from_source() {
    command -v cargo >/dev/null 2>&1 ||
        fail "no prebuilt binary for this machine, and cargo is not installed: get Rust from https://rustup.rs and run this again"
    say "Building Muster ${1:-from its default branch} from source with cargo: this takes a few minutes, and cargo's own progress follows."
    if [ -n "$1" ]; then
        cargo install --locked --force --git "https://github.com/$REPO" --tag "$1" muster
    else
        cargo install --locked --force --git "https://github.com/$REPO" muster
    fi
    installed="${CARGO_HOME:-$HOME/.cargo}/bin/muster"
}

# The version of the muster this install replaces, or nothing.
current() {
    if [ -x "$INSTALL_DIR/muster" ]; then
        "$INSTALL_DIR/muster" --version 2>/dev/null | awk '{ print $2; exit }'
    fi
}

main() {
    command -v curl >/dev/null 2>&1 || fail "curl is needed"
    installed=""
    platform=$(target)
    if [ -n "${MUSTER_VERSION:-}" ]; then step "Release asked for"; else step "Finding the latest release"; fi
    version=$(release)
    done_ "${version:-none found}"
    was=$(current)
    say "Installing Muster ${version:-from source} for $(machine "${platform:-}")${was:+, over muster $was}."
    # A download that fails falls back to building; one that does not match
    # its checksum or its attestation stops everything (`fail`).
    if [ -n "$platform" ] && [ -n "$version" ]; then
        from_release "$platform" "$version" || installed=""
    fi
    if [ -z "$installed" ]; then
        say "No prebuilt binary to download; building from source instead."
        from_source "$version"
    fi
    now=$("$installed" --version | awk '{ print $2 }')
    say ""
    if [ -n "$was" ] && [ "$was" != "$now" ]; then
        say "Muster $now is installed at $installed (it was $was)."
    else
        say "Muster $now is installed at $installed."
    fi

    case ":$PATH:" in
    *":$(dirname "$installed"):"*) ;;
    *) say "Add its folder to your PATH: export PATH=\"$(dirname "$installed"):\$PATH\"" ;;
    esac
    if pgrep -f 'muster daemon' >/dev/null 2>&1; then
        say "A Muster daemon is running the old version: 'muster reload' moves it onto this one, every session running on."
    else
        say "Run 'muster' to start."
    fi
}

main
