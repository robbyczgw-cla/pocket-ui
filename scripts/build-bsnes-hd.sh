#!/bin/sh
# The pinned ARM64 bsnes-hd for ROCKNIX's Mode 7 HD (src/mode7_hd.rs), through the normal core builder:
#   scripts/build-bsnes-hd.sh OUT WORK
# Needs zig 0.17.0, make, git, network. glibc 2.33 symbols (nall needs getrandom, 2.25), as gpSP: every ROCKNIX image has newer. OUT gets
# cores/bsnes_hd_beta_libretro.so, licences/bsnes_hd_beta/, sources/bsnes_hd_beta-fc26b25.tar.gz and CORES.txt, the input of
# scripts/bsnes-hd-files.sh (make-install-zip.sh, make-fat-install.sh and release-channel.sh --bsnes-hd).
set -eu
OUT=${1:?output directory}; WORK=${2:?work directory}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$WORK/sysroot"
exec env BRICK_CORES=bsnes_hd_beta BRICK_GLIBC=2.33 BRICK_SYSROOT="${BRICK_SYSROOT:-$WORK/sysroot}" JOBS="${JOBS:-4}" \
    bash "$ROOT/scripts/brick-build-cores.sh" "$OUT" "$WORK"
