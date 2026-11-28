#!/bin/sh
# Stage the pinned ARM64 bsnes-hd (Options > Picture > Mode 7 HD on ROCKNIX, src/mode7_hd.rs) beside a release's programs,
# as gpSP and DSperate are: every ROCKNIX package and the app update carry it, so a card whose image has no bsnes-hd still
# plays Mode 7 HD (docs/user/options.md says "bundled").
#   scripts/bsnes-hd-files.sh BUILD_DIR DEST     BUILD_DIR: scripts/build-bsnes-hd.sh's OUT; DEST: the release's cores/
#   scripts/bsnes-hd-files.sh --check CORES_DIR  exit 1 unless the verified core, its licences and its source are there
# DEST gets bsnes_hd_beta_libretro.so and bsnes-hd/ (LICENSE: GPL-3.0; LICENSE-SameBoy: MIT, its Game Boy part; source.tar.gz:
# the corresponding source; SOURCE.txt; the build recipe; SHA256SUMS over all of it). Never touches DEST/SHA256SUMS (gpSP's).
set -eu
SHA=9fe9806e71dcd3c1d25f1e0d1d14095b487d2e9b3bce2b532f4fd1e21c631ec9
COMMIT=fc26b25ea236f0f877f0265d2a2c37dfd93dfde9
die() { echo "bsnes-hd-files: $*" >&2; exit 1; }
check() {
  [ "$(sha256sum "$1/bsnes_hd_beta_libretro.so" 2>/dev/null | cut -d ' ' -f1)" = "$SHA" ] \
    && [ -s "$1/bsnes-hd/LICENSE" ] && [ -s "$1/bsnes-hd/source.tar.gz" ] \
    && (cd "$1/bsnes-hd" && sha256sum -c SHA256SUMS >/dev/null 2>&1)
}
if [ "${1:-}" = --check ]; then
  [ $# -eq 2 ] || { echo 'usage: bsnes-hd-files.sh --check CORES_DIR' >&2; exit 2; }
  check "$2" || die "$2 lacks the verified bsnes-hd, its licences or its source (scripts/build-bsnes-hd.sh, then --bsnes-hd BUILD_DIR)"
  exit 0
fi
[ $# -eq 2 ] || { echo 'usage: bsnes-hd-files.sh BUILD_DIR DEST | --check CORES_DIR' >&2; exit 2; }
SRC=$1 DEST=$2
SO=$SRC/cores/bsnes_hd_beta_libretro.so
file -b "$SO" | grep -q 'ELF 64-bit LSB.*ARM aarch64' || die 'requires the ARM64 build of scripts/build-bsnes-hd.sh'
# The pin: zig 0.17.0, glibc 2.33, the commit ROCKNIX 20261001 builds; the same bytes from a fresh work folder (25.11.2026).
[ "$(sha256sum "$SO" | cut -d ' ' -f1)" = "$SHA" ] || die 'unverified core build'
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$DEST/bsnes-hd"
install -m 755 "$SO" "$DEST/bsnes_hd_beta_libretro.so"
install -m 644 "$SRC/licences/bsnes_hd_beta/LICENSE" "$DEST/bsnes-hd/LICENSE"
install -m 644 "$SRC/licences/bsnes_hd_beta/bsnes_gb/LICENSE" "$DEST/bsnes-hd/LICENSE-SameBoy"
install -m 644 "$SRC/sources/bsnes_hd_beta-${COMMIT%${COMMIT#???????}}.tar.gz" "$DEST/bsnes-hd/source.tar.gz"
printf '%s\n' 'bsnes-hd: https://github.com/DerKoun/bsnes-hd' "Commit: $COMMIT" \
  'GPL-3.0 (LICENSE); its Game Boy part from SameBoy, MIT (LICENSE-SameBoy). Corresponding source in source.tar.gz.' \
  'Build: zig 0.17.0, aarch64-linux-gnu.2.33, target=libretro build=performance openmp=false (build-bsnes-hd.sh, brick-build-cores.sh).' >"$DEST/bsnes-hd/SOURCE.txt"
install -m 644 "$ROOT/scripts/build-bsnes-hd.sh" "$ROOT/scripts/brick-build-cores.sh" "$DEST/bsnes-hd/"
(cd "$DEST/bsnes-hd" && { find . -type f ! -name SHA256SUMS | LC_ALL=C sort | xargs sha256sum; sha256sum ../bsnes_hd_beta_libretro.so; } >SHA256SUMS)
check "$DEST" || die "staged files don't check out in $DEST"
