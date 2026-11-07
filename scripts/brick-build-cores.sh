#!/bin/bash
# Builds the pinned libretro cores for the TrimUI Brick standalone card (docs/TRIMUI-BRICK-STANDALONE.md):
#   scripts/brick-build-cores.sh OUT_DIR [WORK_DIR]
# Verified with zig 0.17.0 (cross C/C++ compiler), make, git, network. Runs on any host (an Apple-silicon Mac, a Linux box).
# Target aarch64, glibc 2.33 (the Brick's TinaLinux), so the .so files carry no symbol newer than GLIBC_2.33.
# The commits are the ones ROCKNIX 20261001 builds (the same as NOTICE.md lists); each is fetched by its full id. gpsp and fceumm are
# not on ROCKNIX's list: the upstream master of 2026-11-03 is pinned. picodrive is the commit ROCKNIX 20261001 builds (non-commercial licence, NOTICE.md).
# -z nodelete: pocket-core dlclose()s a core when a game ends, and the C++ cores (zig's libc++ is linked into each) crash in
# dlclose; with the flag the library just stays mapped until the process ends (checked by brick-core-check, which does the dlclose).
# Additional software cores use the ROCKNIX 20261001 commits recorded in NOTICE.md.
# The three 3D cores (mupen64plus_next, ppsspp, flycast; GLES 3, no Vulkan) link against the Brick's own libGLESv2/libEGL, read from the device
# into a sysroot (BRICK_SYSROOT: libc, libm, libpthread, libdl, libEGL, libGLESv2 copied off a Brick); only link-time stubs of them are made, they are never shipped.
# Additional software cores use the ROCKNIX 20261001 commits recorded in NOTICE.md, except Yabause's software port
# (8926b0c, also in NOTICE.md); YabaSanshiro's GL-only libretro port is unsuitable for this software card.
# OUT_DIR gets cores/<id>_libretro.so, licences/<id>/..., sources/<id>-<short>.tar.gz (the corresponding source), CORES.txt.
set -eu
OUT=${1:?out dir}; WORK=${2:-${TMPDIR:-/tmp}/brick-cores-work}
GLIBC=${BRICK_GLIBC:-2.33}
JOBS=${JOBS:-4}
[[ "$JOBS" =~ ^[1-4]$ ]] || { echo "brick-build-cores: JOBS must be 1 to 4" >&2; exit 1; }
TARGET=aarch64-linux-gnu.$GLIBC
SYSROOT=${BRICK_SYSROOT:?BRICK_SYSROOT: a folder with the libraries of the Brick (lib/, usr/lib/), copied off the device}
# id|repo|commit|makefile dir|makefile|output name
CORES='
gambatte|https://github.com/libretro/gambatte-libretro|d9d6cd06382d1ced30de34d56d3609452323dab1|.|Makefile.libretro|gambatte_libretro.so
mgba|https://github.com/libretro/mgba|e31759b24e7a4e3899285ff720d7b573ac328ae7|.|cmake|mgba_libretro.so
nestopia|https://github.com/libretro/nestopia|b08b473f961e057216bc9e8c9d61054b1ab116cf|libretro|Makefile|nestopia_libretro.so
snes9x|https://github.com/libretro/snes9x|890b5d445538fe790aa3add3d5702c80f551e0ae|libretro|Makefile|snes9x_libretro.so
pcsx_rearmed|https://github.com/libretro/pcsx_rearmed|da2cb8ecd17fd0932ab6d94774c0522beebce6e3|.|Makefile.libretro|pcsx_rearmed_libretro.so
gpsp|https://github.com/libretro/gpsp|5819380c2ffb0900219d700a382ee68c464ebb99|.|Makefile|gpsp_libretro.so
fceumm|https://github.com/libretro/libretro-fceumm|e0f3086a8104f8fbbe49a2615b10401504a9533f|.|Makefile.libretro|fceumm_libretro.so
picodrive|https://github.com/libretro/picodrive|733c711a477a642fd2006d5a7a581b2790ec36b4|.|Makefile.libretro|picodrive_libretro.so
beetle_ngp|https://github.com/libretro/beetle-ngp-libretro|a50d5ac288a81f2104ddf43195a4efdd15c72227|.|Makefile|mednafen_ngp_libretro.so
beetle_wswan|https://github.com/libretro/beetle-wswan-libretro|4b01295838ea89e3f1355bbe4cb5cf98aa6108cd|.|Makefile|mednafen_wswan_libretro.so
beetle_pce_fast|https://github.com/libretro/beetle-pce-fast-libretro|bebe2b13a840fbfd45cb501f5cd0efe01bdb1735|.|Makefile|mednafen_pce_fast_libretro.so
handy|https://github.com/libretro/libretro-handy|bc55d462f0b2d6b073ea93dc552ebd73cec60fd1|.|Makefile|handy_libretro.so
stella|https://github.com/stella-emu/stella|c1ffb833c8b180433b0cad76bb6b55f8dfbc46ee|src/os/libretro|Makefile|stella_libretro.so
prosystem|https://github.com/libretro/prosystem-libretro|8a88014287c7a01cd568067e5a557d0a2b2a051f|.|Makefile|prosystem_libretro.so
fbneo|https://github.com/libretro/FBNeo|f3b774987e009d07f1322ebc4910532ed5b8c808|src/burner/libretro|Makefile|fbneo_libretro.so
mupen64plus_next|https://github.com/libretro/mupen64plus-libretro-nx|f275caf4b2bfa1e6d1c51636746ea793f3d80320|.|Makefile|mupen64plus_next_libretro.so
ppsspp|https://github.com/hrydgard/ppsspp|fa50bb1976065c4f8b1b47af227d367fe9771555|.|cmake3d|ppsspp_libretro.so
flycast|https://github.com/flyinghead/flycast|c3763d8fc4208dd6f8f0bc456383543b8406a8a0|.|cmake3d|flycast_libretro.so
beetle_supergrafx|https://github.com/libretro/beetle-supergrafx-libretro|3c6fcd3deded54ebecd69408f108407ac03d11b5|.|Makefile|mednafen_supergrafx_libretro.so
beetle_vb|https://github.com/libretro/beetle-vb-libretro|83ed42608601fb7b01d41e4f8fb2007a37b8c84e|.|Makefile|mednafen_vb_libretro.so
pokemini|https://github.com/libretro/pokemini|132111b76343559860532a1ccc094f93f1ed5650|.|Makefile.libretro|pokemini_libretro.so
dosbox_pure|https://github.com/schellingb/dosbox-pure|7f6e8fb7385fa446d1444d671063268520bf9b54|.|Makefile|dosbox_pure_libretro.so
fake08|https://github.com/jtothebell/fake-08|814991a2571ad3970e386cef48f3b148aa1c27b9|platform/libretro|Makefile|fake08_libretro.so
gearsystem|https://github.com/drhelius/Gearsystem|2c725424715dd41a5d6f8e97a4d318316cf9dbad|platforms/libretro|Makefile|gearsystem_libretro.so
yabause|https://github.com/libretro/yabause|8926b0c6c347f8c5c755911ddb0ac695420ffbf8|yabause/src/libretro|Makefile|yabause_libretro.so
tic80|https://github.com/nesbox/TIC-80|7020500a6e88f6ee91301933bb77f082a10e10f5|.|cmake-tic|tic80_libretro.so
melondsds|https://github.com/JesseTG/melonds-ds|bc4e4b67d2d470d7c682810a1e892cafd6f9082b|.|cmake-melon|melondsds_libretro.so
scummvm|https://github.com/libretro/scummvm|292e409938384ac0b3819a336c61fbb71dcbb9c3|backends/platform/libretro|Makefile|scummvm_libretro.so
'
ONLY=${BRICK_CORES:-}
mkdir -p "$OUT/cores" "$OUT/licences" "$OUT/sources" "$WORK"
OUT=$(cd "$OUT" && pwd); WORK=$(cd "$WORK" && pwd)
MAKE=$(command -v gmake || command -v make)
# Link-time names for the Brick's GLES/EGL libraries (3D cores only).
GLSTUB=$WORK/gles-link
# zig as CC/CXX: wrappers, because make passes CC as one word in some Makefiles.
BIN=$WORK/bin; mkdir -p "$BIN"
# zig's linker does not know -Wl,-no-undefined (a libtool-ism some Makefiles pass); it is the default for shared objects there anyway.
# libmad/version must not shadow libc++'s <version>; its include directory goes after system headers.
# The build date and time are replaced by a fixed word (pcsx_rearmed prints them, and clang refuses __DATE__ under -Werror): same bytes every time.
# -ffile-prefix-map for every core: no build folder (and so no account name) in assert and log strings.
for t in cc c++; do
  cat >"$BIN/$t" <<WRAP
#!/bin/sh
for a; do shift; case "\$a" in -Wl,-no-undefined) ;; -I*/libmad) set -- "\$@" -idirafter "\${a#-I}";; *) set -- "\$@" "\$a";; esac; done
if [ "$t" = c++ ] && [ "\$BRICK_CXX_C" = 1 ]; then
  for a; do case "\$a" in *.c) set -- -x c++ "\$@"; break;; esac; done
fi
exec zig $t -target $TARGET -Wno-builtin-macro-redefined '-D__DATE__="redacted"' '-D__TIME__="redacted"' -ffile-prefix-map=$WORK=/work \$BRICK_CC_EXTRA "\$@"
WRAP
done
printf '#!/bin/sh\nexec zig ar "$@"\n' >"$BIN/ar"
printf '#!/bin/sh\nexec zig ranlib "$@"\n' >"$BIN/ranlib"
chmod +x "$BIN"/*
touch "$OUT/CORES.txt"
# The Khronos API headers (EGL, KHR, GLES2/3: MIT/Apache-2.0 licensed) the 3D cores include; the Brick has no headers, only the libraries. Pinned, compile-time only.
khronos_headers() {
  local inc=$WORK/gles-include
  if [ ! -f "$inc/.done" ]; then
    rm -rf "$inc" "$WORK/khronos"; mkdir -p "$inc" "$WORK/khronos"
    for pin in EGL-Registry:e964211ff5f7c2bcada786954a16d5b813379395 OpenGL-Registry:6af574a14089ccfee87efe230ebcdd8742859813; do
      git init -q "$WORK/khronos/${pin%%:*}"; git -C "$WORK/khronos/${pin%%:*}" fetch -q --depth 1 "https://github.com/KhronosGroup/${pin%%:*}" "${pin##*:}"
      git -C "$WORK/khronos/${pin%%:*}" checkout -q --detach FETCH_HEAD
    done
    cp -R "$WORK/khronos/EGL-Registry/api/EGL" "$WORK/khronos/EGL-Registry/api/KHR" "$inc/"
    cp -R "$WORK/khronos/OpenGL-Registry/api/GLES2" "$WORK/khronos/OpenGL-Registry/api/GLES3" "$WORK/khronos/OpenGL-Registry/api/GL" "$inc/"
    touch "$inc/.done"
  fi
}
case " ${ONLY:-mupen64plus_next ppsspp flycast} " in *" mupen64plus_next "*|*" ppsspp "*|*" flycast "*) khronos_headers;; esac
echo "$CORES" | while IFS='|' read -r id repo commit dir mk so; do
  [ -n "$id" ] || continue
  if [ -n "$ONLY" ]; then case " $ONLY " in *" $id "*) ;; *) continue;; esac; fi
  src=$WORK/$id
  if [ ! -d "$src/.git" ] || [ "$(git -C "$src" rev-parse HEAD 2>/dev/null)" != "$commit" ]; then
    rm -rf "$src"; git init -q "$src"
    git -C "$src" fetch -q --depth 1 "$repo" "$commit"
    git -C "$src" checkout -q --detach FETCH_HEAD
  fi
  # The 3D cores take only the submodules they build with (not Vulkan, SDL, Discord, other platforms' tools), recursively.
  SUBS=; REC=
  case $id in
    flycast) SUBS="core/deps/asio core/deps/libchdr core/deps/rcheevos core/deps/tinygettext core/deps/xbyak"; REC=--recursive;;
    ppsspp) SUBS="ext/aemu_postoffice ext/armips ext/cpu_features ext/glslang ext/libchdr ext/lua ext/miniupnp ext/naett ext/OpenXR-SDK ext/rapidjson ext/rcheevos ext/SPIRV-Cross ext/zstd ext/freetype ffmpeg libretro/libretro-common"; REC=--recursive;;
  esac
  # shellcheck disable=SC2086
  git -C "$src" submodule update -q --init --depth 1 $REC $SUBS 2>/dev/null || true
  [ "$(git -C "$src" rev-parse HEAD)" = "$commit" ] || { echo "brick-build-cores: $id is not at $commit" >&2; exit 1; }
  case $id in fake08|tic80) git -C "$src" submodule update --init --depth 1;; esac
  git -C "$src" clean -fdxq
  # (and submodules back to their commit: a failed earlier run may have edited tracked files, e.g. cmake renaming zconf.h)
  git -C "$src" submodule foreach -q --recursive "git reset -q --hard; git clean -fdxq" >/dev/null 2>&1 || true
  git -C "$src" reset -q --hard
  echo "== $id $commit"
  # The 3D cores: their source is taken here, before the build (cmake edits the tree; ppsspp's ffmpeg is cut to linux/aarch64); the others'
  # after it (brick-core-source.py, with what CMake and ScummVM fetch).
  if [ "$mk" = cmake3d ]; then
    # Corresponding source: the commit's tree with its submodules' files (picodrive's libchdr, zstd and lzma are submodules), taken before the build (cmake edits files in the tree); deterministic.
    # (ppsspp: its ffmpeg submodule carries every platform's prebuilt libraries, 1.5 GB; only the source and linux/aarch64, the one linked, go in.)
    ( cd "$src" && { git ls-files -z --recurse-submodules >"$WORK/$id.files" 2>/dev/null || git ls-files -z >"$WORK/$id.files"; cat "$WORK/$id.files"; } | python3 -I -c '
  import sys, tarfile, gzip, os, re
  skip = re.compile(rb"^ffmpeg/(android|Windows|Windows10|WindowsInclude|ios|macosx|tvos|symbian|blackberry|wiiu|linux/(?!aarch64/))") if sys.argv[2] == "ppsspp" else None
  names = sorted(n for n in sys.stdin.buffer.read().split(b"\0") if n and not (skip and skip.match(n)))
  with gzip.GzipFile(fileobj=open(sys.argv[1], "wb"), mode="wb", mtime=0) as gz, tarfile.open(fileobj=gz, mode="w") as t:
      for n in names:
          n = os.fsdecode(n)
          if os.path.isdir(n) and not os.path.islink(n): continue   # a submodule'"'"'s gitlink: not part of this tree
          i = t.gettarinfo(n); i.mtime = 1577836800; i.uid = i.gid = 0; i.uname = i.gname = ""
          t.addfile(i, open(n, "rb") if i.isreg() else None)
  ' "$OUT/sources/$id-${commit%${commit#???????}}.tar.gz" "$id" )
  fi
  # platform=unix: the plain GNU/Linux branch of every libretro Makefile; ARCH tells those that look at it not to guess x86.
  # Per-core extras (XARGS). pcsx_rearmed keeps -shared in its own LDFLAGS: ours go to EXTRA_LDFLAGS there; the others take LDFLAGS.
  XARGS=("LDFLAGS=-Wl,-z,noexecstack -Wl,-z,nodelete -s")
  # (and no physical CD drive support: it needs libretro-common pieces the unix build does not compile, and a handheld has no drive)
  # gpSP: the Makefile only knows the aarch64 dynarec for some platforms; the Brick's CPU is one of them, so say so.
  [ "$id" = gpsp ] && XARGS+=(CPU_ARCH=arm64 HAVE_DYNAREC=1)
  # gpSP's dynarec calls __clear_cache after writing code; zig's does not sync on aarch64-linux (SIGILL in the first generated block),
  # so gpSP gets scripts/brick-clear-cache.h in its place (see the header). pcsx_rearmed's ari64 has its own sync (clear_cache_arm64) and
  # breaks on a forced include (pthread_setname_np needs _GNU_SOURCE before the first header), so only gpSP sees it.
  BRICK_CC_EXTRA=; case $id in picodrive) BRICK_CC_EXTRA="-D_GNU_SOURCE=1 -D__builtin___clear_cache=brick_clear_cache -include $(cd "$(dirname "$0")" && pwd)/brick-clear-cache.h";; gpsp) BRICK_CC_EXTRA="-D__clear_cache=brick_clear_cache -include $(cd "$(dirname "$0")" && pwd)/brick-clear-cache.h";; esac
  # FAKE-08 intentionally compiles its Lua .c files as C++; zig c++ needs an explicit language for those.
  BRICK_CXX_C=; [ "$id" != fake08 ] || BRICK_CXX_C=1
  export BRICK_CC_EXTRA BRICK_CXX_C
  # picodrive: its Makefile adds -shared to LDFLAGS itself, which our LDFLAGS on the command line would replace, so -shared is ours here.
  # Most added Makefiles put -shared inside LDFLAGS, replaced by our command-line value.
  # PicoDrive uses its AArch64 SH2 recompiler with the same explicit cache sync as gpSP.
  case $id in yabause|scummvm|fake08|dosbox_pure|gearsystem|beetle_supergrafx|beetle_vb|pokemini|beetle_ngp|beetle_wswan|beetle_pce_fast|handy|stella|prosystem|fbneo) XARGS=("LDFLAGS=-shared -fPIC -Wl,-z,noexecstack -Wl,-z,nodelete -s");; esac
  [ "$id" = picodrive ] && XARGS=("LDFLAGS=-shared -fPIC -Wl,-z,noexecstack -Wl,-z,nodelete -s" ARCH=aarch64 use_sh2drc=1)
  # FBNeo's HAVE_NEON adds a GCC-only ARM32 option; AArch64 SIMD stays available in zig.
  [ "$id" = fbneo ] && XARGS+=(HAVE_NEON=0)
  # mupen64plus_next: its own aarch64 GLES 3 target (dynarec aarch64, Cortex-A53). Our LDFLAGS replace the Makefile's, so its shared-object flags,
  # version script, -ldl/-lpthread, -lEGL and the Brick's libGLESv2 (found through $GLSTUB) are said here; HAVE_NEON stays off (the target does not set it; the flag set is GCC-only). CPUFLAGS: zig names the CPU cortex_a53 (the Makefile's -mcpu=cortex-a53 is read as a feature list).
  if [ "$id" = mupen64plus_next ]; then
    mkdir -p "$GLSTUB"; ln -sf "$SYSROOT/usr/lib/libGLESv2.so.2" "$GLSTUB/libGLESv2.so"; ln -sf "$SYSROOT/usr/lib/libEGL.so.1" "$GLSTUB/libEGL.so"
    XARGS=("LDFLAGS=-shared -fPIC -Wl,--version-script=libretro/link.T -Wl,-z,noexecstack -Wl,-z,nodelete -s -L$GLSTUB -lEGL -ldl -lpthread" HAVE_NEON=0 "platform=arm64_cortex_a53_gles3" "CPUFLAGS=-mcpu=cortex_a53")
    BRICK_CC_EXTRA="-idirafter $WORK/gles-include -D__clear_cache=brick_clear_cache -include $(cd "$(dirname "$0")" && pwd)/brick-clear-cache.h"; export BRICK_CC_EXTRA
  fi
  [ "$id" = pcsx_rearmed ] && XARGS=("EXTRA_LDFLAGS=-Wl,-z,noexecstack -Wl,-z,nodelete -s" HAVE_PHYSICAL_CDROM=0)
  [ "$id" = dosbox_pure ] && XARGS+=(ISMAC= ISWIN= "MAKE_CPUFLAGS=-fPIC")
  [ "$id" = yabause ] && XARGS+=(HAVE_SSE=0 DYNAREC=0)
  [ "$id" = scummvm ] && XARGS+=(core BUILD_64BIT=1 FORCE_OPENGLNONE=1 USE_LIBCO=0 "AR=$BIN/ar cru" "LDFLAGS=-shared -fPIC -Wl,-z,noexecstack -Wl,-z,nodelete -s -pthread -lm -ldl -Wl,--no-undefined")
  if [[ "$mk" = cmake-* ]]; then
    CARGS=(); CXX_EXTRA=
    case $id in
      tic80) CARGS=(-DBUILD_LIBRETRO=ON -DBUILD_SDL=OFF -DBUILD_EDITORS=OFF -DBUILD_PLAYER=OFF -DBUILD_TOOLS=OFF -DBUILD_STATIC=ON);;
      # fmt needs an explicit cstdlib include with this libc++; DS networking headers are unconditional upstream.
      melondsds) CXX_EXTRA="-include cstdlib"; CARGS=(-DENABLE_OPENGL=OFF -DENABLE_JIT=ON -DENABLE_NETWORKING=ON -DENABLE_TESTING=OFF -DENABLE_LTO=OFF -DENABLE_LTO_RELEASE=OFF);;
    esac
    if [ "$id" = melondsds ]; then
      while IFS='|' read -r dependency repository revision; do
        case $dependency in ''|\#*) continue;; esac
        [[ "$revision" =~ ^[a-f0-9]{40}$ ]] || { echo "invalid DS dependency pin: $dependency" >&2; exit 1; }
        key=$(printf '%s' "$dependency" | tr '[:lower:]-' '[:upper:]_')
        CARGS+=("-D${key}_REPOSITORY_URL=$repository" "-D${key}_REPOSITORY_TAG=$revision")
      done <"$(dirname "$0")/brick-melonds-deps.txt"
    fi
    ( cd "$src" && cmake -S . -B build -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
      -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_SYSTEM_PROCESSOR=aarch64 -DCMAKE_C_COMPILER="$BIN/cc" -DCMAKE_CXX_COMPILER="$BIN/c++" \
      -DCMAKE_AR="$BIN/ar" -DCMAKE_RANLIB="$BIN/ranlib" -DCMAKE_MAKE_PROGRAM="$MAKE" -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
      -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER -DCMAKE_C_FLAGS="-O2 -fPIC" -DCMAKE_CXX_FLAGS="-O2 -fPIC $CXX_EXTRA" \
      -DCMAKE_MODULE_LINKER_FLAGS="-s -Wl,-z,nodelete -Wl,-z,noexecstack" -DCMAKE_SHARED_LINKER_FLAGS="-s -Wl,-z,nodelete -Wl,-z,noexecstack" "${CARGS[@]}" >"$WORK/$id.log" 2>&1 && \
      cmake --build build --target "${id}_libretro" -j"$JOBS" >>"$WORK/$id.log" 2>&1 ) || { tail -30 "$WORK/$id.log" >&2; exit 1; }
  elif [ "$mk" = cmake3d ]; then
    # flycast (-DLIBRETRO=ON, USE_GLES: GLES 3) and ppsspp (libretro target, USING_GLES2 + ARM64): CMake, no host libraries (the find root is an empty
    # folder), no Vulkan, no extras; the Brick's libGLESv2/libEGL only as link stubs.
    # -ffile-prefix-map: the compiler would write the absolute build folder into assert and log strings; with it two builds in different folders give the same bytes.
    BRICK_CC_EXTRA="-idirafter $WORK/gles-include -ffile-prefix-map=$src=/src -ffile-prefix-map=$WORK=/work"; export BRICK_CC_EXTRA
    mkdir -p "$GLSTUB" "$WORK/empty-root"; ln -sf "$SYSROOT/usr/lib/libGLESv2.so.2" "$GLSTUB/libGLESv2.so"; ln -sf "$SYSROOT/usr/lib/libEGL.so.1" "$GLSTUB/libEGL.so"
    # Feature checks: flycast (libzip's memcpy_s, ...) must really link, so its test programs are executables (zig links them against glibc 2.33's names; none is run).
    TRYTYPE=STATIC_LIBRARY; [ "$id" = flycast ] && TRYTYPE=EXECUTABLE
    case $id in
      flycast) CM3=(-DLIBRETRO=ON -DUSE_GLES=ON -DUSE_VULKAN=OFF -DUSE_DX9=OFF -DUSE_DX11=OFF -DUSE_HOST_LIBZIP=OFF -DUSE_BREAKPAD=OFF -DUSE_LUA=OFF -DUSE_OPENMP=OFF -DUSE_DISCORD=OFF -DUSE_LIBCDIO=OFF -DUSE_ALSA=OFF -DUSE_LIBAO=OFF -DUSE_PULSEAUDIO=OFF);;
      ppsspp) CM3=(-DLIBRETRO=ON -DUSING_GLES2=ON -DARM_NO_VULKAN=ON -DUSE_WAYLAND_WSI=OFF -DUSING_X11_VULKAN=OFF -DUSE_DISCORD=OFF -DUNITTEST=OFF -DHEADLESS=OFF -DSIMULATOR=OFF);;
    esac
    ( cd "$src" && rm -rf build && cmake -S . -B build -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_SYSTEM_NAME=Linux \
        -DCMAKE_SYSTEM_PROCESSOR=aarch64 -DCMAKE_C_COMPILER="$BIN/cc" -DCMAKE_CXX_COMPILER="$BIN/c++" -DCMAKE_AR="$BIN/ar" \
        -DCMAKE_RANLIB="$BIN/ranlib" -DCMAKE_MAKE_PROGRAM="$MAKE" -DCMAKE_TRY_COMPILE_TARGET_TYPE=$TRYTYPE \
        -DCMAKE_FIND_ROOT_PATH="$WORK/empty-root" -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER -DCMAKE_FIND_ROOT_PATH_MODE_LIBRARY=ONLY \
        -DCMAKE_FIND_ROOT_PATH_MODE_INCLUDE=ONLY -DCMAKE_FIND_ROOT_PATH_MODE_PACKAGE=ONLY \
        -DCMAKE_C_FLAGS="-O3 -fPIC" -DCMAKE_CXX_FLAGS="-O3 -fPIC" -DCMAKE_SHARED_LINKER_FLAGS="-s -Wl,-z,noexecstack -Wl,-z,nodelete -L$GLSTUB" \
        "${CM3[@]}" >"$WORK/$id.log" 2>&1 && \
      cmake --build build --target "${so%.so}" -j"$JOBS" >>"$WORK/$id.log" 2>&1 ) || { tail -30 "$WORK/$id.log" >&2; echo "brick-build-cores: $id failed (log $WORK/$id.log)" >&2; exit 1; }
  elif [ "$mk" = cmake ]; then
    # mGBA: upstream's CMake build of the libretro core, on its bundled zlib/libpng/lzma (no host libraries, no extras).
    ( cd "$src" && rm -rf build && cmake -S . -B build -G "Unix Makefiles" -DCMAKE_BUILD_TYPE=Release -DCMAKE_POLICY_VERSION_MINIMUM=3.5 -DCMAKE_SYSTEM_NAME=Linux \
        -DCMAKE_SYSTEM_PROCESSOR=aarch64 -DCMAKE_C_COMPILER="$BIN/cc" -DCMAKE_CXX_COMPILER="$BIN/c++" -DCMAKE_AR="$BIN/ar" \
        -DCMAKE_RANLIB="$BIN/ranlib" -DCMAKE_MAKE_PROGRAM="$MAKE" -DCMAKE_TRY_COMPILE_TARGET_TYPE=STATIC_LIBRARY \
        -DCMAKE_FIND_ROOT_PATH_MODE_PROGRAM=NEVER -DCMAKE_C_FLAGS="-O3 -fPIC" -DCMAKE_SHARED_LINKER_FLAGS="-s -Wl,-z,nodelete" -DBUILD_LIBRETRO=ON -DBUILD_SHARED=OFF -DBUILD_STATIC=OFF \
        -DBUILD_QT=OFF -DBUILD_SDL=OFF -DBUILD_GL=OFF -DBUILD_GLES2=OFF -DBUILD_GLES3=OFF -DBUILD_TEST=OFF -DBUILD_EXAMPLE=OFF \
        -DBUILD_PERF=OFF -DBUILD_HEADLESS=OFF -DUSE_FFMPEG=OFF -DUSE_EDITLINE=OFF -DUSE_SQLITE3=OFF -DUSE_ELF=OFF -DUSE_LIBZIP=OFF \
        -DUSE_MINIZIP=OFF -DENABLE_SCRIPTING=OFF -DUSE_DISCORD_RPC=OFF -DUSE_DEBUGGERS=OFF >"$WORK/$id.log" 2>&1 && \
      cmake --build build --target mgba_libretro -j"$JOBS" >>"$WORK/$id.log" 2>&1 ) || { tail -30 "$WORK/$id.log" >&2; echo "brick-build-cores: $id failed (log $WORK/$id.log)" >&2; exit 1; }
  else
    ( cd "$src/$dir" && $MAKE -f "$mk" -j"$JOBS" platform=unix ARCH=arm64 \
      CC="$BIN/cc" CXX="$BIN/c++" AR="$BIN/ar" RANLIB="$BIN/ranlib" STRIP=true \
      HAVE_NEON=1 "${XARGS[@]}" >"$WORK/$id.log" 2>&1 ) || { tail -30 "$WORK/$id.log" >&2; echo "brick-build-cores: $id failed (log $WORK/$id.log)" >&2; exit 1; }
  fi
  if [ "$id" = scummvm ]; then
    python3 -I "$(dirname "$0")/brick-scummvm-data.py" "$src" "$OUT/assets"
  fi
  # Include sources fetched by CMake/ScummVM as well as submodules, excluding build outputs.
  [ "$mk" = cmake3d ] || python3 -I "$(dirname "$0")/brick-core-source.py" "$src" "$OUT/sources/$id-${commit%${commit#???????}}.tar.gz"
  f=$(find "$src" -name "$so" -type f | head -1)
  [ -n "$f" ] || { echo "brick-build-cores: $id built no $so" >&2; exit 1; }
  cp "$f" "$OUT/cores/${id}_libretro.so"
  so=${id}_libretro.so
  # Licence files of the core itself (every top-level file whose name says licence/copying/copyright/notice, plus libretro/).
  mkdir -p "$OUT/licences/$id"
  ( cd "$src" && for l in $(ls | grep -i -E '^(licen[sc]e|copying|copyright|notice|authors|readme-license)' ); do [ -f "$l" ] && cp "$l" "$OUT/licences/$id/"; done
    find . -type f \( -iname 'license*' -o -iname 'licence*' -o -iname 'copying*' -o -iname 'copyright*' -o -iname 'notice*' \) ! -path '*/.git/*' | while read -r p; do
      d="$OUT/licences/$id/$(dirname "${p#./}" | tr '/' '_')"; mkdir -p "$d"; cp "$p" "$d/"; done )
  # Handy carries its zlib notice in source headers rather than a top-level licence file.
  if [ "$id" = handy ]; then
    cp "$src/lynx/ram.h" "$OUT/licences/$id/ram.h"
  fi
  digest=$(shasum -a 256 "$OUT/cores/$so" 2>/dev/null | cut -d' ' -f1 || sha256sum "$OUT/cores/$so" | cut -d' ' -f1)
  { awk -v id="$id" '$1 != id' "$OUT/CORES.txt"; echo "$id $commit $so $digest"; } | LC_ALL=C sort >"$OUT/CORES.next"
  mv "$OUT/CORES.next" "$OUT/CORES.txt"
done
cat "$OUT/CORES.txt"
