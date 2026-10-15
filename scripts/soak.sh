#!/bin/sh
# Runs a daemon in a sandbox of its own under a steady churn of clients
# and sessions, and samples what it holds: its memory, and `muster
# status`'s counts of tasks, work, clients and terminals. A leak shows as a
# number that only climbs (ARC-13, ARC-14); memory that rises and levels
# off is the allocator, not a leak.
#
#   scripts/soak.sh [minutes] [muster binary]
#
# Defaults: 10 minutes, target/release/muster (`cargo build --release`).
# It never touches your own daemon: everything is under a temporary folder.
set -eu

minutes="${1:-10}"
muster="${2:-target/release/muster}"
[ -x "$muster" ] || {
    echo "no $muster; build it with: cargo build --release" >&2
    exit 1
}
muster=$(cd "$(dirname "$muster")" && pwd)/$(basename "$muster")

# Short: a socket's path must fit in about 100 bytes.
base=$(mktemp -d "${TMPDIR:-/tmp}/msoak.XXXXXX")
export MUSTER_RUNTIME_DIR="$base/r" MUSTER_DATA_DIR="$base/d" \
    MUSTER_CONFIG_DIR="$base/c" MUSTER_WORKTREES_DIR="$base/w" \
    MUSTER_KEYCHAIN=off SHELL=/bin/sh
cleanup() {
    "$muster" daemon stop >/dev/null 2>&1 || true
    rm -rf "$base"
}
trap cleanup EXIT INT TERM

git init -q "$base/repo"
git -C "$base/repo" -c user.name=soak -c user.email=soak@example.com \
    commit -q --allow-empty -m first

"$muster" daemon >"$base/daemon.log" 2>&1 &
daemon=$!
tries=0
until "$muster" status >/dev/null 2>&1; do
    tries=$((tries + 1))
    [ "$tries" -lt 100 ] || {
        echo "the daemon did not start; its log:" >&2
        cat "$base/daemon.log" >&2
        exit 1
    }
    sleep 0.1
done
"$muster" project add "$base/repo" --name soak >/dev/null

sample() {
    rss=$(ps -o rss= -p "$daemon" | tr -d ' ')
    inside=$("$muster" status | sed -n 's/^Inside: //p')
    printf '%6ss  %8s KB  %s\n' "$1" "$rss" "$inside"
}

echo "elapsed      memory  inside"
start=$(date +%s)
end=$((start + minutes * 60))
next=$start
while [ "$(date +%s)" -lt "$end" ]; do
    # A session that starts, runs a moment and ends, as the person's do,
    # and clients that come and go, as the UI and commands do.
    session=$("$muster" session start soak --title churn | grep -o 'ses_[0-9A-Z]*' | head -n 1)
    "$muster" status >/dev/null
    sleep 0.2
    "$muster" session stop "$session" >/dev/null 2>&1 || true
    # Taken off the list once it has ended, as the person would.
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        "$muster" session remove "$session" >/dev/null 2>&1 && break
        sleep 0.1
    done
    now=$(date +%s)
    if [ "$now" -ge "$next" ]; then
        sample $((now - start))
        next=$((now + 30))
    fi
done
# Settled: what is left once the churn stops should be what was there
# before it began.
sleep 3
sample $(($(date +%s) - start))
