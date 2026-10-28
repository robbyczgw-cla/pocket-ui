#!/bin/sh
# The pictures of README.md and docs/media/: snapshots of pocket-ui with the made-up library of
# scripts/curator-fake-library.py, scaled to 960 px and stored as compressed PNG (each under 300 KB).
#   scripts/readme-shots.sh [OUT]        (default: docs/media)
# BIN=path/to/pocket-ui uses a binary you already built; otherwise `cargo build --release --bin pocket-ui` runs first.
# Needs python3 and ImageMagick's `convert`. On Linux without a compositor, start scripts/wayland-stub.py first
# (docs/DEVELOPING.md, "Tests").
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/docs/media}
FAKE=${TMPDIR:-/tmp}/pocketui-readme-fake
RAW=${TMPDIR:-/tmp}/pocketui-readme-raw
command -v convert >/dev/null || { echo "readme-shots: ImageMagick's convert is needed" >&2; exit 1; }
if [ -z "${BIN:-}" ]; then
  BIN=$ROOT/target/release/pocket-ui
  (cd "$ROOT" && cargo build --release -q --bin pocket-ui)
fi
mkdir -p "$OUT" "$RAW"

# name  look  state
shots() {
  cat <<'EOT'
home dark curator:home
home-light light curator:home
home-split dark curator:home:split
home-cabinet dark curator:cabinet:dd
game-card dark curator:card:snes:chrono
lens dark curator:lens:dddddra
echoes dark curator:echoes:snes:chrono
pause-menu dark menu:pocket-core
save-slots dark slot-save
settings dark settings
EOT
}

shots | while read -r name look state; do
  python3 -I "$ROOT/scripts/curator-fake-library.py" "$FAKE" "$look" >/dev/null
  POCKETUI_MOCK=1 POCKETUI_SNAPSHOT_WIFI=1 POCKETUI_ES_SYSTEMS="$FAKE/es_systems.cfg" POCKETUI_DATA="$FAKE/data" \
    "$BIN" --snapshot "$state" "$RAW/$name.png" >/dev/null
  convert "$RAW/$name.png" -strip -resize 960x -define png:compression-level=9 "$OUT/$name.png"
  # Still too big (a picture with many gradients): 256 colours.
  if [ "$(wc -c < "$OUT/$name.png")" -gt 300000 ]; then
    convert "$RAW/$name.png" -strip -resize 960x -dither FloydSteinberg -colors 256 "PNG8:$OUT/$name.png"
  fi
  printf '%-14s %s  %6s bytes\n' "$name" "$state" "$(wc -c < "$OUT/$name.png")"
done
rm -rf "$RAW"
