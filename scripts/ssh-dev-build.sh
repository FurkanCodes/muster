#!/bin/sh
# Builds this checkout's muster on a machine of yours, over ssh, and puts it
# where `muster ssh` looks (~/.local/bin/muster): for trying a branch there
# before it is released. The machine needs git's sources only as sent here,
# a C toolchain and Rust (https://rustup.rs). Your keys stay ssh's.
#
#   scripts/ssh-dev-build.sh you@192.168.1.20
set -eu
target="${1:?usage: scripts/ssh-dev-build.sh you@host}"
case "$target" in -*) echo "not an ssh target: $target" >&2; exit 2 ;; esac
top="$(git rev-parse --show-toplevel)"
echo "Sending $(git -C "$top" rev-parse --short HEAD) (with what is not committed) to $target…"
# The tracked files as they are now, uncommitted changes included.
git -C "$top" ls-files -z | (cd "$top" && tar --null -T - -czf -) |
  ssh -C -- "$target" 'rm -rf "$HOME/muster-src" && mkdir -p "$HOME/muster-src" && tar -xzf - -C "$HOME/muster-src"'
echo "Building there (the first build takes a while)…"
ssh -t -C -- "$target" 'set -e
  PATH="$HOME/.cargo/bin:$PATH"
  command -v cargo >/dev/null || { echo "Rust is not there: curl https://sh.rustup.rs -sSf | sh"; exit 1; }
  cd "$HOME/muster-src"
  cargo build --release --locked -p muster
  mkdir -p "$HOME/.local/bin"
  install -m 755 target/release/muster "$HOME/.local/bin/muster.part"
  mv "$HOME/.local/bin/muster.part" "$HOME/.local/bin/muster"
  "$HOME/.local/bin/muster" --version
  "$HOME/.local/bin/muster" reload 2>/dev/null || true'
echo "Done: \`muster ssh <machine> --check\` next."
