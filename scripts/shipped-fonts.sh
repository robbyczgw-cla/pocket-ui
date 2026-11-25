#!/bin/sh
# Puts pocket-ui's font files (the Chinese ones, read from the card: src/text_fallback.rs `shipped_dir`) into <dir>/fonts,
# <dir> being the folder of the programs (a release's folder, the Brick's app/bin). Every package that carries the
# programs calls this; `--check <dir>` says whether <dir>/fonts has them, byte for byte.
#   shipped-fonts.sh <dir>          shipped-fonts.sh --check <dir>
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
FONTS="FusionPixel12-zh_hans-CJK.otf NotoSansSC-Regular-CJK.otf"
if [ "${1:-}" = --check ]; then
  for f in $FONTS; do
    cmp -s "$ROOT/assets/fonts/$f" "$2/fonts/$f" || { echo "shipped-fonts: $2/fonts/$f is missing or differs from assets/fonts" >&2; exit 1; }
  done
  exit 0
fi
mkdir -p "$1/fonts"
for f in $FONTS; do install -m 644 "$ROOT/assets/fonts/$f" "$1/fonts/$f"; done
