#!/bin/sh
# The browser demo (docs/WEB-DEMO.md): pocket-ui for wasm32-unknown-emscripten, the demo library packed beside it,
# and the page, all in web/demo/dist/.
#
#   web/build.sh [release|fast|dev|data]     (data: only the library and the page again, no compile)
#   WEB_OPT=z|s|3 (release's opt-level, 3 by default: 2.3x faster painting than z for 1.5 MB more), WEB_SIMD=0 (no wasm
#   SIMD: dist/base instead of dist/simd, for browsers without it). Build both for a site.
#
# Needs: emsdk's emcc on PATH (source emsdk_env.sh), the toolchain's rust-src (std is built again with atomics, so the
# App's threads become Web Workers: -Zbuild-std, RUSTC_BOOTSTRAP on the stable compiler), and web/demo/storage
# (web/make-demo-library.py). Builds nothing for the devices; the root Cargo.toml and Cargo.lock are not read.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
profile=${1:-release}
[ -d "$here/demo/storage/roms" ] || { echo "no demo library: python3 -I web/make-demo-library.py <thumbs> web/demo/storage" >&2; exit 1; }
command -v emcc >/dev/null || { echo "emcc not on PATH (source emsdk_env.sh)" >&2; exit 1; }

exports=_main,_pocket_web_button,_pocket_web_touch,_pocket_web_resize,_pocket_web_redraw,_pocket_web_frame,_pocket_web_card,_pocket_web_paint_stats,_pocket_web_audio,_pocket_web_audio_running,_malloc,_free
features=+atomics,+bulk-memory,+mutable-globals
cflags=-pthread
[ "${WEB_SIMD:-1}" = 1 ] && features=$features,+simd128 && cflags="$cflags -msimd128"
flags="-C target-feature=$features"
# Panic messages name source files: as the crates' own paths, never the build machine's folders (the last matching
# prefix wins, so the home folder comes first).
sysroot=$(rustc --print sysroot)
for map in "$HOME=/home" "$(cd "$here/.." && pwd)=/pocket-ui" "${CARGO_HOME:-$HOME/.cargo}=/cargo" "$sysroot=/rust"; do
  flags="$flags --remap-path-prefix=$map"
done
variant=simd
[ "${WEB_SIMD:-1}" = 1 ] || variant=base
for a in -pthread -sPROXY_TO_PTHREAD=1 -sPTHREAD_POOL_SIZE=16 -sALLOW_MEMORY_GROWTH=1 \
    -sINITIAL_MEMORY=134217728 -sMAXIMUM_MEMORY=1073741824 -sSTACK_SIZE=8388608 -sDEFAULT_PTHREAD_STACK_SIZE=2097152 \
    -sENVIRONMENT=web,worker -sEXIT_RUNTIME=0 -sFORCE_FILESYSTEM=1 -sEXPORTED_FUNCTIONS=$exports \
    -sEXPORTED_RUNTIME_METHODS=HEAPU8,HEAPU32,ENV,FS -sMALLOC=mimalloc; do
  flags="$flags -C link-arg=$a"
done
# release: what goes on the site; fast: the same code without LTO, for trying things.
case $profile in
  release) opt=${WEB_OPT:-3}; flags="$flags -C link-arg=-O$opt"; export CARGO_PROFILE_RELEASE_OPT_LEVEL=$opt
    cargo_profile=--release; dir=release ;;
  fast) flags="$flags -C link-arg=-O2"; cargo_profile="--profile fast"; dir=fast ;;
  data) dir=release ;;
  *) cargo_profile=; dir=debug ;;
esac

cd "$here"
[ "$profile" = data ] || CFLAGS_wasm32_unknown_emscripten="$cflags" RUSTFLAGS="$flags" RUSTC_BOOTSTRAP=1 \
  cargo build $cargo_profile --target wasm32-unknown-emscripten -Zbuild-std=std,panic_abort --bin pocket-ui

out=${CARGO_TARGET_DIR:-$here/target}/wasm32-unknown-emscripten/$dir
dist=$here/demo/dist
mkdir -p "$dist/$variant"
# emcc names the program after the crate (pocket_ui.wasm) and its loader after the binary. One folder per variant: simd
# (wasm SIMD, Safari 16.4 and later) and base (without); the page picks one (try.js on the site, index.html here).
[ "$profile" = data ] || cp "$out/pocket-ui.js" "$out/pocket_ui.wasm" "$dist/$variant/"
# The library on its own: a new cover or setting needs no new link. The page loads demo-data.js before pocket-ui.js.
# From inside dist and the library's own folder, so no path of this machine ends up in demo-data.js.
packager="$(dirname "$(command -v emcc)")/tools/file_packager.py"
(cd "$here/demo" && python3 "$packager" dist/demo.data --preload storage@/storage --js-output=dist/demo-data.js --no-node)
cp "$here"/demo/page/* "$dist/"
ls -l "$dist"
