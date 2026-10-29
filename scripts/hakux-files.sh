#!/bin/sh
# Copies the hakuX Xbox core (the output of scripts/build-hakux-core.sh) with everything that must travel with it (GPL-3.0,
# docs/XBOX.md "Shipping the core") into DEST, as the packages carry it (hakux/ in the install zip, pocket-ui/hakux/ in the
# card seed): hakux_libretro.so, the .info of deploy/hakux/ (the one a test pins to "no save states"), SOURCE.txt, BUILD-INFO,
# our patches to the GPL source if any (patches/), the core's own licence texts (licenses/) and SHA256SUMS over all of them.
# Both scripts use this, so the SSH package and the two files for the card cannot drift.
#
#   scripts/hakux-files.sh DIR DEST
#     DIR   a hakux dir from scripts/build-hakux-core.sh (lib/libretro/hakux_libretro.so, share/licenses/hakux, BUILD-INFO;
#           a plain hakux_libretro.so and licenses/ directly in DIR work too)
#     DEST  the folder to fill (created); on the device it becomes /storage/pocket-ui/hakux
set -eu
bad() { echo "hakux-files: $*" >&2; exit 1; }
[ $# -eq 2 ] || { echo "usage: scripts/hakux-files.sh DIR DEST" >&2; exit 2; }
HAKUX_DIR=$1
DIR=$2
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$DIR/licenses"
SO=$HAKUX_DIR/lib/libretro/hakux_libretro.so
[ -s "$SO" ] || SO=$HAKUX_DIR/hakux_libretro.so
[ -s "$SO" ] || bad "no hakux_libretro.so in $HAKUX_DIR"
LICS=$HAKUX_DIR/share/licenses/hakux
[ -d "$LICS" ] || LICS=$HAKUX_DIR/licenses
[ -s "$LICS/COPYING" ] || bad "the core's licence texts are missing ($LICS/COPYING)"
install -m 755 "$SO" "$DIR/hakux_libretro.so"
install -m 644 "$ROOT/deploy/hakux/hakux_libretro.info" "$ROOT/deploy/hakux/SOURCE.txt" "$DIR/"
for P in "$ROOT"/deploy/hakux/patches/*.patch; do   # none while the core is a PR commit as it is
  [ -e "$P" ] || break
  mkdir -p "$DIR/patches" && install -m 644 "$P" "$DIR/patches/"
done
[ ! -s "$HAKUX_DIR/BUILD-INFO" ] || install -m 644 "$HAKUX_DIR/BUILD-INFO" "$DIR/"
cp "$LICS/"* "$DIR/licenses/" && chmod 644 "$DIR/licenses/"*
(cd "$DIR" && find . -type f ! -name SHA256SUMS | sort | xargs sha256sum >SHA256SUMS)
