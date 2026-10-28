#!/bin/sh
# Snapshots of Credits & links (X on a console page), dark and light, with Curator's made-up library
# (scripts/curator-fake-library.py) plus a few Sega, NEC, Bandai and Atari games, so the links have somewhere to go.
#   scripts/console-credits-shots.sh [OUT]        (default: screenshots/console-credits)
# Each console twice: the top of the page and scrolled down (<id>-<look>.png, <id>-down-<look>.png).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-$ROOT/screenshots/console-credits}
BIN=$ROOT/target/release/pocket-ui
FAKE=${TMPDIR:-/tmp}/pocketui-credits-fake
mkdir -p "$OUT"
(cd "$ROOT" && cargo build --release -q --bin pocket-ui)
for look in dark light; do
  python3 -I "$ROOT/scripts/curator-fake-library.py" "$FAKE" "$look" >/dev/null
  # More systems: (folder, name, extensions, games).
  python3 -I - "$FAKE" <<'EOF'
import os, sys
out = sys.argv[1]
extra = [
    ("mastersystem", "Master System", ".sms", ["Alex Kidd in Miracle World (USA, Europe).sms", "Phantasy Star (USA, Europe).sms"]),
    ("gamegear", "Game Gear", ".gg", ["Sonic The Hedgehog 2 (World).gg", "Shinobi (USA, Europe).gg"]),
    ("sg-1000", "SG-1000", ".sg", ["Flicky (Japan).sg"]),
    ("dreamcast", "Dreamcast", ".chd", ["Sonic Adventure (USA).chd", "Jet Grind Radio (USA).chd"]),
    ("saturn", "Sega Saturn", ".chd", ["Panzer Dragoon Saga (USA).chd"]),
    ("segacd", "Sega CD", ".chd", ["Sonic CD (USA).chd"]),
    ("pcengine", "PC Engine", ".pce", ["Bonk's Adventure (USA).pce"]),
    ("wonderswan", "WonderSwan", ".ws", ["Gunpey (Japan).ws"]),
    ("atari2600", "Atari 2600", ".a26", ["Adventure (USA).a26", "Pitfall! (USA).a26"]),
]
cfg = os.path.join(out, "es_systems.cfg")
lines = open(cfg).read().replace("</systemList>\n", "")
for sid, name, ext, games in extra:
    folder = os.path.join(out, "roms", sid)
    os.makedirs(folder, exist_ok=True)
    for f in os.listdir(folder):
        os.remove(os.path.join(folder, f))
    for g in games:
        open(os.path.join(folder, g), "w").write("x")
    lines += f"  <system><name>{sid}</name><fullname>{name}</fullname><path>{folder}</path><extension>{ext}</extension></system>\n"
open(cfg, "w").write(lines + "</systemList>\n")
EOF
  for id in gb gbc gba nds 3ds nes snes n64 gamecube psx ps2 psp megadrive mastersystem gamegear sg-1000 saturn dreamcast pcengine wonderswan atari2600; do
    for state in "credits" "credits:8"; do
      name=$id-$(echo "$state" | sed 's/credits:8/down/;s/credits//')
      name=${name%-}
      POCKETUI_MOCK=1 POCKETUI_ES_SYSTEMS="$FAKE/es_systems.cfg" POCKETUI_DATA="$FAKE/data" \
        "$BIN" --snapshot "console:$id:$state" "$OUT/$name-$look.png" >/dev/null
    done
  done
done
echo "$OUT"
