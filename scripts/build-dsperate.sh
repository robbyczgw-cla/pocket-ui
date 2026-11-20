#!/bin/sh
# Normal core builder, restricted to the pinned portable ARM64 DS core.
# scripts/build-dsperate.sh OUT WORK [GLIBC]; default 2.17 works on every Linux target.
set -eu
OUT=${1:?output directory}; WORK=${2:?work directory}; GLIBC=${3:-2.17}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$WORK/sysroot"
exec env BRICK_CORES=dsperate BRICK_GLIBC="$GLIBC" BRICK_SYSROOT="${BRICK_SYSROOT:-$WORK/sysroot}" JOBS="${JOBS:-2}" \
    bash "$ROOT/scripts/brick-build-cores.sh" "$OUT" "$WORK"
