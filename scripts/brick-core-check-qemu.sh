#!/bin/bash
# Load check for built cores without Docker: dlopen, retro_init, retro_deinit, dlclose under qemu-user, against the Brick's own glibc 2.33 and libraries.
#   scripts/brick-core-check-qemu.sh QEMU_AARCH64_STATIC SYSROOT CORES_DIR [EXTRA_LIB_DIR]
# SYSROOT      a copy of the Brick's /lib and /usr/lib (ld-2.33.so, libc, ...); EXTRA_LIB_DIR  where the GPU driver's libraries (libIMGegl.so, libsrv_um.so,
#              ...) are, when SYSROOT lacks them (libGLESv2.so.2 and libEGL.so.1 need them). Also fails on a GLIBC symbol newer than 2.33 and a missing NODELETE flag.
# Needs zig (builds the probe, brick-core-check.c) and readelf. Nothing is written but a temp folder.
set -u
Q=$(realpath "${1:?qemu-aarch64-static}"); S=$(realpath "${2:?sysroot}"); C=$(realpath "${3:?cores dir}"); X=${4:+$(realpath "$4")}
HERE=$(cd "$(dirname "$0")" && pwd); W=$(mktemp -d); trap 'rm -rf "$W"' EXIT
zig cc -target aarch64-linux-gnu.2.33 -O1 -o "$W/check" "$HERE/brick-core-check.c" -ldl || exit 1
bad=0
for so in "$C"/*_libretro.so; do
  n=$(basename "$so")
  g=$(readelf -V "$so" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1)
  nd=$(readelf -d "$so" | grep -c NODELETE)
  printf '%s: glibc max %s, nodelete %s, needs:%s\n' "$n" "${g:-none}" "$nd" "$(readelf -d "$so" | sed -n 's/.*NEEDED.*\[\(.*\)\]/ \1/p' | tr -d '\n')"
  [ "$(printf '%s\nGLIBC_2.33\n' "${g:-GLIBC_0}" | sort -V | tail -1)" = GLIBC_2.33 ] || { echo "FAIL newer glibc symbol"; bad=1; }
  [ "$nd" -gt 0 ] || { echo "FAIL no NODELETE"; bad=1; }
  timeout 600 "$Q" -L "$S" -E "LD_LIBRARY_PATH=$S/lib:$S/usr/lib${X:+:$X/usr/lib:$X/lib}" "$W/check" "$so" --load-only 120 || bad=1
done
exit $bad
