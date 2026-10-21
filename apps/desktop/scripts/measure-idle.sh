#!/bin/zsh
# Measures the release app at rest (ARC-25): its CPU while seen and idle,
# then while hidden, which must be zero, and its memory. Run on a Mac after
# `pnpm tauri build --no-bundle`; it needs `muster` (MUSTER_BIN, else the
# core's target/debug/muster) and asks System Events to bring the window to
# the front and then hide it, which macOS may ask the person to allow once.
#
# The app runs over a daemon of its own, never the person's: a runtime
# folder under /tmp, the rest in a temporary folder, with a stand-in agent
# asking the person (a shell that says so in its title, as Codex does) and
# an idle shell. CPU is the time the app and the WebKit processes it started
# spent in each period (SECONDS_EACH, 60 by default), as `ps` counts it;
# memory is resident and, where `footprint` can tell, the footprint.
set -u
here=${0:A:h}
root=${here:h:h:h}
app=$here/../src-tauri/target/release/muster-desktop
export MUSTER_BIN=${MUSTER_BIN:-$root/target/debug/muster}
seconds=${SECONDS_EACH:-60}
[ -x "$app" ] || { echo "no release app: run pnpm tauri build --no-bundle" >&2; exit 1; }
[ -x "$MUSTER_BIN" ] || { echo "no muster at $MUSTER_BIN" >&2; exit 1; }

sandbox=$(mktemp -d "${TMPDIR:-/tmp}/muster-measure-XXXX")
runtime=$(mktemp -d /tmp/mdzXXXX)
mkdir -p $sandbox/bin $sandbox/repos/app
printf '#!/bin/sh\nprintf "\\033]0;[ ! ] Action Required\\007"\necho "Shall the stand-in go on?"\nexec /bin/sh\n' > $sandbox/bin/codex
chmod +x $sandbox/bin/codex
(
  cd $sandbox/repos/app && git init -q -b main && echo test > README.md && git add README.md &&
    GIT_CONFIG_GLOBAL=/dev/null git -c user.name=Test -c user.email=test@example.com commit -qm first
)
export MUSTER_RUNTIME_DIR=$runtime MUSTER_DATA_DIR=$sandbox/data MUSTER_CONFIG_DIR=$sandbox/config
export MUSTER_WORKTREES_DIR=$sandbox/worktrees PATH=$sandbox/bin:$PATH SHELL=/bin/sh
export SSH_CONNECTION=measure MUSTER_NOTCH=off MUSTER_KEYCHAIN=off
unset MUSTER_SESSION

webkit() { pgrep -f '^/System/Library/Frameworks/WebKit.framework/.*com.apple.WebKit.(WebContent|Networking|GPU)$' | sort }
before=$(webkit)
$app >$sandbox/app.log 2>&1 &
pid=$!
cleanup() {
  kill $pid 2>/dev/null
  sleep 1
  $MUSTER_BIN daemon stop >/dev/null 2>&1
  rm -rf $sandbox $runtime
}
trap cleanup EXIT
sleep 6
$MUSTER_BIN project add $sandbox/repos/app >/dev/null
$MUSTER_BIN session start app --harness codex --title "Stand-in agent" >/dev/null
$MUSTER_BIN session start app --harness shell >/dev/null
sleep 4
webkits=(${(f)"$(comm -13 <(echo $before) <(webkit))"})
echo "the app: pid $pid; the WebKit processes it started: ${webkits[*]}"

cpu() { # CPU seconds the given processes have used so far
  local total=0 time
  for p in "$@"; do
    time=$(ps -o time= -p $p 2>/dev/null | tr -d ' ')
    [ -n "$time" ] && total=$(python3 -c "
s = 0.0
for part in '$time'.split(':'): s = s * 60 + float(part)
print($total + s)")
  done
  echo $total
}
window() { osascript -e "tell application \"System Events\" to set $1 of (first process whose unix id is $pid) to $2" }
measure() {
  local a0=$(cpu $pid) w0=$(cpu $webkits)
  sleep $seconds
  local a1=$(cpu $pid) w1=$(cpu $webkits)
  python3 -c "
a, w, n = $a1 - $a0, $w1 - $w0, $seconds
print(f'  cpu: app {a:.2f} s in {n} s ({100*a/n:.3f}%), its WebKit {w:.2f} s ({100*w/n:.3f}%), together {100*(a+w)/n:.3f}%')"
}
memory() {
  local resident=0
  for p in $pid $webkits; do resident=$(( resident + $(ps -o rss= -p $p) )); done
  echo "  memory: app $(( $(ps -o rss= -p $pid) / 1024 )) MB resident, with its WebKit $(( resident / 1024 )) MB"
  for p in $pid $webkits; do
    footprint $p 2>/dev/null | grep -m1 'Footprint:' | sed 's/^/  /'
  done
}

window frontmost true
sleep 10
echo "seen and idle, $seconds s:"
measure
memory
window visible false
sleep 10
echo "hidden, $seconds s:"
measure
memory
