#!/bin/bash
# Proves the Brick's cores load and run a frame against the Brick's own C library:
#   scripts/brick-core-check.sh CORES_DIR SYSROOT ROMS_DIR
# CORES_DIR  the *_libretro.so files (scripts/brick-build-cores.sh)
# SYSROOT    a copy of the Brick's /lib and /usr/lib (ld-2.33.so, libc, libm, libdl, libpthread, libgcc_s, libstdc++, ...; read
#            from the device, not part of this repository)
# ROMS_DIR   (psx-test.exe is made here if absent) gb.gb, nes.nes (original endless-loop ROMs, see below), gba-test.gba, snes-test.sfc, md-test.md, sms-test.sms and gg-test.gg (scripts/testroms.py)
# Needs zig and an aarch64 Docker (an Apple-silicon Mac with Colima). The test frontend (brick-core-check.c) is built for glibc 2.33
# and started with the Brick's own ld.so, so a symbol the Brick's libc lacks makes dlopen fail here as it would there.
# Also prints the newest GLIBC/GLIBCXX version every core asks for (must be <= 2.33 / 3.4.28, the Brick's libs).
set -eu
CORES=$(cd "${1:?cores dir}" && pwd); SYS=$(cd "${2:?sysroot}" && pwd); ROMS=$(cd "${3:?roms dir}" && pwd)
HERE=$(cd "$(dirname "$0")" && pwd)
W=$(mktemp -d "${BRICK_CHECK_TMP:-$HOME}/.brick-check.XXXXXX"); trap 'rm -rf "$W"' EXIT
zig cc -target aarch64-linux-gnu.2.33 -O1 -o "$W/check" "$HERE/brick-core-check.c" -ldl
bad=0
for so in "$CORES"/*_libretro.so; do
  n=$(basename "$so")
  g=$(strings -a "$so" | grep -o 'GLIBC_[0-9.]*' | sort -uV | tail -1); x=$(strings -a "$so" | grep -o 'GLIBCXX_[0-9.]*' | sort -uV | tail -1)
  echo "$n needs ${g:-no glibc symbol} ${x:+and $x}"
  python3 -I - "$g" "$x" <<'PYVERS' || bad=1
import sys
for value, limit in zip(sys.argv[1:], [(2, 33), (3, 4, 28)]):
    if value and tuple(map(int, value.split("_")[1].split("."))) > limit:
        raise SystemExit("FAIL newer symbol: " + value)
PYVERS
done
run() {  # core rom frames
  rom=/roms/$2; case $2 in --load-only|--no-content) rom=$2;; esac
  docker run --rm --name "brick-core-check-$$" --platform linux/arm64 -v "$SYS":/brick:ro -v "$CORES":/cores:ro -v "$ROMS":/roms:ro -v "$W":/w:ro alpine:latest \
    /brick/lib/ld-2.33.so --library-path /brick/lib:/brick/usr/lib /w/check "/cores/$1" "$rom" "${3:-120}"
}
# An original PlayStation program (a PS-X EXE that jumps to itself): enough for the core to start on its built-in BIOS and show frames.
[ -f "$ROMS/psx-test.exe" ] || python3 -I -c '
import struct, sys
h = bytearray(2048); h[:8] = b"PS-X EXE"
struct.pack_into("<IIIIII", h, 0x10, 0x80010000, 0, 0x80010000, 2048, 0, 0)    # pc, gp, load address, size, (data)
struct.pack_into("<I", h, 0x30, 0x801FFF00)                                    # stack
body = bytearray(2048); struct.pack_into("<II", body, 0, 0x08004000, 0)        # j 0x80010000 ; nop
open(sys.argv[1], "wb").write(bytes(h) + bytes(body))' "$ROMS/psx-test.exe"
# Every built core must resolve symbols, initialise and unload, even without a test ROM.
for so in "$CORES"/*_libretro.so; do
  printf "%s " "$(basename "$so")"
  run "$(basename "$so")" --load-only || { echo "FAIL $(basename "$so"): load/init/deinit/dlclose"; bad=1; }
done
if [ -f "$CORES/scummvm_libretro.so" ]; then
  echo "scummvm GUI (no game data)"
  run scummvm_libretro.so --no-content 120 || bad=1
fi
for pair in picodrive:32x-test.32x gearsystem:sg-test.sg beetle_vb:vb-test.vb melondsds:delve-ndsmode.nds melondsds:MinecraftDS-ndsmode.nds fake08:pico-test.p8 tic80:tic-test.tic dosbox_pure:dos-test.com beetle_supergrafx:sgx-test.sgx beetle_pce_fast:pce-test.pce beetle_wswan:ws-test.ws stella:a2600-test.a26 gambatte:gb.gb mgba:gba-test.gba nestopia:nes.nes snes9x:snes-test.sfc pcsx_rearmed:psx-test.exe gpsp:gba-test.gba fceumm:nes.nes picodrive:md-test.md picodrive:sms-test.sms picodrive:gg-test.gg; do
  [ -f "$CORES/${pair%%:*}_libretro.so" ] || continue
  core=${pair%%:*}; rom=${pair#*:}
  [ -f "$ROMS/$rom" ] || { echo "SKIP $core frames: no $rom (load-only was checked)"; continue; }
  printf '%-10s %-14s ' "$core" "$rom"
  run "${core}_libretro.so" "$rom" 120 || bad=1
done
exit $bad
