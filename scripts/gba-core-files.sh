#!/bin/sh
# Stage the pinned ARM64 gpSP build, licence and corresponding source beside a release's programs.
# Input: the core-build output (cores/, licences/gpsp/COPYING, sources/gpsp-5819380.tar.gz).
# Never take ROCKNIX's ELF32 gpSP for the ELF64 pocket-core.
set -eu
[ $# -eq 2 ] || { echo 'usage: gba-core-files.sh BUILD_DIR DEST'; exit 2; }
SRC=$1 DEST=$2
SO=$SRC/cores/gpsp_libretro.so
file -b "$SO" | grep -q 'ELF 64-bit LSB.*ARM aarch64' || { echo 'gba-core-files: requires ARM64 gpSP'; exit 1; }
# This pin includes the ARM64 dynarec and the instruction-cache sync used by the release build.
[ "$(sha256sum "$SO" | cut -d ' ' -f1)" = 7e60aa52c9e81375cd5c342abc00e54717774a1af28691c78480c1d76057d202 ] || { echo 'gba-core-files: unverified core build'; exit 1; }
mkdir -p "$DEST/gpsp"
install -m 755 "$SO" "$DEST/gpsp_libretro.so"
install -m 644 "$SRC/licences/gpsp/COPYING" "$DEST/gpsp/COPYING"
install -m 644 "$SRC/sources/gpsp-5819380.tar.gz" "$DEST/gpsp/source.tar.gz"
printf '%s\n' 'gpSP: https://github.com/libretro/gpsp' 'Commit: 5819380c2ffb0900219d700a382ee68c464ebb99' 'GPL-2.0-or-later; corresponding source in source.tar.gz.' 'Build: aarch64-linux-gnu.2.33, CPU_ARCH=arm64 HAVE_DYNAREC=1.' 'Instruction-cache sync patch and build recipe accompany the source.' >"$DEST/gpsp/SOURCE.txt"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
install -m 644 "$ROOT/deploy/gpsp/clear-cache.h" "$ROOT/scripts/build-gpsp.sh" "$DEST/gpsp/"
(cd "$DEST" && find . -type f ! -name SHA256SUMS | LC_ALL=C sort | xargs sha256sum >SHA256SUMS)
