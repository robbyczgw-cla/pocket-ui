#!/bin/sh
# Build helper (runs in the Debian stage of docker/Dockerfile): copies Debian's Mesa EGL + GLES + llvmpipe,
# with every library they need except glibc, into $1 (default /out). The Mesa in ROCKNIX has no llvmpipe
# (the Mini V2's GPU is a Freedreno; its software fallback is softpipe, ~2 fps at 1240x1080).
set -eu
OUT=${1:-/out}
L=/usr/lib/aarch64-linux-gnu
mkdir -p "$OUT/lib/dri" "$OUT/share/glvnd/egl_vendor.d"

roots="$L/libEGL.so.1 $L/libGLESv2.so.2 $L/libEGL_mesa.so.0 $L/libgbm.so.1 $L/dri/swrast_dri.so $L/dri/kms_swrast_dri.so"
for g in $L/libgallium-*.so; do roots="$roots $g"; done

for r in $roots; do
  ldd "$r" | awk '/=> \// {print $1, $3}'
done | sort -u | while read -r soname path; do
  case "$soname" in
    libc.so.* | libm.so.* | libdl.so.* | libpthread.so.* | librt.so.* | libutil.so.* | libresolv.so.* | ld-linux*) continue ;;
  esac
  cp -L "$path" "$OUT/lib/$soname"
done
for r in $roots; do
  cp -L "$r" "$OUT/lib/$(basename "$r")" 2>/dev/null || true
done
cp -L $L/dri/swrast_dri.so $L/dri/kms_swrast_dri.so "$OUT/lib/dri/"
cp /usr/share/glvnd/egl_vendor.d/50_mesa.json "$OUT/share/glvnd/egl_vendor.d/"
du -sh "$OUT"
