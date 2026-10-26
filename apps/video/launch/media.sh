#!/bin/sh
# Writes the README's copies of a rendered cut into docs/media: a smaller
# MP4 to watch, with its sound, and a GIF that plays in the README itself
# (800 px, 10 fps). The desktop app's cut (out/muster.mp4) becomes
# muster.gif and muster.mp4; the terminal UI's (out/tui/muster.mp4),
# muster-tui.gif and muster-tui.mp4. Needs ffmpeg.
#
#   ./launch/media.sh [desktop|tui]
set -eu
cd "$(dirname "$0")"
cut=${1:-desktop}
case "$cut" in
  desktop) from=out/muster.mp4; name=muster ;;
  tui) from=out/tui/muster.mp4; name=muster-tui ;;
  *) echo "no cut $cut: desktop or tui" >&2; exit 1 ;;
esac
media=../../../docs/media
mkdir -p "$media"
ffmpeg -loglevel error -y -i "$from" \
  -c:v libx264 -crf 27 -preset slow -pix_fmt yuv420p -color_range tv \
  -c:a aac -b:a 128k -movflags +faststart "$media/$name.mp4"
ffmpeg -loglevel error -y -i "$from" \
  -vf "fps=10,scale=800:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
  "$media/$name.gif"
ls -l "$media/$name.mp4" "$media/$name.gif"
