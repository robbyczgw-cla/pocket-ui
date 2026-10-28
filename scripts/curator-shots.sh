#!/bin/sh
# Snapshots of every Curator screen, dark and light, with the made-up library of scripts/curator-fake-library.py and
# the shipped data (gamedb, curatordb).
#   scripts/curator-shots.sh [OUT]        (default: screenshots/curator)
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/screenshots/curator}
BIN=$ROOT/target/release/pocket-ui
FAKE=${TMPDIR:-/tmp}/pocketui-curator-fake
mkdir -p "$OUT"
(cd "$ROOT" && cargo build --release -q --bin pocket-ui)
for look in dark light; do
  python3 -I "$ROOT/scripts/curator-fake-library.py" "$FAKE" "$look" >/dev/null
  for state in \
    home home:split hub options shelf:0 \
    lens lens:dddddra lens:rddda lens:ddrada save:dralddddra ready ready:lens \
    card:ps2:revenge card:ps2:revenge:1 card:snes:chrono:1 \
    echoes:ps2:revenge echoes:snes:chrono echoes:gba:maximum echoes:gamecube:double versions:snes:chrono versions:ps2:dominator versions:n64:ocarina card:n64:ocarina:2 stickers:snes:mario \
    settings settings:2 settings:5 \
    lens:dddddrada lens:dddddddddr lens:ddddddddddrda lens:dddddddddddddr echoes:psp:ridge \
    upnext:ps2:revenge upnext:ps2:revenge:cabinet upnext:ps2:revenge:split \
    cabinet:dd cabinet:ddd cabinet:ddrrra cabinet:ddrrrrra cabinetlens:rda \
    card:ps2:takedown card:ps2:revenge:yddddrr surprise surprise:1:1 surprise:empty lens:S lens:Sdda lens:Sddas options:da \
    home:shelf:favorites home:shelf:shelf home:split:favorites cabinet:ddr; do
    name=$(echo "$state" | tr ':' '-')
    POCKETUI_MOCK=1 POCKETUI_ES_SYSTEMS="$FAKE/es_systems.cfg" POCKETUI_DATA="$FAKE/data" \
      "$BIN" --snapshot "curator:$state" "$OUT/$name-$look.png" >/dev/null
  done
done
echo "$OUT"
