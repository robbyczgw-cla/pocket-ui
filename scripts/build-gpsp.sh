#!/bin/sh
# Reproduce the pinned ARM64 gpSP for H700. Requires git, make and zig 0.17.0.
#   scripts/build-gpsp.sh OUT WORK
set -eu
OUT=$1 WORK=$2
ROOT=$(cd "$(dirname "$0")/.." && pwd)
HEADER=$ROOT/deploy/gpsp/clear-cache.h
# The packaged recipe carries the same header beside itself, outside a source checkout.
[ -f "$HEADER" ] || HEADER=$(cd "$(dirname "$0")" && pwd)/clear-cache.h
mkdir -p "$OUT/cores" "$OUT/licences/gpsp" "$OUT/sources" "$WORK/bin"
OUT=$(cd "$OUT" && pwd); WORK=$(cd "$WORK" && pwd)
git init -q "$WORK/gpsp"
git -C "$WORK/gpsp" fetch -q --depth 1 https://github.com/libretro/gpsp 5819380c2ffb0900219d700a382ee68c464ebb99
git -C "$WORK/gpsp" checkout -q --detach FETCH_HEAD
cat >"$WORK/bin/cc" <<WRAP
#!/bin/sh
exec zig cc -target aarch64-linux-gnu.2.33 -Wno-builtin-macro-redefined '-D__DATE__="redacted"' '-D__TIME__="redacted"' -ffile-prefix-map=$WORK=/work -D__clear_cache=brick_clear_cache -include $HEADER "\$@"
WRAP
chmod +x "$WORK/bin/cc"
(cd "$WORK/gpsp" && make -j4 platform=unix ARCH=arm64 CPU_ARCH=arm64 HAVE_DYNAREC=1 HAVE_NEON=1 STRIP=true CC="$WORK/bin/cc" 'LDFLAGS=-Wl,-z,noexecstack -Wl,-z,nodelete -s')
cp "$WORK/gpsp/gpsp_libretro.so" "$OUT/cores/"
cp "$WORK/gpsp/COPYING" "$OUT/licences/gpsp/"
git -C "$WORK/gpsp" archive --format=tar.gz HEAD >"$OUT/sources/gpsp-5819380.tar.gz"
