#!/bin/sh
# Renders the agents' logos the terminal UI draws (crates/muster-tui/
# assets/logos, whose README says where each comes from) from their SVGs
# into 64-pixel PNG files, and each PNG's pixels, unpacked, into a .rgba
# file beside it, which the UI lays on the background of the line it is
# drawn on. Needs librsvg's `rsvg-convert` and python3.
set -eu

cd "$(dirname "$0")/../crates/muster-tui/assets/logos"
for logo in claude codex gemini opencode copilot; do
    rsvg-convert --width 64 --height 64 "$logo.svg" --output "$logo.png"
    python3 -I - "$logo.png" "$logo.rgba" <<'PY'
import struct, sys, zlib

data = open(sys.argv[1], "rb").read()
assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
at, idat = 8, b""
while at < len(data):
    (length,) = struct.unpack(">I", data[at : at + 4])
    kind, body = data[at + 4 : at + 8], data[at + 8 : at + 8 + length]
    if kind == b"IHDR":
        width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", body)
        assert (depth, colour, interlace) == (8, 6, 0), "8-bit RGBA, not interlaced"
    elif kind == b"IDAT":
        idat += body
    at += 12 + length
raw, stride, out = zlib.decompress(idat), width * 4, bytearray()
previous, at = bytearray(stride), 0
for _ in range(height):
    kind, line = raw[at], bytearray(raw[at + 1 : at + 1 + stride])
    at += 1 + stride
    for x in range(stride):
        a = line[x - 4] if x >= 4 else 0
        b = previous[x]
        c = previous[x - 4] if x >= 4 else 0
        if kind == 1:
            line[x] = (line[x] + a) & 255
        elif kind == 2:
            line[x] = (line[x] + b) & 255
        elif kind == 3:
            line[x] = (line[x] + (a + b) // 2) & 255
        elif kind == 4:
            pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
            line[x] = (line[x] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
    out += line
    previous = line
open(sys.argv[2], "wb").write(bytes(out))
PY
done
