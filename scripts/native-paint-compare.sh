#!/bin/sh
# Painted natively against the old way (docs/dev/native-paint.md): every state of tests/i18n-states.txt with
# scripts/size-sweep.sh, once with POCKETUI_PAINT=scaled and once with POCKETUI_PAINT=native, and what differs.
#   OUT/scaled/, OUT/native/        size-sweep's output of each (pictures kept)
#   OUT/diff.tsv                    per state and setting: the share of pixels more than 32 steps apart (any channel),
#                                   the largest step, sorted from the largest share down
#   OUT/problems.diff               the O/X/E/... findings of the two (layout and text) and the text each shows
#                                   (POCKETUI_VISIBLE_TEXT, V lines): must be empty, the layout is the same
#   OUT/sheet-1.jpg, sheet-2.jpg    the 40 states that differ most, old | new | difference, 20 to a sheet
#   OUT/big.md5.diff                with BIN_OLD: the checksums at the sizes painted as before (BIG), old binary
#                                   against this one with POCKETUI_PAINT=native: must be empty
# Settings: BIN (default target/release/pocket-ui), BIN_OLD (a build without native painting, optional),
#   SMALL="640x480 720x480", BIG="1240x1080 1920x1080 1280x960", LOOKS="dark light", TEXTS="normal large",
#   FONTS="barlow retro" (POCKETUI_FONT), STATES (";" between), JOBS=4. Needs python3 with PIL (python3-pil).
#   Build BIN and BIN_OLD with the same POCKETUI_COMMIT, or About shows two different ones.
#   scripts/native-paint-compare.sh [OUT]   (default /tmp/pocketui-native-paint; nothing of it belongs in git)
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
OUT=${1:-/tmp/pocketui-native-paint}
BIN=${BIN:-$ROOT/target/release/pocket-ui}
SMALL=${SMALL:-640x480 720x480}
BIG=${BIG:-1240x1080 1920x1080 1280x960}
FONTS=${FONTS:-barlow retro}
export VISIBLE=1 LOOKS="${LOOKS:-dark light}" TEXTS="${TEXTS:-normal large}" LANGS="${LANGS:-en}" JOBS="${JOBS:-4}"
# The same clock and work folder in every run: the status corner's minute and the paths About shows don't differ.
export POCKETUI_MOCK_NOW="${POCKETUI_MOCK_NOW:-1794139200}" SWEEP_WORK="${TMPDIR:-/tmp}/pocketui-native-paint-work"
mkdir -p "$OUT"
for font in $FONTS; do
  for mode in scaled native; do
    echo "== $font $mode: $SMALL"
    POCKETUI_FONT=$font POCKETUI_PAINT=$mode SIZES=$SMALL KEEP=1 BIN=$BIN sh "$ROOT/scripts/size-sweep.sh" "$OUT/$mode/$font"
  done
done
python3 -I "$ROOT/scripts/native-paint-diff.py" "$OUT"
if [ -n "${BIN_OLD:-}" ] && [ -n "$BIG" ]; then
  echo "== unchanged at $BIG"
  POCKETUI_FONT=barlow POCKETUI_PAINT=native SIZES=$BIG TEXTS=normal BIN=$BIN sh "$ROOT/scripts/size-sweep.sh" "$OUT/big-new"
  POCKETUI_FONT=barlow SIZES=$BIG TEXTS=normal BIN=$BIN_OLD sh "$ROOT/scripts/size-sweep.sh" "$OUT/big-old"
  : >"$OUT/big.md5.diff"
  for f in "$OUT"/big-old/*.md5; do
    diff "$f" "$OUT/big-new/$(basename "$f")" >>"$OUT/big.md5.diff" || true
  done
  echo "unchanged at $BIG: $(cat "$OUT"/big-old/*.md5 | wc -l | tr -d ' ') pictures, $(grep -c '^>' "$OUT/big.md5.diff" || true) differ"
fi
