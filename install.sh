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
#
# On a Mac it also installs the Muster app, from the same release and
# checked the same way, in /Applications (or ~/Applications where that
# cannot be written; MUSTER_APP_DIR for another folder), unless told not
# to: MUSTER_APP=0, or `sh -s -- --no-app`. A Muster app already there is
# updated. The app is signed ad hoc, not by Apple: fetched
# this way it carries no quarantine, so macOS opens it without asking.
# MUSTER_DOWNLOADS points the downloads at a stand-in for GitHub's, for tests.
#
# On a terminal it speaks in colour (NO_COLOR for none); in a log, a CI run
# or over `muster ssh`, in plain lines.
set -eu

REPO="FurkanCodes/muster"
INSTALL_DIR="${MUSTER_INSTALL_DIR:-$HOME/.local/bin}"
APP_ARCHIVE="muster-app-universal-apple-darwin.tar.gz"

# Colour and symbols on a terminal that takes them; plain text elsewhere.
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != dumb ]; then
    bold=$(printf '\033[1m') dim=$(printf '\033[2m') reset=$(printf '\033[0m')
    green=$(printf '\033[32m') yellow=$(printf '\033[33m') red=$(printf '\033[31m')
    blue=$(printf '\033[34m') tty=1
else
    bold="" dim="" reset="" green="" yellow="" red="" blue="" tty=""
fi
case "${LC_ALL:-${LC_CTYPE:-${LANG:-}}}" in
*UTF-8* | *utf-8* | *UTF8* | *utf8*) ok="✓" skip="○" bad="✗" doing="•" kep="▼•ᴥ•▼" ;;
*) ok="ok" skip="--" bad="x" doing="*" kep="Muster" ;;
esac

say() { printf '%s\n' "$*"; }
# A detail under a step, quieter.
note() { printf '    %s%s%s\n' "$dim" "$*" "$reset"; }
# A step under way: said before it starts, so a slow one is never silent.
step() {
    step_text=$*
    printf '  %s%s%s %s… ' "$blue" "$doing" "$reset" "$*"
}
# How a step ended: on a terminal its line is said again, marked done.
done_() {
    if [ -n "$tty" ]; then
        printf '\r\033[K  %s%s%s %s %s%s%s\n' "$green" "$ok" "$reset" "$step_text" "$dim" "$*" "$reset"
    else
        printf '%s\n' "$*"
    fi
}
# How a step ended where it was left out.
skipped() {
    if [ -n "$tty" ]; then
        printf '\r\033[K  %s%s%s %s %s%s%s\n' "$yellow" "$skip" "$reset" "$step_text" "$dim" "$*" "$reset"
    else
        printf '%s\n' "$*"
    fi
}
fail() {
    printf '\n  %s%s error:%s %s\n' "$red$bold" "$bad" "$reset" "$*" >&2
    exit 1
}
# A heading for a part of the install.
part() { printf '\n%s%s%s\n' "$bold" "$*" "$reset"; }

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
    if [ -n "${MUSTER_SKIP_ATTESTATION:-}" ]; then
        skipped "skipped, as MUSTER_SKIP_ATTESTATION asks"
        return 0
    fi
    if ! command -v gh >/dev/null 2>&1 ||
        ! gh attestation verify --help >/dev/null 2>&1 || ! gh auth status >/dev/null 2>&1; then
        skipped "skipped: the GitHub CLI (gh) is not installed, too old, or not signed in"
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
    base="${MUSTER_DOWNLOADS:-https://github.com/$REPO/releases/download}/$2"
    tmp=$(mktemp -d)
    trap 'rm -rf "${tmp:-}" "${app_tmp:-}"' EXIT
    bytes=$(remote_size "$base/$archive")
    part "Muster in the terminal"
    say "  ${blue}${doing}${reset} Downloading $archive${bytes:+ ${dim}$(size "$bytes")${reset}}"
    note "from $base"
    if ! fetch "$base/$archive" "$tmp/$archive"; then
        say "  ${yellow}${skip}${reset} The download failed."
        return 1
    fi
    curl -fsSL "$base/$archive.sha256" -o "$tmp/$archive.sha256" || {
        say "  ${yellow}${skip}${reset} Its checksum could not be downloaded."
        return 1
    }
    step "Checksum"
    expected=$(cut -d ' ' -f 1 <"$tmp/$archive.sha256")
    [ "$(checksum "$tmp/$archive")" = "$expected" ] ||
        fail "the download does not match its published checksum; nothing was installed"
    done_ "SHA-256 matches"
    step "Build attestation"
    attested "$tmp/$archive"
    step "Unpacking"
    tar -xzf "$tmp/$archive" -C "$tmp"
    done_ "done"
    step "Installing $INSTALL_DIR/muster"
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
    part "Muster in the terminal, from source"
    note "Building ${1:-its default branch} with cargo: this takes a few minutes, and cargo's own progress follows."
    if [ -n "$1" ]; then
        cargo install --locked --force --git "https://github.com/$REPO" --tag "$1" muster
    else
        cargo install --locked --force --git "https://github.com/$REPO" muster
    fi
    installed="${CARGO_HOME:-$HOME/.cargo}/bin/muster"
}

# Where Muster.app is now, or nothing.
app_now() {
    for dir in "${MUSTER_APP_DIR:-}" /Applications "$HOME/Applications"; do
        if [ -n "$dir" ] && [ -d "$dir/Muster.app" ]; then
            echo "$dir/Muster.app"
            return
        fi
    done
}

# The version app $1 says it is.
app_version() {
    /usr/libexec/PlistBuddy -c "Print CFBundleShortVersionString" "$1/Contents/Info.plist" 2>/dev/null || true
}

# Sets `app` to 1 to install the Muster app, 0 not to: on a Mac, always,
# unless the command line (`$1`: 1, 0 or nothing) or MUSTER_APP says not.
choose_app() {
    app=0
    [ "$(uname -s)" = Darwin ] || return 0
    app=${1:-${MUSTER_APP:-1}}
}

# Downloads, checks and installs release $1's Muster app, setting
# `app_installed`; says so and installs nothing if the release has none.
install_app() {
    base="${MUSTER_DOWNLOADS:-https://github.com/$REPO/releases/download}/$1"
    bytes=$(remote_size "$base/$APP_ARCHIVE")
    part "The Muster app"
    if [ -z "$bytes" ]; then
        say "  ${yellow}${skip}${reset} Release $1 has no Muster app to download: it comes with a later one."
        return 0
    fi
    app_tmp=$(mktemp -d)
    trap 'rm -rf "${tmp:-}" "${app_tmp:-}"' EXIT
    say "  ${blue}${doing}${reset} Downloading $APP_ARCHIVE ${dim}$(size "$bytes")${reset}"
    note "from $base"
    fetch "$base/$APP_ARCHIVE" "$app_tmp/$APP_ARCHIVE" ||
        fail "the Muster app could not be downloaded; Muster in the terminal is installed (MUSTER_APP=0 skips the app)"
    curl -fsSL "$base/$APP_ARCHIVE.sha256" -o "$app_tmp/$APP_ARCHIVE.sha256" ||
        fail "the Muster app's checksum could not be downloaded, so the app was not installed"
    step "Checksum"
    expected=$(cut -d ' ' -f 1 <"$app_tmp/$APP_ARCHIVE.sha256")
    [ "$(checksum "$app_tmp/$APP_ARCHIVE")" = "$expected" ] ||
        fail "the Muster app does not match its published checksum, so it was not installed"
    done_ "SHA-256 matches"
    step "Build attestation"
    attested "$app_tmp/$APP_ARCHIVE"
    step "Unpacking"
    tar -xzf "$app_tmp/$APP_ARCHIVE" -C "$app_tmp"
    done_ "done"
    there=$(app_now)
    if [ -n "$there" ]; then
        dest=$(dirname "$there")
    elif [ -n "${MUSTER_APP_DIR:-}" ]; then
        dest=$MUSTER_APP_DIR
    elif [ -w /Applications ]; then
        dest=/Applications
    else
        dest="$HOME/Applications"
    fi
    step "Installing $dest/Muster.app"
    mkdir -p "$dest"
    rm -rf "$dest/.Muster.app.new" "$dest/.Muster.app.old"
    # Beside the old one first, then swapped: a Muster app open now runs on
    # from the files it started with.
    mv "$app_tmp/Muster.app" "$dest/.Muster.app.new"
    if [ -d "$dest/Muster.app" ]; then
        mv "$dest/Muster.app" "$dest/.Muster.app.old"
    fi
    mv "$dest/.Muster.app.new" "$dest/Muster.app"
    rm -rf "$dest/.Muster.app.old"
    # A copy fetched by curl carries no quarantine; this makes sure.
    xattr -dr com.apple.quarantine "$dest/Muster.app" 2>/dev/null || true
    done_ "done"
    app_installed="$dest/Muster.app"
}

# The version of the muster this install replaces, or nothing.
current() {
    if [ -x "$INSTALL_DIR/muster" ]; then
        "$INSTALL_DIR/muster" --version 2>/dev/null | awk '{ print $2; exit }'
    fi
}

main() {
    command -v curl >/dev/null 2>&1 || fail "curl is needed"
    app_flag=""
    for argument in "$@"; do
        case "$argument" in
        --app) app_flag=1 ;;
        --no-app) app_flag=0 ;;
        *) fail "unknown option $argument (--no-app)" ;;
        esac
    done
    installed=""
    app_installed=""
    platform=$(target)
    printf '\n  %s%s%s  %sMuster%s %sinstaller%s\n\n' "$bold" "$kep" "$reset" "$bold" "$reset" "$dim" "$reset"
    if [ -n "${MUSTER_VERSION:-}" ]; then step "Release asked for"; else step "Latest release"; fi
    version=$(release)
    done_ "${version:-none found}"
    was=$(current)
    choose_app "$app_flag"
    app_was=$(app_now)
    app_was=${app_was:+$(app_version "$app_was")}
    if [ "$app" = 1 ]; then what="muster in the terminal and the Muster app"; else what="muster in the terminal"; fi
    note "${version:-from source} for $(machine "${platform:-}"): $what${was:+, over muster $was}"
    # A download that fails falls back to building; one that does not match
    # its checksum or its attestation stops everything (`fail`).
    if [ -n "$platform" ] && [ -n "$version" ]; then
        from_release "$platform" "$version" || installed=""
    fi
    if [ -z "$installed" ]; then
        note "No prebuilt binary to download; building from source instead."
        from_source "$version"
    fi
    if [ "$app" = 1 ]; then
        if [ -n "$version" ]; then
            install_app "$version"
        else
            note "No release to take the Muster app from: Muster in the terminal is installed alone."
        fi
    fi
    now=$("$installed" --version | awk '{ print $2 }')
    printf '\n  %s%s Muster %s is ready.%s' "$green$bold" "$ok" "$now" "$reset"
    if [ -n "$was" ] && [ "$was" != "$now" ]; then printf ' %s(it was %s)%s' "$dim" "$was" "$reset"; fi
    printf '\n\n'
    say "    ${bold}muster${reset}       ${dim}$installed${reset}"
    if [ -n "$app_installed" ]; then
        app_now_version=$(app_version "$app_installed")
        say "    ${bold}Muster.app${reset}   ${dim}$app_installed${app_now_version:+ ($app_now_version)}${app_was:+, it was $app_was}${reset}"
    fi
    say ""
    case ":$PATH:" in
    *":$(dirname "$installed"):"*) ;;
    *) say "  Add its folder to your PATH: ${bold}export PATH=\"$(dirname "$installed"):\$PATH\"${reset}" ;;
    esac
    if pgrep -f 'muster daemon' >/dev/null 2>&1; then
        say "  A Muster daemon is running the old version: ${bold}muster reload${reset} moves it onto this one, every session running on."
    elif [ -n "$app_installed" ]; then
        say "  Run ${bold}muster${reset} in a terminal, or open the Muster app from Launchpad or Spotlight."
    else
        say "  Run ${bold}muster${reset} to start."
    fi
    say ""
}

main "$@"
