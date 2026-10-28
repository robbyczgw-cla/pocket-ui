#!/bin/sh
# Screen frames in the virtual Mini V2: runs a game with a Picture setting, takes screenshots and
# reports the draw time per frame (pocket-core --bench). See docs/FRAMES.md.
#   VDEV_NAME=frames scripts/frame-shots.sh SYSTEM '<settings json>' NAME [SECONDS] [SHOT_AT...]
# SYSTEM is gb, gbc, gba, nes or snes (its first ROM in /storage/roms/SYSTEM is used). Screenshots
# land in screenshots/NAME-<second>.png; pocket-ui is stopped first (vdev.sh deploy brings it back).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
V="$ROOT/scripts/vdev.sh"
sys=$1 json=$2 name=$3 secs=${4:-12}
shift 3
[ $# -gt 0 ] && shift
shots=${*:-6}
case "$sys" in gb | gbc) core=gambatte ;; gba) core=mgba ;; nes) core=nestopia ;; snes) core=snes9x ;; *) echo "unknown system $sys" >&2; exit 1 ;; esac
"$V" stop >/dev/null 2>&1 || true
"$V" sh "mkdir -p /storage/.config/pocket-ui/game-settings && rm -f /storage/.config/pocket-ui/game-settings/$sys/*.json && printf '%s' '$json' > /storage/.config/pocket-ui/game-settings/$sys.json"
rom=$("$V" sh "ls /storage/roms/$sys/* | head -1")
log=$(mktemp)
"$V" run-core "$core" "$rom" --system "$sys" --bench "$secs" >"$log" 2>&1 </dev/null &
pid=$!
# KEYS="3:START 5:A": buttons pressed on the virtual pad at those seconds (past title screens).
if [ -n "${KEYS:-}" ]; then
  (
    k=0
    for key in $KEYS; do
      sleep $((${key%%:*} - k))
      k=${key%%:*}
      "$V" pad press "${key#*:}" 120 >/dev/null 2>&1 || true
    done
  ) &
fi
t=0
for at in $shots; do
  sleep $((at - t))
  t=$at
  "$V" shot "$name-$at" >/dev/null
done
wait $pid || true
grep -E "^(draw|frames|bezel|ambient)" "$log" | sed "s/^/$name: /"
rm -f "$log"
