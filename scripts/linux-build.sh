#!/bin/sh
# Builds this checkout's muster for Linux in a Docker container here, for
# trying a branch on a Linux machine before it is released: the machine
# needs nothing but ssh, as `muster ssh vm --install` sends it the binary
# MUSTER_REMOTE_BINARY names (ARC-27). Built on Ubuntu 22.04, so it runs on
# glibc 2.35 and later (Ubuntu 22.04+, Debian 12+). Docker caches the
# toolchain and the build, so the next one is quick.
#
#   scripts/linux-build.sh            # x86_64 (a PC, most VMs)
#   scripts/linux-build.sh aarch64    # ARM (a VM on a Mac, a Raspberry Pi)
set -eu
arch="${1:-x86_64}"
case "$arch" in
x86_64) platform=linux/amd64 ;;
aarch64) platform=linux/arm64 ;;
*) echo "usage: scripts/linux-build.sh [x86_64|aarch64]" >&2; exit 2 ;;
esac
command -v docker >/dev/null || { echo "Docker is needed: https://docs.docker.com/desktop/" >&2; exit 1; }
top="$(git rev-parse --show-toplevel)"
out="$top/target/linux"
mkdir -p "$out"
echo "Building muster for Linux on $arch in Docker (the first build takes a while)…"
# The source read-only; the toolchain and the build in volumes of their own.
docker run --rm --platform "$platform" \
  -v "$top:/src:ro" \
  -v "muster-linux-$arch-rustup:/root/.rustup" \
  -v "muster-linux-$arch-cargo:/root/.cargo" \
  -v "muster-linux-$arch-target:/target" \
  -v "$out:/out" \
  -e CARGO_TARGET_DIR=/target \
  -w /src \
  ubuntu:22.04 sh -euc '
    apt-get update -qq
    DEBIAN_FRONTEND=noninteractive apt-get install -y -qq build-essential cmake curl pkg-config >/dev/null
    [ -x /root/.cargo/bin/rustup ] ||
      curl -fsSL https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain none
    PATH="/root/.cargo/bin:$PATH"
    cargo build --release --locked -p muster
    install -m 755 /target/release/muster /out/muster-'"$arch"'
  '
binary="$out/muster-$arch"
echo "Built $binary"
echo "Put it on a machine with:"
echo "  MUSTER_REMOTE_BINARY=\"$binary\" muster ssh <machine> --install"
