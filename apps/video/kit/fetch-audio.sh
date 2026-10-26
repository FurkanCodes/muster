#!/bin/sh
# Fetches the sound kit from Mixkit into kit/audio, by the names the clip
# uses. They are Mixkit's recordings, free to use under its licenses
# (https://mixkit.co/license/), and fetched here rather than kept in git,
# as those licenses do not allow passing the files on as they are.
# AUDIO.md says what each is and where it was measured to peak.
set -eu
cd "$(dirname "$0")/audio"

fetch() {
  [ -s "$1.mp3" ] && return 0
  curl -fsSL -A "Mozilla/5.0" -o "$1.mp3" "$2"
  echo "fetched $1.mp3"
}

fetch music https://assets.mixkit.co/music/471/471.mp3
fetch click https://assets.mixkit.co/active_storage/sfx/1117/1117-preview.mp3
fetch typing https://assets.mixkit.co/active_storage/sfx/1396/1396-preview.mp3
fetch whoosh https://assets.mixkit.co/active_storage/sfx/1490/1490-preview.mp3
fetch pop https://assets.mixkit.co/active_storage/sfx/2358/2358-preview.mp3
fetch ding https://assets.mixkit.co/active_storage/sfx/933/933-preview.mp3
fetch success https://assets.mixkit.co/active_storage/sfx/2865/2865-preview.mp3
fetch impact https://assets.mixkit.co/active_storage/sfx/2902/2902-preview.mp3
fetch rewind https://assets.mixkit.co/active_storage/sfx/1092/1092-preview.mp3
