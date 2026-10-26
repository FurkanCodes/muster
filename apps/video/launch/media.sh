#!/bin/sh
# Writes the README's copies of the rendered clip (out/muster.mp4) into
# docs/media: a smaller MP4 to watch, with its sound, and a GIF that plays
# in the README itself (800 px, 10 fps, about 7 MB). Needs ffmpeg.
set -eu
cd "$(dirname "$0")"
media=../../../docs/media
mkdir -p "$media"
ffmpeg -loglevel error -y -i out/muster.mp4 \
  -c:v libx264 -crf 27 -preset slow -pix_fmt yuv420p -color_range tv \
  -c:a aac -b:a 128k -movflags +faststart "$media/muster.mp4"
ffmpeg -loglevel error -y -i out/muster.mp4 \
  -vf "fps=10,scale=800:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=128:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle" \
  "$media/muster.gif"
ls -l "$media"
