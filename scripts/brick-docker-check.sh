#!/bin/bash
# The card's real programs with the Brick's own C library, on an aarch64 Docker host (an Apple-silicon Mac with Colima), no Brick:
#   scripts/brick-docker-check.sh CARD_DIR SYSROOT ROMS_DIR
# CARD_DIR  the folder scripts/brick-make-card.sh built; SYSROOT / ROMS_DIR as for brick-core-check.sh.
# One container (Alpine's BusyBox as the shell) whose dynamic loader is the Brick's ld-2.33.so and whose library path is the Brick's
# libraries; the card is mounted where the Brick mounts it. Then:
#   1. the card's own updater runs for 25 s with a file as the screen: pocket-ui comes up on pocket-fb, and the screen is not empty
#   2. for each core: pocket-core with that core and a test ROM for 6 s on pocket-fb, the CPU copy, with its numbers
# Prints PASS/FAIL lines; exit 0 only when all pass.
set -u
CARD=$(cd "${1:?card dir}" && pwd); SYS=$(cd "${2:?sysroot}" && pwd); ROMS=$(cd "${3:?roms dir}" && pwd)
W=$(mktemp -d "${BRICK_CHECK_TMP:-$HOME}/.brick-docker.XXXXXX"); trap 'rm -rf "$W"' EXIT
mkdir -p "$W/fw/lib" "$W/card" "$W/out"
cp -a "$SYS"/lib/. "$W/fw/lib/"; cp -a "$SYS"/usr/lib/. "$W/fw/lib/"
cp -a "$CARD"/. "$W/card/"
mkdir -p "$W/card/Roms/gba" "$W/card/Roms/snes" "$W/card/Roms/gb" "$W/card/Roms/nes"
mkdir -p "$W/card/Roms/psx"; [ -f "$ROMS/psx-test.exe" ] && cp "$ROMS/psx-test.exe" "$W/card/Roms/psx/"; cp "$ROMS/gba-test.gba" "$W/card/Roms/gba/"; cp "$ROMS/snes-test.sfc" "$W/card/Roms/snes/"; cp "$ROMS/gb.gb" "$W/card/Roms/gb/"; cp "$ROMS/nes.nes" "$W/card/Roms/nes/"
for t in megadrive:md-test.md mastersystem:sms-test.sms gamegear:gg-test.gg; do mkdir -p "$W/card/Roms/${t%%:*}"; [ -f "$ROMS/${t#*:}" ] && cp "$ROMS/${t#*:}" "$W/card/Roms/${t%%:*}/"; done
head -c $((1024 * 768 * 4)) /dev/zero >"$W/out/fb"
cat >"$W/inside.sh" <<'IN'
#!/bin/sh
P=/mnt/SDCARD/pocket-ui; fails=0
pass() { echo "PASS $*"; }; fail() { echo "FAIL $*"; fails=$((fails + 1)); }
mkdir -p /storage /tmp/pocket-ui
mount --bind $P/storage /storage 2>/dev/null && mount --bind /mnt/SDCARD/Roms /storage/roms && mkdir -p /mnt/SDCARD/Saves/savestates /storage/roms/savestates && mount --bind /mnt/SDCARD/Saves/savestates /storage/roms/savestates && mount --bind $P/app/cores /storage/cores || echo "(no bind mounts: Docker without privileges, the updater makes them itself on the Brick)"
echo "== 1. the updater"
BRICK_FB=/out/fb BRICK_FB_MODE=1024x768x32 BRICK_FIRMWARE=/fw BRICK_BATTERY=/tmp/bat BRICK_NO_POWEROFF=1 BRICK_SHOW_FOR=1 \
  BRICK_NO_MOUNT=$([ -d /storage/roms/gba ] && echo 1) timeout 25 sh /mnt/SDCARD/.tmp_update/updater >/tmp/updater.out 2>&1
LOG=$P/logs/boot.log
grep -q "pocket-ui starts" $LOG && pass "the updater started pocket-ui" || fail "no start in the log"
grep -q "pocket-ui ended" $LOG && fail "pocket-ui ended by itself: $(grep 'ended' $LOG | head -1)" || pass "pocket-ui ran the whole 25 s"
grep -q "pocket-fb did not come up" $LOG && fail "the bridge did not come up" || pass "the bridge came up"
n=$(tr -d '\000' </out/fb | wc -c)
[ "$n" -gt 100000 ] && pass "the screen is painted ($n non-zero bytes of 3145728)" || fail "the screen is empty ($n non-zero bytes)"
echo "--- boot.log"; tail -12 $LOG
echo "== 2. the cores, a game each, CPU copy"
export LD_LIBRARY_PATH=$P/app/lib:/fw/lib WAYLAND_DISPLAY=/tmp/pk-wl SWAYSOCK=/tmp/pk-sway XDG_RUNTIME_DIR=/tmp POCKET_CORE_SOFTWARE=1
$P/app/bin/pocket-fb --wayland /tmp/pk-wl --ipc /tmp/pk-sway --fb /out/fb --mode 1024x768x32 >/tmp/fb2.out 2>&1 &
FBPID=$!; sleep 1
for t in gambatte:gb/gb.gb:gb mgba:gba/gba-test.gba:gba nestopia:nes/nes.nes:nes snes9x:snes/snes-test.sfc:snes pcsx_rearmed:psx/psx-test.exe:psx gpsp:gba/gba-test.gba:gba fceumm:nes/nes.nes:nes picodrive:megadrive/md-test.md:megadrive picodrive:mastersystem/sms-test.sms:mastersystem picodrive:gamegear/gg-test.gg:gamegear; do
  core=${t%%:*}; rest=${t#*:}; rom=${rest%%:*}; sys=${rest#*:}
  out=$(timeout 40 $P/app/bin/pocket-core $P/app/cores/${core}_libretro.so /mnt/SDCARD/Roms/$rom --system $sys --bench 6 2>&1); rc=$?
  echo "--- $core: exit $rc"; echo "$out" | grep -E 'frames|emulated|first frame|panic|error|late|underrun' | head -5
  [ $rc = 0 ] && pass "$core ran its bench" || fail "$core exited $rc"
done
kill $FBPID 2>/dev/null
[ "$fails" = 0 ] && echo "ALL PASS" || echo "$fails FAILED"
exit $fails
IN
chmod +x "$W/inside.sh"
docker run --rm --platform linux/arm64 --privileged -v "$W/card":/mnt/SDCARD -v "$W/fw":/fw:ro -v "$SYS/lib/ld-2.33.so":/lib/ld-linux-aarch64.so.1:ro \
  -v "$W/out":/out -v "$W/inside.sh":/inside.sh:ro alpine:latest sh /inside.sh
