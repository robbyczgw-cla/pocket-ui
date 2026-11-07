#!/bin/sh
# Optional bundled gpSP: fixed image inventory and validation, separate from ROCKNIX's system cores.
set -eu
case $1 in
  --files)
    REL=./pocket-ui/releases/$2/cores
    printf '755 0:0 d %s\n' "$REL" "$REL/gpsp"
    printf '755 0:0 f %s/gpsp_libretro.so\n' "$REL"
    for f in SHA256SUMS gpsp/COPYING gpsp/SOURCE.txt gpsp/source.tar.gz gpsp/build-gpsp.sh gpsp/clear-cache.h; do
      printf '644 0:0 f %s/%s\n' "$REL" "$f"
    done
    ;;
  --validate)
    D=$2
    for f in gpsp_libretro.so SHA256SUMS gpsp/COPYING gpsp/SOURCE.txt gpsp/source.tar.gz gpsp/build-gpsp.sh gpsp/clear-cache.h; do
      [ -s "$D/$f" ] || exit 1
    done
    [ "$(sha256sum "$D/gpsp_libretro.so" | cut -d ' ' -f1)" = 7e60aa52c9e81375cd5c342abc00e54717774a1af28691c78480c1d76057d202 ]
    file -b "$D/gpsp_libretro.so" | grep -q 'ELF 64-bit LSB.*ARM aarch64'
    (cd "$D" && sha256sum -c SHA256SUMS >/dev/null)
    grep -q '5819380c2ffb0900219d700a382ee68c464ebb99' "$D/gpsp/SOURCE.txt"
    grep -q 'Version 2' "$D/gpsp/COPYING"
    ;;
  *) echo 'usage: gba-checks.sh --files VERSION | --validate CORES_DIR' >&2; exit 2 ;;
esac
