#!/bin/bash
# Host tests of the Brick standalone card's start script and its stand-ins (docs/TRIMUI-BRICK-STANDALONE.md), no Brick needed:
# the updater runs in a fake card with stand-in programs, once under dash and once under BusyBox ash (the Brick's shell).
#   scripts/test-brick-card.sh
# What they prove: a missing part ends on a picture (never a black screen, never a loop); a pocket-ui that keeps ending is
# tried three times and then replaced by the "does not start" picture; a healthy pocket-ui is started again when it ends and gets
# the environment it needs; the stand-in helpers answer the way pocket-ui asks (volume, mute, brightness, power, Wi-Fi switch).
# What they cannot prove: anything about the real hardware (see the doc's "Only a real boot can answer").
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
fails=0
ok() { echo "ok   $*"; }
bad() { echo "FAIL $*"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else bad "$1: $2"; fi; }

WORK=$(mktemp -d "${TMPDIR:-/tmp}/brick-card-test.XXXXXX"); trap 'rm -rf "$WORK"' EXIT

# Exercise power loss with tiny program folders: no daemons, hardware or sleeps.
# BRICK_BOOT_TESTS_ONLY=1 runs just this recovery suite while iterating.
asset_recovery_tests() {
  echo '== boot asset recovery: interrupted moves, activation and rollback'
  B=$WORK/recovery
  mkdir -p "$WORK/recovery-tools"
  # sync is a durability boundary on the card; host tests model the persisted
  # states on either side without flushing every unrelated host filesystem.
  printf '#!/bin/sh\nexit 0\n' >"$WORK/recovery-tools/sync"
  chmod +x "$WORK/recovery-tools/sync"
  recovery_fixture() {
    rm -rf "$B"; mkdir -p "$B/app/cores" "$B/app/bios-data/nested" "$B/app/bin" "$B/app.new" "$B/storage/pocket-ui"
    cp "$ROOT/deploy/brick/boot.sh" "$B/start.sh"
    printf '#!/bin/sh\necho old >>"$BRICK_DATA/started"\nexit 0\n' >"$B/app/start.sh"
    printf '#!/bin/sh\necho new >>"$BRICK_DATA/started"\nrm -f "$BRICK_DATA/storage/pocket-ui/update-pending"\nexit 0\n' >"$B/app.new/start.sh"
    echo 1 >"$B/app/RELEASE"; echo 2 >"$B/app.new/RELEASE"
    for path in cores/test.so bios-data/nested/test.dat bin/curl; do
      printf 'original %s\n' "$path" >"$B/app/$path"
    done
    printf '%s\n' cores/test.so bios-data/nested/test.dat bin/curl >"$B/app/.asset-moves"
    : >"$B/app.new/.ready"
  }
  move_assets() { # number, old, new
    n=0
    while IFS= read -r path; do
      [ "$n" -lt "$1" ] || break
      mkdir -p "$3/${path%/*}"; mv "$2/$path" "$3/$path"; n=$((n + 1))
    done <"$2/.asset-moves"
  }
  boot_recovery() {
    PATH="$WORK/recovery-tools:$PATH" BRICK_DATA=$B timeout 5 $SH "$B/start.sh" >"$B/output" 2>&1
    boot_rc=$?
  }
  originals_present() {
    for path in cores/test.so bios-data/nested/test.dat bin/curl; do
      [ "$(cat "$B/app/$path" 2>/dev/null)" = "original $path" ] || return 1
    done
  }
  rollback_done() {
    [ "$boot_rc" = 0 ] && originals_present && [ "$(cat "$B/app/RELEASE")" = 1 ] &&
      [ "$(cat "$B/storage/pocket-ui/rolled-back")" = 2 ] && [ ! -e "$B/storage/pocket-ui/update-pending" ] &&
      [ ! -e "$B/app/.restore-assets" ] && [ ! -e "$B/app/.asset-moves" ] && grep -qx old "$B/started"
  }
  for SH in dash 'busybox ash'; do
    echo "-- asset recovery under $SH"
    for count in 0 1 2 3; do
      recovery_fixture; move_assets "$count" "$B/app" "$B/app.new"; boot_recovery
      check "asset move $count/3 interrupted: old version has every original and staged readiness is revoked" \
        '[ "$boot_rc" = 0 ] && originals_present && grep -qx old "$B/started" && [ ! -e "$B/app/.asset-moves" ] && [ ! -e "$B/app.new/.ready" ]'
      boot_recovery
      check "asset move $count/3 recovery is idempotent" '[ "$boot_rc" = 0 ] && originals_present'
    done
    recovery_fixture
    special='bios-data/nested/0 with [brackets] ü'
    echo special >"$B/app/$special"; printf '%s\n' "$special" >>"$B/app/.asset-moves"
    move_assets 4 "$B/app" "$B/app.new"; boot_recovery
    check 'safe asset names with spaces, brackets, zero and non-ASCII bytes restore literally' \
      '[ "$boot_rc" = 0 ] && [ "$(cat "$B/app/$special")" = special ] && originals_present'
    # If both trees have a file, the original old file wins without deleting the other copy.
    recovery_fixture; mkdir -p "$B/app.new/cores"; echo replacement >"$B/app.new/cores/test.so"; boot_recovery
    check 'interrupted move preserves the old original when both files exist' \
      'originals_present && [ "$(cat "$B/app.new/cores/test.so")" = replacement ]'
    for count in 0 1 2 3; do
      recovery_fixture; move_assets "$count" "$B/app" "$B/app.new"; rm "$B/app.new/.ready"
      mv "$B/app" "$B/app.previous"; echo 2 >"$B/storage/pocket-ui/update-pending"; boot_recovery
      check "incomplete activation after $count/3 moves restores previous before selecting it" \
        '[ "$boot_rc" = 0 ] && originals_present && grep -qx old "$B/started" && [ ! -e "$B/storage/pocket-ui/update-pending" ]'
    done
    recovery_fixture; move_assets 3 "$B/app" "$B/app.new"; mv "$B/app" "$B/app.previous"
    echo 2 >"$B/storage/pocket-ui/update-pending"; boot_recovery
    check 'ready activation between swaps boots the new version and retains the old manifest for rollback' \
      '[ "$boot_rc" = 0 ] && grep -qx new "$B/started" && [ "$(cat "$B/app/cores/test.so")" = "original cores/test.so" ] && [ -f "$B/app.previous/.asset-moves" ]'
    recovery_fixture; move_assets 3 "$B/app" "$B/app.new"; mv "$B/app" "$B/app.previous"
    echo ../outside >>"$B/app.previous/.asset-moves"; boot_recovery
    check 'ready activation refuses a malformed previous manifest without swapping either tree' \
      '[ "$boot_rc" = 1 ] && [ ! -e "$B/app" ] && [ ! -e "$B/started" ] && [ -f "$B/app.new/cores/test.so" ] && [ -f "$B/app.previous/.asset-moves" ]'
    # New version exits before confirming its first picture: all reusable files return to the old app.
    recovery_fixture; move_assets 3 "$B/app" "$B/app.new"
    printf '#!/bin/sh\nexit 139\n' >"$B/app.new/start.sh"
    mv "$B/app" "$B/app.previous"; mv "$B/app.new" "$B/app"; echo 2 >"$B/storage/pocket-ui/update-pending"; boot_recovery
    check 'failed new version restores every reused asset before rollback' 'rollback_done && [ "$(cat "$B/app.failed/RELEASE")" = 2 ]'
    # Persisted boundaries: marker created, each asset restored, first rename,
    # second rename, and cleanup already removed the manifest/pending marker.
    for boundary in marked restored-1 restored-2 restored-3 failed-renamed old-renamed cleaned; do
      recovery_fixture; move_assets 3 "$B/app" "$B/app.new"
      mv "$B/app" "$B/app.previous"; mv "$B/app.new" "$B/app"
      echo 2 >"$B/storage/pocket-ui/update-pending"; echo 2 >"$B/app.previous/.restore-assets"
      count=0
      case "$boundary" in restored-1) count=1;; restored-2) count=2;; restored-3|failed-renamed|old-renamed|cleaned) count=3;; esac
      n=0
      while IFS= read -r path; do
        [ "$n" -lt "$count" ] || break
        mv "$B/app/$path" "$B/app.previous/$path"; n=$((n + 1))
      done <"$B/app.previous/.asset-moves"
      case "$boundary" in failed-renamed|old-renamed|cleaned) mv "$B/app" "$B/app.failed";; esac
      case "$boundary" in old-renamed|cleaned) mv "$B/app.previous" "$B/app";; esac
      if [ "$boundary" = cleaned ]; then rm "$B/app/.asset-moves" "$B/storage/pocket-ui/update-pending"; fi
      boot_recovery
      check "rollback resumes at persisted boundary $boundary" 'rollback_done'
    done
    for invalid in '../outside' 'cores/../outside' 'cores/./test.so' 'cores//test.so' 'cores/' '/cores/test.so' 'bin/pocket-ui' 'storage/save' 'bin/curl/extra' 'cores/back\slash' $'cores/carriage\rreturn' ''; do
      recovery_fixture; move_assets 1 "$B/app" "$B/app.new"
      printf '%s\n' "$invalid" >>"$B/app/.asset-moves"; boot_recovery
      check "invalid manifest [$invalid] leaves both trees untouched and starts nothing" \
        '[ "$boot_rc" = 1 ] && [ ! -e "$B/started" ] && [ ! -e "$B/app/cores/test.so" ] && [ -f "$B/app.new/cores/test.so" ] && [ -f "$B/app.new/.ready" ] && [ -f "$B/app/.asset-moves" ]'
    done
    for invalid in missing truncated nul directory symlink-manifest symlink-file symlink-parent symlink-tree; do
      recovery_fixture; move_assets 1 "$B/app" "$B/app.new"
      case "$invalid" in
        missing) rm "$B/app/bin/curl";;
        directory) mkdir "$B/app/cores/directory"; echo cores/directory >>"$B/app/.asset-moves";;
        symlink-manifest) mv "$B/app/.asset-moves" "$B/outside-manifest"; ln -s "$B/outside-manifest" "$B/app/.asset-moves";;
        truncated) printf 'cores/no-newline' >>"$B/app/.asset-moves";;
        nul) printf 'cores/with\000nul\n' >>"$B/app/.asset-moves";;
        symlink-file) rm "$B/app/bin/curl"; ln -s "$B/app.new/cores/test.so" "$B/app/bin/curl";;
        symlink-parent) mv "$B/app/bios-data" "$B/outside"; ln -s "$B/outside" "$B/app/bios-data";;
        symlink-tree) mv "$B/app.new" "$B/outside"; ln -s "$B/outside" "$B/app.new";;
      esac
      boot_recovery
      check "manifest $invalid fails closed before any asset is restored" \
        '[ "$boot_rc" = 1 ] && [ ! -e "$B/started" ] && [ ! -e "$B/app/cores/test.so" ] && [ -f "$B/app.new/cores/test.so" ] && [ -f "$B/app/.asset-moves" ]'
    done
    # An embedded NUL must not become a different, otherwise valid existing path.
    recovery_fixture; printf 'cores/test\000.so\n' >"$B/app/.asset-moves"; boot_recovery
    check 'NUL cannot be silently stripped into an allowed existing asset' '[ "$boot_rc" = 1 ] && [ ! -e "$B/started" ] && [ -f "$B/app/.asset-moves" ]'
    # Malformed rollback manifest must not mark, move or delete any program tree.
    recovery_fixture; move_assets 3 "$B/app" "$B/app.new"; mv "$B/app" "$B/app.previous"; mv "$B/app.new" "$B/app"
    printf '#!/bin/sh\nexit 139\n' >"$B/app/start.sh"; echo 2 >"$B/storage/pocket-ui/update-pending"
    echo ../outside >>"$B/app.previous/.asset-moves"; boot_recovery
    check 'invalid rollback manifest leaves the failed and previous versions in place' \
      '[ "$boot_rc" = 1 ] && [ -f "$B/app/cores/test.so" ] && [ ! -e "$B/app.previous/cores/test.so" ] && [ ! -e "$B/app.previous/.restore-assets" ] && [ ! -e "$B/app.failed" ]'
    # Legacy updates have no move manifest and retain their complete copied old tree.
    recovery_fixture; rm "$B/app/.asset-moves"; cp -R "$B/app/cores" "$B/app.new/cores"
    printf '#!/bin/sh\nexit 139\n' >"$B/app.new/start.sh"
    mv "$B/app" "$B/app.previous"; mv "$B/app.new" "$B/app"; echo 2 >"$B/storage/pocket-ui/update-pending"; boot_recovery
    check 'legacy copy-based update still rolls back and keeps the original data' 'rollback_done'
  done
}
asset_recovery_tests
if [ "${BRICK_BOOT_TESTS_ONLY:-0}" = 1 ]; then
  if [ "$fails" = 0 ]; then echo 'test-brick-card recovery: all passed'; else echo "test-brick-card recovery: $fails FAILED"; exit 1; fi
  exit 0
fi

# A fake card: the real updater, stand-ins, the real messages and es_systems; the programs are scripts that write down what they got.
make_card() { # dir
  c=$1; p=$c/pocket-ui/app
  rm -rf "$c"; mkdir -p "$c/.tmp_update" "$p"/{bin,lib,cores,msg,hwroot/usr/bin,licences} "$c/pocket-ui/logs" "$WORK/fw/bin" "$WORK/fw/lib"
  cp "$ROOT/deploy/brick/boot.sh" "$c/pocket-ui/start.sh"; cp "$ROOT/deploy/brick/start.sh" "$p/start.sh"; mkdir -p "$c/trimui/app"
  cp "$ROOT/deploy/brick/entries/updater" "$c/.tmp_update/updater"
  cp "$ROOT/deploy/brick/entries/MainUI" "$ROOT/deploy/brick/entries/premainui.sh" "$ROOT/deploy/brick/entries/preload.sh" "$c/trimui/app/"
  cp "$ROOT"/deploy/brick/msg/* "$p/msg/"; cp "$ROOT/deploy/brick/es_systems.cfg" "$ROOT/deploy/brick/asoundrc" "$p/"
  cp "$ROOT"/deploy/brick/hwroot/usr/bin/* "$p/hwroot/usr/bin/"
  for core in gambatte mgba nestopia snes9x pcsx_rearmed gpsp fceumm picodrive beetle_ngp beetle_wswan beetle_pce_fast handy stella prosystem fbneo mupen64plus_next ppsspp flycast gearsystem beetle_supergrafx pokemini beetle_vb fake08 tic80 melondsds dosbox_pure scummvm yabause; do : >"$p/cores/${core}_libretro.so"; done
  for l in libwayland-client.so.0 libffi.so.8; do : >"$p/lib/$l"; done; echo test >"$p/VERSION"; echo 1 >"$p/RELEASE"
  cat >"$p/bin/pocket-fb" <<'FB'
#!/bin/sh
# the bridge: with --picture it writes down which picture; else it makes the two sockets and waits
echo "pocket-fb $*" >>"$BRICK_TEST_LOG"
case "$1" in
  --picture) sleep "${4:-0}"; exit 0 ;;
esac
exec python3 -I - "$2" "$4" <<'PY'
import socket, sys, time, os
socks = []
for p in sys.argv[1:3]:
    s = socket.socket(socket.AF_UNIX); s.bind(p); s.listen(1); socks.append(s)
time.sleep(600)
PY
FB
  cat >"$p/bin/brick-egl" <<'X'
#!/bin/sh
echo "brick-egl $*" >>"$BRICK_TEST_LOG"
exit "${BRICK_TEST_EGL_RC:-0}"
X
  cat >"$p/bin/pocket-core" <<'X'
#!/bin/sh
exit 0
X
  cat >"$p/bin/brick-hw" <<'X'
#!/bin/sh
echo "brick-hw $*" >>"$BRICK_TEST_LOG"
[ "$1" = watch ] && exec sleep 600
exit 0
X
  chmod +x "$p"/bin/* "$c/.tmp_update/updater" "$c"/trimui/app/* "$p/start.sh" "$c/pocket-ui/start.sh"
  cat >"$WORK/fw/bin/trimui_inputd" <<'X'
#!/bin/sh
echo "trimui_inputd" >>"$BRICK_TEST_LOG"; exec sleep 600
X
  chmod +x "$WORK/fw/bin/trimui_inputd"
}
fake_pocket_ui() { # card, script body [, program folder]
  printf '#!/bin/sh\n%s\n' "$2" >"$1/pocket-ui/${3:-app}/bin/pocket-ui"; chmod +x "$1/pocket-ui/${3:-app}/bin/pocket-ui"
}
run_updater() { # shell card seconds -> exit code; the environment moves every path
  sh=$1; c=$2; limit=$3
  : >"$WORK/log"
  BRICK_TEST_LOG=$WORK/log BRICK_CARD=$c BRICK_FB=$WORK/fb BRICK_FBSYS=$WORK/fbsys BRICK_RUN=$WORK/run BRICK_FIRMWARE=$WORK/fw BRICK_STORAGE=$WORK/storage \
  BRICK_BATTERY=$WORK/bat BRICK_RFKILL=$WORK/rfk BRICK_NO_MOUNT=1 BRICK_NO_POWEROFF=1 BRICK_SHOW_FOR=1 BRICK_FB_BYTES=64 BRICK_FB_MODE=8x2x32 \
  BRICK_FW_ZONEINFO=$WORK/zoneinfo BRICK_FW_LOCALTIME=$WORK/fw-localtime BRICK_NTPD=$WORK/no-ntpd BRICK_NTP_DELAY=0 \
  BRICK_AMIXER=$WORK/amixer BRICK_SPEAKER=$WORK/mute BRICK_PIDOF=${BRICK_PIDOF:-$WORK/no-pidof} BRICK_KILLALL=${BRICK_KILLALL:-true} \
  setsid timeout "$limit" $sh "$c/${ENTRY:-.tmp_update/updater}" >"$WORK/out" 2>&1 &
  pid=$!; wait "$pid"; rc=$?
  kill -TERM -- "-$pid" 2>/dev/null     # whatever the updater left running (bridge, daemons) was started by this run
  echo $rc
}
mkdir -p "$WORK/fbsys" "$WORK/bat" "$WORK/rfk"; echo 55 >"$WORK/bat/capacity"; echo Discharging >"$WORK/bat/status"
# The card's helpers ask pidof whether wpa_supplicant and udhcpc run; this computer's own (a build machine on Wi-Fi) must not answer.
printf '#!/bin/sh\nexit 1\n' >"$WORK/no-pidof"; chmod +x "$WORK/no-pidof"
# Fake firmware zone data (two zones): the card's own pocket-ui/app/zoneinfo wins when it is there (check 5c puts one on the card).
# The firmware's link goes through /tmp as on the Brick (/etc/localtime -> /tmp/localtime -> /usr/share/zoneinfo/...).
mkdir -p "$WORK/zoneinfo/Europe"; : >"$WORK/zoneinfo/Europe/London"; : >"$WORK/zoneinfo/UTC"
ln -s /usr/share/zoneinfo/Europe/London "$WORK/fw-tmp-localtime"; ln -s "$WORK/fw-tmp-localtime" "$WORK/fw-localtime"
printf '#!/bin/sh\nexit 0\n' >"$WORK/amixer"; chmod +x "$WORK/amixer"

for SH in dash "busybox ash"; do
  echo "== updater under $SH"
  SHBIN=$SH
  C=$WORK/card; make_card "$C"

  # 1. A part missing: the picture, no loop, and the log says which part.
  rm -f "$C/pocket-ui/app/bin/pocket-ui"
  rc=$(run_updater "$SHBIN" "$C" 45)
  check "missing binary: ends by itself with 1 (not a hang)" "[ $rc = 1 ]"
  check "missing binary: the log names it" "grep -q 'missing: bin/pocket-ui' $C/pocket-ui/logs/boot.log"
  check "missing binary: the 'missing' picture was asked for" "grep -q 'picture .*/msg/missing.png' $WORK/log"
  check "missing binary: nothing was started" "! grep -q trimui_inputd $WORK/log"

  # 2. Even the bridge is missing: the raw frame goes to the screen file, the pan is reset.
  make_card "$C"; rm -f "$C/pocket-ui/app/bin/pocket-ui" "$C/pocket-ui/app/bin/pocket-fb"; : >"$WORK/fbsys/pan"; head -c 3145728 /dev/zero >"$WORK/fb"
  rc=$(BRICK_FB_BYTES=3145728 run_updater "$SHBIN" "$C" 45)
  check "no bridge: ends with 1" "[ $rc = 1 ]"
  check "no bridge: the raw 'missing' frame is on the screen file" "cmp -s $WORK/fb $C/pocket-ui/app/msg/missing.raw"
  check "no bridge: the screen's page was reset to 0" "grep -q '^0,0' $WORK/fbsys/pan"

  # 3. A missing message picture falls back to a grey screen, still not black.
  make_card "$C"; rm -f "$C/pocket-ui/app/bin/pocket-core" "$C/pocket-ui/app/bin/pocket-fb" "$C/pocket-ui/app/msg/missing.raw" "$C/pocket-ui/app/msg/missing.png"
  rc=$(run_updater "$SHBIN" "$C" 45)
  check "no picture at all: grey, not black" "[ -s $WORK/fb ] && [ \"\$(tr -d '\\200' <$WORK/fb | wc -c)\" = 0 ]"

  # 4. pocket-ui that ends at once: three tries, then the picture; not forever.
  make_card "$C"; fake_pocket_ui "$C" 'echo "ui start" >>"$BRICK_TEST_LOG"; exit 3'
  rc=$(run_updater "$SHBIN" "$C" 40)
  check "crash loop: the updater ends by itself with 1" "[ $rc = 1 ]"
  check "crash loop: exactly three tries" "[ \"\$(grep -c '^ui start' $WORK/log)\" = 3 ]"
  check "crash loop: the 'crashed' picture was asked for" "grep -q 'picture .*/msg/crashed.png' $WORK/log"
  check "crash loop: each end is in the log with its code" "[ \"\$(grep -c 'pocket-ui ended with 3' $C/pocket-ui/logs/boot.log)\" = 3 ]"

  # 5. A healthy pocket-ui is started again when it ends (the 'Restart pocket-ui' path), with the environment it needs.
  make_card "$C"; fake_pocket_ui "$C" 'echo "ui start" >>"$BRICK_TEST_LOG"; env | grep -E "^(POCKETUI_[A-Z_]*|WAYLAND_DISPLAY|SWAYSOCK|POCKET_CORE_SOFTWARE|LD_LIBRARY_PATH|PATH|TZ)=" | sort >>"$BRICK_TEST_LOG"; sleep 2'
  # (with the older layout's program files beside app/, as a card copied over a 7f95a8fd card has them)
  mkdir -p "$C/pocket-ui/bin" "$C/pocket-ui/cores" "$C/pocket-ui/storage/.config/pocket-ui"; : >"$C/pocket-ui/bin/pocket-ui"; echo old >"$C/pocket-ui/VERSION"
  echo mine >"$C/pocket-ui/storage/.config/pocket-ui/settings.json"; echo key >"$C/pocket-ui/authorized_keys"
  rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 9)
  check "healthy: the older layout's program files beside app/ are gone, the player's files stay" "[ ! -e $C/pocket-ui/bin ] && [ ! -e $C/pocket-ui/VERSION ] && [ ! -e $C/pocket-ui/cores ] && [ -x $C/pocket-ui/app/bin/pocket-ui ] && grep -q mine $C/pocket-ui/storage/.config/pocket-ui/settings.json && grep -q key $C/pocket-ui/authorized_keys"
  check "healthy: still running when stopped from outside (124)" "[ $rc = 124 ]"
  check "healthy: started more than once" "[ \"\$(grep -c '^ui start' $WORK/log)\" -ge 2 ]"
  check "healthy: the pad's input daemon was started" "grep -q '^trimui_inputd' $WORK/log"
  check "healthy: the backlight follower was started on the stand-in panel" "grep -q 'brick-hw watch .*/hwroot/sys/class/backlight/panel 70' $WORK/log"
  check "healthy: the mute slider is followed" "grep -q 'brick-hw slider-watch' $WORK/log"
  check "healthy: device is brick" "grep -q '^POCKETUI_DEVICE=brick' $WORK/log"
  check "healthy: hardware seam root is the fake root" "grep -q '^POCKETUI_HW_ROOT=.*/hwroot' $WORK/log"
  check "healthy: core copies in software" "grep -q '^POCKET_CORE_SOFTWARE=1' $WORK/log"
  check "healthy: stand-ins are first on PATH" "grep '^PATH=' $WORK/log | head -1 | grep -q '/hwroot/usr/bin:'"
  check "healthy: the bridge's sockets are in the environment" "grep -q '^WAYLAND_DISPLAY=.*wayland-0' $WORK/log && grep -q '^SWAYSOCK=.*sway.sock' $WORK/log"
  check "healthy: the fake root has battery, radio and stand-ins" "[ -L $WORK/run/hwroot/sys/class/power_supply/battery ] && [ -x $WORK/run/hwroot/usr/bin/volume ] && [ -d $WORK/run/hwroot/run/systemd/system ]"
  check "healthy: library root, folders and cores' folder are made" "[ -d $C/Roms/gba ] && [ -d $C/Roms/snes ] && [ -d $C/pocket-ui/storage/.config/emulationstation ]"
  check "healthy: the clock's links are built (firmware zone through two links, no choice yet)" "[ -L $WORK/run/hwroot/usr/share/zoneinfo ] && [ \"\$(readlink $WORK/run/hwroot/etc/localtime)\" = ../usr/share/zoneinfo/Europe/London ] && grep -q '^TZ=.*/Europe/London' $WORK/log"

  # 5b. A zone chosen on the clock page (its name kept on the card) is the zone of the next start; a name the zone data
  # has no file for falls back to the firmware's zone instead of none.
  make_card "$C"; fake_pocket_ui "$C" 'echo "ui start" >>"$BRICK_TEST_LOG"; sleep 2'
  mkdir -p "$C/pocket-ui/app/zoneinfo/Europe" "$C/pocket-ui/storage/.config/pocket-ui"; : >"$C/pocket-ui/app/zoneinfo/Europe/Berlin"; : >"$C/pocket-ui/app/zoneinfo/UTC"
  echo "Europe/Berlin" >"$C/pocket-ui/storage/.config/pocket-ui/timezone"; touch "$C/pocket-ui/storage/.config/pocket-ui/ntp-synced"
  rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "chosen zone: the card's own data and zone win" "[ \"\$(readlink $WORK/run/hwroot/usr/share/zoneinfo)\" = $C/pocket-ui/app/zoneinfo ] && [ \"\$(readlink $WORK/run/hwroot/etc/localtime)\" = ../usr/share/zoneinfo/Europe/Berlin ]"
  check "chosen zone: last boot's sync note is gone until this boot syncs" "[ ! -e $C/pocket-ui/storage/.config/pocket-ui/ntp-synced ]"
  echo "Europe/Nowhere" >"$C/pocket-ui/storage/.config/pocket-ui/timezone"
  rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "bad zone name: falls back (here to UTC, the firmware's London is not in the card's data)" "[ \"\$(readlink $WORK/run/hwroot/etc/localtime)\" = ../usr/share/zoneinfo/UTC ] && grep -q 'not in .*zoneinfo' $C/pocket-ui/logs/boot.log"

  # The release startup used to recopy ~90 MB of identical engine data without a write limit.
  # It now calls the bounded helper before UI startup; only a successful helper publishes the release marker.
  make_card "$C"
  mkdir -p "$C/pocket-ui/app/bios-data" "$C/Roms/bios"
  echo engine >"$C/pocket-ui/app/bios-data/engine.dat"
  fake_pocket_ui "$C" 'if [ "${1:-}" = --brick-seed-data ]; then
    [ "$2" = "$BRICK_CARD/pocket-ui/app/bios-data" ] && [ "$3" = "$BRICK_CARD/Roms/bios" ] && [ "$4" = "$(cat "$BRICK_CARD/pocket-ui/app/RELEASE")" ] || exit 9
    printf "%s\n" "$4" >"$3/.pocket-ui-data"
    echo seed >>"$BRICK_TEST_LOG"; exit 0
  fi
  cmp -s "$BRICK_CARD/pocket-ui/app/RELEASE" "$BRICK_CARD/Roms/bios/.pocket-ui-data" || exit 9
  echo "ui start" >>"$BRICK_TEST_LOG"; sleep 30'
  rc=$(run_updater "$SHBIN" "$C" 5)
  check "engine data: bounded helper precedes UI and publishes marker after success" "[ $rc = 124 ] && [ \"\$(grep -c '^seed' $WORK/log)\" = 1 ] && grep -q '^ui start' $WORK/log && cmp -s $C/pocket-ui/app/RELEASE $C/Roms/bios/.pocket-ui-data && [ ! -e $C/Roms/bios/.pocket-ui-data.new ]"
  rc=$(run_updater "$SHBIN" "$C" 5)
  check "engine data: matching release marker avoids seeding on the next start" "[ $rc = 124 ] && ! grep -q '^seed' $WORK/log && grep -q '^ui start' $WORK/log"
  rm -f "$C/Roms/bios/.pocket-ui-data"
  mkdir -p "$C/pocket-ui/storage/pocket-ui"; cat "$C/pocket-ui/app/RELEASE" >"$C/pocket-ui/storage/pocket-ui/update-pending"
  fake_pocket_ui "$C" 'if [ "${1:-}" = --brick-seed-data ]; then echo seed-failed >>"$BRICK_TEST_LOG"; exit 8; fi
  echo "ui start" >>"$BRICK_TEST_LOG"; exit 0'
  # Enter the app starter directly: exit 76 is the durable boot script's rollback request.
  rc=$(ENTRY=pocket-ui/app/start.sh run_updater "$SHBIN" "$C" 5)
  check "engine data failure: requests rollback without starting UI or publishing success" "[ $rc = 76 ] && grep -q '^seed-failed' $WORK/log && ! grep -q '^ui start' $WORK/log && [ ! -e $C/Roms/bios/.pocket-ui-data ] && [ -s $C/pocket-ui/storage/pocket-ui/update-pending ]"

  # 6. The same through TrimUI's own firmware's entry (trimui/app/MainUI): same script, same result, the log says which entry.
  make_card "$C"; fake_pocket_ui "$C" 'echo "ui start" >>"$BRICK_TEST_LOG"; exit 3'
  rc=$(ENTRY=trimui/app/MainUI run_updater "$SHBIN" "$C" 40)
  check "stock entry: crash loop ends by itself with 1, three tries" "[ $rc = 1 ] && [ \"\$(grep -c '^ui start' $WORK/log)\" = 3 ]"
  check "stock entry: the log names the entry" "grep -q 'boot via stock-firmware' $C/pocket-ui/logs/boot.log"
  check "nextui entry: the log names the entry" "rc=\$(run_updater '$SHBIN' '$C' 40); grep -q 'boot via nextui-hook' $C/pocket-ui/logs/boot.log"
  check "stock entry: premainui.sh succeeds, preload.sh says 'no card UI ran' (1), so the firmware's own menu is the fallback" "sh $C/trimui/app/premainui.sh && ! sh $C/trimui/app/preload.sh"
  # 7. Wi-Fi: started unless pocket-ui's switch put it off (TrimUI's start script has stopped it when its own setting was off).
  make_card "$C"; fake_pocket_ui "$C" 'sleep 3'; mkdir -p "$C/pocket-ui/storage/.config/pocket-ui"
  printf '#!/bin/sh\necho "wpa $*" >>"$BRICK_TEST_LOG"\n' >"$WORK/wpa-init"; chmod +x "$WORK/wpa-init"
  rc=$(BRICK_WPA_INIT=$WORK/wpa-init BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 5)
  check "wifi on by default: the supplicant is started" "grep -q '^wpa start' $WORK/log"
  touch "$C/pocket-ui/storage/.config/pocket-ui/wifi-off"
  rc=$(BRICK_WPA_INIT=$WORK/wpa-init BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 5)
  check "wifi switched off by the player: not started" "! grep -q '^wpa start' $WORK/log"
  # 8. Saves and games stay on the card: the card's Saves/savestates is made and bound under /storage/roms
  check "saves folder is made on the card" "[ -d $C/Saves/savestates ]"
  # 9. The GPU: on by default on this card, with the CPU copy as the way back.
  make_card "$C"; fake_pocket_ui "$C" 'sleep 3'
  rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "gpu: probed, and the bridge is started with --gl by default" "grep -q '^brick-egl --probe' $WORK/log && grep -q '^pocket-fb --wayland .* --gl --gl-window null' $WORK/log && grep -q 'GL: the probe works' $C/pocket-ui/logs/boot.log"
  rc=$(BRICK_TEST_EGL_RC=7 BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "gpu: a failed probe means the CPU copy, and the log says why" "! grep -q -- '--gl' $WORK/log && grep -q 'GL: the probe failed (exit 7)' $C/pocket-ui/logs/boot.log"
  : >"$C/pocket-ui/gl-off"; rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "gpu: the gl-off file keeps it off without a probe" "! grep -q '^brick-egl' $WORK/log && ! grep -q -- '--gl' $WORK/log"
  rm "$C/pocket-ui/gl-off"; rm -f "$C/pocket-ui/app/bin/brick-egl"; rc=$(BRICK_FAST_END=1 run_updater "$SHBIN" "$C" 6)
  check "gpu: without brick-egl the card still starts, on the CPU copy" "! grep -q -- '--gl' $WORK/log && grep -q 'GL: no bin/brick-egl' $C/pocket-ui/logs/boot.log && grep -q '^pocket-fb --wayland' $WORK/log"
done

echo "== the card's zip (built from stand-in parts): folders are folders"
CZ=$WORK/cardzip; mkdir -p "$CZ/bins" "$CZ/libs" "$CZ/cores/cores" "$CZ/cores/licences" "$CZ/cores/sources"
for f in pocket-ui pocket-core pocket-fb brick-hw brick-egl; do echo x >"$CZ/bins/$f"; done
for f in libwayland-client.so.0 libwayland-cursor.so.0 libwayland-egl.so.1 libxkbcommon.so.0 libffi.so.8; do echo x >"$CZ/libs/$f"; done
mkdir -p "$CZ/cores/assets/bios/scummvm/extra" "$CZ/cores/assets/bios/scummvm/theme"
echo x >"$CZ/cores/assets/bios/scummvm/extra/sky.cpt"
echo x >"$CZ/cores/assets/bios/scummvm/theme/scummmodern.zip"
: >"$CZ/cores/CORES.txt"
for c in gambatte mgba nestopia snes9x pcsx_rearmed gpsp fceumm picodrive beetle_ngp beetle_wswan beetle_pce_fast handy stella prosystem fbneo mupen64plus_next ppsspp flycast gearsystem beetle_supergrafx pokemini beetle_vb fake08 tic80 melondsds dosbox_pure scummvm yabause; do
  echo x >"$CZ/cores/cores/${c}_libretro.so"; echo "$c abc123 ${c}_libretro.so 00" >>"$CZ/cores/CORES.txt"; tar czf "$CZ/cores/sources/$c-abc123.tar.gz" -C "$CZ" libs
done
# The non-commercial cores' own licence texts; FBNeo's top-level file is only a link, its text is src/license.txt in its source.
mkdir -p "$CZ/cores/licences/snes9x" "$CZ/cores/licences/picodrive" "$CZ/cores/licences/fbneo" "$CZ/fbsrc/src"
echo "Snes9x is freeware" >"$CZ/cores/licences/snes9x/LICENSE"; echo "may not be sold" >"$CZ/cores/licences/picodrive/COPYING"
echo "Please see the license here" >"$CZ/cores/licences/fbneo/LICENSE.md"; echo " - You may not sell, lease, rent" >"$CZ/fbsrc/src/license.txt"
mkdir -p "$CZ/cores/licences/picodrive/._pico_cd_libchdr"; echo bsd >"$CZ/cores/licences/picodrive/._pico_cd_libchdr/LICENSE.txt"   # an older core build's hidden name
tar czf "$CZ/cores/sources/fbneo-abc123.tar.gz" -C "$CZ/fbsrc" src
printf '#!/bin/sh\nmkdir -p "$1"; echo x >"$1/LICENSE"\n' >"$CZ/lic.sh"; chmod +x "$CZ/lic.sh"   # the real one needs cargo
# The network parts as scripts/brick-remote-parts.sh lays them out.
mkdir -p "$CZ/parts/bin" "$CZ/parts/licences/tailscale" "$CZ/parts/licences/curl" "$CZ/parts/licences/dropbear" "$CZ/parts/licences/syncthing" "$CZ/parts/sources"
for f in tailscale tailscaled curl dropbearmulti syncthing; do echo x >"$CZ/parts/bin/$f"; done
for d in tailscale curl dropbear syncthing; do echo licence >"$CZ/parts/licences/$d/LICENSE"; done
echo where >"$CZ/parts/sources/SOURCES.txt"; echo "tailscale 1 00" >"$CZ/parts/PARTS.txt"
if BRICK_PARTS=$CZ/parts BRICK_UPDATE_URL=https://example.test/brick-updates/beta/update.json BRICK_LICENCE_FILES=$CZ/lic.sh \
   "$ROOT/scripts/brick-make-card.sh" "$CZ/out" "$CZ/bins" "$CZ/libs" "$CZ/cores" test >"$CZ/make.log" 2>&1; then
  ok "brick-make-card builds a card from stand-in parts"
  ZIP=$CZ/out/pocket-ui-brick-card-test.zip
  mkdir "$CZ/unpacked"
  python3 -I - "$ZIP" "$CZ/unpacked" <<'PY' && ok "the zip lists its folders as directory entries" || bad "the zip stores a folder as a file"
import stat, sys, zipfile
z = zipfile.ZipFile(sys.argv[1]); names = {i.filename: i for i in z.infolist()}
for d in ["Roms/", "Saves/savestates/", "pocket-ui/logs/", ".tmp_update/"]:
    i = names.get(d)
    assert i is not None and stat.S_ISDIR(i.external_attr >> 16) and i.external_attr & 0x10, d
for f in [".tmp_update/updater", "pocket-ui/start.sh", "pocket-ui/app/start.sh", "pocket-ui/app/bin/pocket-ui", "trimui/app/MainUI"]:
    assert (names[f].external_attr >> 16) & 0o111, f + " is not executable in the zip"
z.extractall(sys.argv[2])
PY
  U=$CZ/unpacked
  check "ScummVM engine data and themes are in the BIOS folder" "[ -s $U/Roms/bios/scummvm/extra/sky.cpt ] && [ -s $U/Roms/bios/scummvm/theme/scummmodern.zip ]"
  for s in gb gbc gba nes snes psx megadrive mastersystem gamegear ngp ngpc wonderswan wonderswancolor pcengine atarilynx atari2600 atari7800 fbn neogeo cps1 cps2 cps3 n64 psp dreamcast fds sg-1000 segacd sega32x pcenginecd supergrafx pokemini virtualboy pico-8 tic-80 nds pc scummvm saturn bios; do
    check "unpacked: Roms/$s is a folder with a README.txt" "[ -d $U/Roms/$s ] && [ -s $U/Roms/$s/README.txt ]"
  done
  # every system pocket-ui is told about has its folder
  for s in $(grep -o '<name>[^<]*' "$ROOT/deploy/brick/es_systems.cfg" | cut -c7-); do check "unpacked: es_systems' $s has a Roms folder" "[ -d $U/Roms/$s ]"; done
  check "unpacked: Saves/savestates and pocket-ui/logs are folders" "[ -d $U/Saves/savestates ] && [ -d $U/pocket-ui/logs ]"
  check "unpacked: the program is pocket-ui/app, with its version, channel and the network parts" "[ \"\$(cat $U/pocket-ui/app/RELEASE)\" = test ] && grep -q '/brick-updates/beta/update.json' $U/pocket-ui/app/update-url && [ -s $U/pocket-ui/app/bin/tailscaled ] && [ -s $U/pocket-ui/app/bin/dropbearmulti ] && [ -s $U/pocket-ui/app/licences/tailscale/LICENSE ] && [ -s $U/pocket-ui/app/sources/network/SOURCES.txt ]"
  check "unpacked: no authorized_keys and no storage in the zip (a copy over a card keeps the player's)" "[ ! -e $U/pocket-ui/authorized_keys ] && [ ! -e $U/pocket-ui/storage ]"
  check "unpacked: pocket-ui/start.sh is boot.sh, the program's start.sh is start.sh" "cmp -s $U/pocket-ui/start.sh $ROOT/deploy/brick/boot.sh && cmp -s $U/pocket-ui/app/start.sh $ROOT/deploy/brick/start.sh"
  check "unpacked: FBNeo's licence is there word for word (from its source), next to snes9x's and PicoDrive's" "grep -q 'You may not sell' $U/pocket-ui/app/licences/cores/fbneo/license.txt && [ -s $U/pocket-ui/app/licences/cores/snes9x/LICENSE ] && [ -s $U/pocket-ui/app/licences/cores/picodrive/COPYING ] && grep -q SOURCES.txt $U/pocket-ui/app/licences/README.txt && [ -s $U/pocket-ui/app/licences/SOURCES.txt ]"
  check "unpacked: no hidden ._ licence folders (macOS drops them), the files under visible names" "[ -z \"\$(find $U/pocket-ui/app/licences -name '._*')\" ] && [ -s $U/pocket-ui/app/licences/cores/picodrive/pico_cd_libchdr/LICENSE.txt ]"
  check "unpacked (beta card): the cores' source is on the card" "[ -s $U/pocket-ui/sources/cores/fbneo-abc123.tar.gz ]"
  # The public card: no update-url (it asks update::BRICK_DEFAULT_URL), the source in its own zip next to it.
  echo src >"$CZ/pocket-ui-src.tar.gz"
  if BRICK_PARTS=$CZ/parts BRICK_LICENCE_FILES=$CZ/lic.sh BRICK_SOURCES_APART=1 BRICK_POCKETUI_SOURCE=$CZ/pocket-ui-src.tar.gz \
     "$ROOT/scripts/brick-make-card.sh" "$CZ/pub" "$CZ/bins" "$CZ/libs" "$CZ/cores" 0.2.1 >"$CZ/pub.log" 2>&1; then
    check "public card: no update-url, no cores' source in the card zip, both in the sources zip with pocket-ui's" "! unzip -l $CZ/pub/pocket-ui-brick-card-0.2.1.zip | grep -q -e update-url -e 'sources/cores/' && unzip -l $CZ/pub/pocket-ui-brick-sources-0.2.1.zip | grep -q 'sources/cores/fbneo-abc123.tar.gz' && unzip -l $CZ/pub/pocket-ui-brick-sources-0.2.1.zip | grep -q 'sources/pocket-ui-src.tar.gz' && unzip -l $CZ/pub/pocket-ui-brick-sources-0.2.1.zip | grep -q 'sources/network/SOURCES.txt' && unzip -p $CZ/pub/pocket-ui-brick-card-0.2.1.zip pocket-ui/app/licences/SOURCES.txt | grep -q 'pocket-ui-brick-sources-0.2.1.zip'"
  else
    bad "brick-make-card failed for the public card: $(tail -3 "$CZ/pub.log" | tr '\n' ' ')"
  fi
  printf 'x\000/home/builder/target/out/egl_bindings.rs\000' >"$CZ/bins/pocket-core"
  check "a program with a build machine path in it is refused" "! BRICK_PARTS=$CZ/parts BRICK_LICENCE_FILES=$CZ/lic.sh $ROOT/scripts/brick-make-card.sh $CZ/paths $CZ/bins $CZ/libs $CZ/cores 0.2.1 >$CZ/paths.log 2>&1 && grep -q 'build path' $CZ/paths.log"
  echo x >"$CZ/bins/pocket-core"
  rm "$CZ/cores/licences/snes9x/LICENSE"
  check "a card without snes9x's licence text is refused" "! BRICK_PARTS=$CZ/parts BRICK_LICENCE_FILES=$CZ/lic.sh $ROOT/scripts/brick-make-card.sh $CZ/nolic $CZ/bins $CZ/libs $CZ/cores 0.2.1 >$CZ/nolic.log 2>&1 && grep -q 'snes9x/LICENSE is missing' $CZ/nolic.log"
  echo "Snes9x is freeware" >"$CZ/cores/licences/snes9x/LICENSE"
else
  bad "brick-make-card failed with stand-in parts: $(tail -3 "$CZ/make.log" | tr '\n' ' ')"
fi

echo "== updates: the program folder is switched, a failed version is taken back (boot.sh, under dash and BusyBox ash)"
# A card at version 1 whose pocket-ui "installs" version 2 the way update::install_brick does (app.new with .ready, update-pending,
# two renames) and exits 75. Version 2's pocket-ui is given per case.
stage_v2() { # card, pocket-ui body of version 2
  c=$1; rm -rf "$c/pocket-ui/app.new"; cp -R "$c/pocket-ui/app" "$c/pocket-ui/app.new"; echo 2 >"$c/pocket-ui/app.new/RELEASE"
  fake_pocket_ui "$c" "$2" app.new; : >"$c/pocket-ui/app.new/.ready"
}
# Version 1 installs 2 when there is one to install; once it is back after a failed 2 it just runs ("v1 back").
INSTALL_V2='d=$BRICK_DATA; if [ -d $d/app.new ]; then echo "v1 runs" >>"$BRICK_TEST_LOG"; mkdir -p $d/storage/pocket-ui; echo 2 >$d/storage/pocket-ui/update-pending; mv $d/app $d/app.previous; mv $d/app.new $d/app; exit 75; fi; echo "v1 back" >>"$BRICK_TEST_LOG"; sleep 30'
CONFIRM_V2='echo "v2 runs" >>"$BRICK_TEST_LOG"; rm -f $BRICK_DATA/storage/pocket-ui/update-pending; sleep 30'
for SH in dash "busybox ash"; do
  echo "-- $SH"
  C=$WORK/upd; make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" "$CONFIRM_V2"; fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 8)
  check "installed: v1 exits 75, the new folder starts and confirms itself" "grep -q '^v1 runs' $WORK/log && grep -q '^v2 runs' $WORK/log && [ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 2 ] && [ ! -e $C/pocket-ui/storage/pocket-ui/update-pending ]"
  check "installed: the previous version is kept" "[ \"\$(cat $C/pocket-ui/app.previous/RELEASE)\" = 1 ]"
  check "installed: the log says it restarted into 2" "grep -q 'restart into 2 after an update' $C/pocket-ui/logs/boot.log"

  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" 'echo "v2 crashed" >>"$BRICK_TEST_LOG"; exit 139'; fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 12)
  # Twice: with the GPU on (the default), the first quick end is the GPU's way back (CPU copy); the second is the version's.
  check "crashing update: 2 crashed (GPU, then CPU copy), then 1 runs again by itself" "[ \"\$(grep -c '^v2 crashed' $WORK/log)\" = 2 ] && grep -q '^v1 back' $WORK/log && [ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ]"
  check "crashing update: rolled-back names 2 (pocket-ui shows it once), update-pending is gone, 2 is kept aside" "[ \"\$(cat $C/pocket-ui/storage/pocket-ui/rolled-back)\" = 2 ] && [ ! -e $C/pocket-ui/storage/pocket-ui/update-pending ] && [ \"\$(cat $C/pocket-ui/app.failed/RELEASE)\" = 2 ]"
  check "crashing update: no failure picture, no ten-minute wait" "! grep -q 'picture .*/msg/crashed.png' $WORK/log && grep -q 'back to 1' $C/pocket-ui/logs/boot.log"

  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" "$CONFIRM_V2"; rm -f "$C/pocket-ui/app.new/bin/pocket-fb" "$C/pocket-ui/app.new/msg/missing.png"; fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 10)
  check "update missing a part: back to 1 at once, not the 'missing' picture" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ] && grep -q 'just installed: back to the previous version' $C/pocket-ui/logs/boot.log && grep -q '^v1 back' $WORK/log"

  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" "$CONFIRM_V2"; printf 'if then fi (\n' >"$C/pocket-ui/app.new/start.sh"; fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 10)
  check "update whose start script is broken: back to 1" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ] && [ \"\$(cat $C/pocket-ui/storage/pocket-ui/rolled-back)\" = 2 ] && grep -q '^v1 back' $WORK/log"

  # A power cut between the two renames: app/ is gone, app.previous (1) and a complete app.new (2) are there.
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"; stage_v2 "$C" "$CONFIRM_V2"
  mv "$C/pocket-ui/app" "$C/pocket-ui/app.previous"; echo 2 >"$C/pocket-ui/storage/pocket-ui/update-pending"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 6)
  check "power cut mid-switch: the next start finishes it and 2 confirms itself" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 2 ] && grep -q '^v2 runs' $WORK/log && grep -q 'power cut interrupted' $C/pocket-ui/logs/boot.log"
  # ... and one before app.new was complete: the previous version comes back.
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"; stage_v2 "$C" "$CONFIRM_V2"; rm -f "$C/pocket-ui/app.new/.ready"
  fake_pocket_ui "$C" 'echo "v1 back" >>"$BRICK_TEST_LOG"; sleep 30'
  mv "$C/pocket-ui/app" "$C/pocket-ui/app.previous"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 6)
  check "power cut before the new folder was ready: 1 comes back" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ] && grep -q '^v1 back' $WORK/log"
  # A leftover update-pending that names another version is not a reason to go back (1 has run long on this card before).
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui" "$C/pocket-ui/app.previous"; echo 0 >"$C/pocket-ui/app.previous/RELEASE"; : >"$C/pocket-ui/app.previous/start.sh"
  echo 1 >"$C/pocket-ui/storage/pocket-ui/known-good"
  echo 9 >"$C/pocket-ui/storage/pocket-ui/update-pending"; fake_pocket_ui "$C" 'echo "ui start" >>"$BRICK_TEST_LOG"; exit 3'
  rc=$(run_updater "$SH" "$C" 40)
  check "stale update-pending: dropped, no way back, the usual three tries" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ] && [ \"\$(grep -c '^ui start' $WORK/log)\" = 3 ] && grep -q 'update-pending names 9' $C/pocket-ui/logs/boot.log"

  # An update that shows its first picture (it confirms itself) and then crashes every time, seconds later: it never ran long on
  # this card, so 1 comes back by itself, as for one that crashed before its picture. Not the failure picture, not a card to repair.
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" 'echo "v2 runs" >>"$BRICK_TEST_LOG"; rm -f $BRICK_DATA/storage/pocket-ui/update-pending; exit 139'; fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=1 run_updater "$SH" "$C" 20)
  check "update crashing after its picture: 1 comes back by itself" "[ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 1 ] && grep -q '^v1 back' $WORK/log && [ \"\$(cat $C/pocket-ui/storage/pocket-ui/rolled-back)\" = 2 ]"
  check "update crashing after its picture: three quick ends (the first one is the GPU's way back), no failure picture" "[ \"\$(grep -c '^v2 runs' $WORK/log)\" = 3 ] && ! grep -q 'picture .*/msg/crashed.png' $WORK/log && grep -q 'never ran 120 s on this card' $C/pocket-ui/logs/boot.log"
  check "update crashing after its picture: 2 is kept aside, update-pending is gone" "[ \"\$(cat $C/pocket-ui/app.failed/RELEASE)\" = 2 ] && [ ! -e $C/pocket-ui/storage/pocket-ui/update-pending ]"

  # A version that ran long on this card keeps its place when it crashes later: the failure picture as before, no going back.
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" 'n=$(($(cat $BRICK_DATA/runs 2>/dev/null || echo 0) + 1)); echo $n >$BRICK_DATA/runs; echo "v2 run $n" >>"$BRICK_TEST_LOG"; rm -f $BRICK_DATA/storage/pocket-ui/update-pending; [ $n = 1 ] && sleep 3; exit 139'
  fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_STABLE_SECS=1 BRICK_FAST_END=1 run_updater "$SH" "$C" 20)
  check "proven version crashing later: 2 is the known good one and stays" "[ \"\$(cat $C/pocket-ui/storage/pocket-ui/known-good)\" = 2 ] && [ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 2 ] && [ ! -e $C/pocket-ui/storage/pocket-ui/rolled-back ]"
  check "proven version crashing later: the failure picture, as before" "grep -q 'picture .*/msg/crashed.png' $WORK/log && [ $rc = 1 ]"

  # A restart pocket-ui asks for (exit 0) is no crash: a new version restarted three times early stays.
  make_card "$C"; mkdir -p "$C/pocket-ui/storage/pocket-ui"
  stage_v2 "$C" 'n=$(($(cat $BRICK_DATA/runs 2>/dev/null || echo 0) + 1)); echo $n >$BRICK_DATA/runs; echo "v2 run $n" >>"$BRICK_TEST_LOG"; rm -f $BRICK_DATA/storage/pocket-ui/update-pending; [ $n -le 3 ] && exit 0; sleep 30'
  fake_pocket_ui "$C" "$INSTALL_V2"
  rc=$(BRICK_FAST_END=0 run_updater "$SH" "$C" 8)
  check "restarts asked for are no crashes: 2 stays" "grep -q '^v2 run 4' $WORK/log && [ \"\$(cat $C/pocket-ui/app/RELEASE)\" = 2 ] && [ ! -e $C/pocket-ui/storage/pocket-ui/rolled-back ]"
done

echo "== publishing to the private channel (scripts/brick-publish.sh, a throwaway key, file:// instead of the tailnet)"
ST=${STORE_TOOL:-}
for c in "$ROOT/target/debug/store-tool" "$ROOT/target/gate/store-tool" "${CARGO_TARGET_DIR:-$ROOT/target}/gate/store-tool" "${CARGO_TARGET_DIR:-$ROOT/target}/debug/store-tool"; do
  [ -z "$ST" ] && [ -x "$c" ] && ST=$c
done
if [ -n "$ST" ]; then
  PB=$WORK/publish; mkdir -p "$PB/app/bin" "$PB/app/msg" "$PB/app/hwroot/usr/bin" "$PB/web"
  "$ST" keygen "$PB/key" >/dev/null
  fake_arm() { # file version: an aarch64 ELF header with the version string inside, as zigbuild leaves POCKETUI_VERSION in the binary
    printf '\177ELF\002\001\001\000\000\000\000\000\000\000\000\000\002\000\267\000' >"$1"; head -c 3000 /dev/urandom >>"$1"; printf 'x%sx' "$2" >>"$1"
  }
  build() { # version
    for b in pocket-ui pocket-core pocket-fb; do fake_arm "$PB/app/bin/$b" "$1"; done
    for b in brick-hw tailscale tailscaled curl dropbearmulti syncthing; do echo x >"$PB/app/bin/$b"; done
    cp "$ROOT/deploy/brick/start.sh" "$PB/app/start.sh"; echo png >"$PB/app/msg/starting.png"
    cp "$ROOT"/deploy/brick/hwroot/usr/bin/* "$PB/app/hwroot/usr/bin/"; echo "$1" >"$PB/app/RELEASE"
  }
  pub() { STORE_TOOL=$ST POCKETUI_SIGNING_KEY=$PB/key BRICK_PUBLISH_VERIFY_KEY=$PB/key BRICK_CHANNEL_BASE=file://$PB/web "$ROOT/scripts/brick-publish.sh" "$PB/app" --web "$PB/web" --channel test "$@"; }
  CH=$PB/web/brick-updates/test
  build 0.1.12-brick.1
  check "a build is published: tarball, signed list, read back" "pub --notes 'first' --full-check >$PB/out 2>&1 && [ -s $CH/update.json ] && [ -s $CH/update.json.sig ] && [ -s $CH/pocket-ui-brick-0.1.12-brick.1.tar.gz ] && \"$ST\" verify $CH/update.json $PB/key >/dev/null"
  check "the list names the version, the tarball's size and sha256, relative to the list" "\"$ST\" list $CH/update.json | awk -F'\t' '\$1==\"app\"' | grep -q \"0.1.12-brick.1	\$(wc -c <$CH/pocket-ui-brick-0.1.12-brick.1.tar.gz)	\$(sha256sum $CH/pocket-ui-brick-0.1.12-brick.1.tar.gz | cut -d' ' -f1)	.*/brick-updates/test/pocket-ui-brick-0.1.12-brick.1.tar.gz\""
  check "the tarball is the program folder: start.sh, RELEASE and bin/ at its top" "tar -tzf $CH/pocket-ui-brick-0.1.12-brick.1.tar.gz | grep -qx './RELEASE' && tar -tzf $CH/pocket-ui-brick-0.1.12-brick.1.tar.gz | grep -qx './bin/pocket-ui'"
  cp "$CH/pocket-ui-brick-0.1.12-brick.1.tar.gz" "$PB/first.tar.gz"; rm -f "$CH/pocket-ui-brick-0.1.12-brick.1.tar.gz"; rm -f "$CH/update.json" "$CH/update.json.sig"
  pub >/dev/null 2>&1
  check "the same folder packs to the same bytes" "cmp -s $PB/first.tar.gz $CH/pocket-ui-brick-0.1.12-brick.1.tar.gz"
  check "the key is never printed" "! grep -qi \"\$(od -An -tx1 $PB/key | tr -d ' \\n' | cut -c1-16)\" $PB/out"
  check "the same version again is refused (a Brick would not take it)" "! pub >/dev/null 2>&1"
  fake_arm "$PB/app/bin/pocket-ui" 0.1.12-brick.9; echo 0.1.12-brick.2 >"$PB/app/RELEASE"
  check "programs built as another version are refused" "! pub >$PB/err 2>&1 && grep -q 'was not built as 0.1.12-brick.2' $PB/err"
  build 0.1.12-brick.2; rm "$PB/app/bin/curl"
  check "a folder without a part the card needs is refused" "! pub >$PB/err 2>&1 && grep -q 'no bin/curl' $PB/err"
  build 0.1.12-brick.2; echo 'if then' >"$PB/app/start.sh"
  check "a broken start script is refused" "! pub >/dev/null 2>&1"
  build 0.1.12-brick.2; printf 'not an elf' >"$PB/app/bin/pocket-fb"
  check "a program that is not aarch64 is refused" "! pub >$PB/err 2>&1 && grep -q 'not an aarch64' $PB/err"
  for v in 2 3 4 5; do build 0.1.12-brick.$v; pub >/dev/null 2>&1; done
  check "the list moves on, the last three tarballs stay" "\"$ST\" list $CH/update.json | grep -q '0.1.12-brick.5' && [ \"\$(ls $CH/*.tar.gz | wc -l)\" = 3 ]"
  # A mirror near the Brick (BRICK_MIRROR in scripts/brick-publish.sh): the tarball goes there too and the list names it there; the list stays.
  build 0.1.12-brick.6
  check "with a mirror the list names the mirror's tarball, which is the same file" "BRICK_MIRROR=$PB/mirror BRICK_MIRROR_BASE=file://$PB/mirror pub --full-check >$PB/out 2>&1 && \"$ST\" list $CH/update.json | awk -F'\\t' '\$1==\"app\"' | grep -q 'file://$PB/mirror/test/pocket-ui-brick-0.1.12-brick.6.tar.gz' && cmp -s $PB/mirror/test/pocket-ui-brick-0.1.12-brick.6.tar.gz $CH/pocket-ui-brick-0.1.12-brick.6.tar.gz && grep -q 'from the mirror' $PB/out"
  build 0.1.12-brick.7
  check "a mirror that does not give the tarball back: the list names this machine's copy" "BRICK_MIRROR=$PB/mirror BRICK_MIRROR_BASE=file://$PB/nowhere pub >$PB/out 2>&1 && \"$ST\" list $CH/update.json | awk -F'\\t' '\$1==\"app\"' | grep -q '/brick-updates/test/pocket-ui-brick-0.1.12-brick.7.tar.gz' && ! \"$ST\" list $CH/update.json | grep -q nowhere && grep -q 'did not take or give back' $PB/out"
  check "a wrong signing key is refused before the list changes" "\"$ST\" keygen $PB/other >/dev/null; build 0.1.12-brick.8; ! STORE_TOOL=$ST POCKETUI_SIGNING_KEY=$PB/other BRICK_PUBLISH_VERIFY_KEY=$PB/key BRICK_CHANNEL_BASE=file://$PB/web $ROOT/scripts/brick-publish.sh $PB/app --web $PB/web --channel test >/dev/null 2>&1 && \"$ST\" list $CH/update.json | grep -q '0.1.12-brick.7'"
  # The public channel (docs/UPDATE-CHANNEL.md, "The Brick card"): files for the GitHub release and the website's list, nothing uploaded.
  PO=$PB/public; ppub() { STORE_TOOL=$ST POCKETUI_SIGNING_KEY=$PB/key BRICK_PUBLISH_VERIFY_KEY=$PB/key "$ROOT/scripts/brick-publish.sh" "$PB/app" --public owner/pocket-ui --out "$PO" "$@"; }
  build 0.2.1; echo zip >"$PB/card.zip"
  check "public: an old updater cannot publish a small update" "! ppub >$PB/err 2>&1 && grep -q 'rebuild the public card' $PB/err"
  printf 'asset-packages.json' >>"$PB/app/bin/pocket-ui"
  mkdir -p "$PB/app/cores"; printf core >"$PB/app/cores/test_libretro.so"
  check "public: the list names the GitHub release's tarball and the card zip, signed, and the tarball is the program folder" "ppub --zip $PB/card.zip --notes 'first public' >$PB/out 2>&1 && \"$ST\" verify $PO/updates/brick/update.json $PB/key >/dev/null && \"$ST\" list $PO/updates/brick/update.json | awk -F'\\t' '\$1==\"app\"' | grep -q \"0.2.1	\$(wc -c <$PO/release/pocket-ui-brick-0.2.1.tar.gz)	\$(sha256sum $PO/release/pocket-ui-brick-0.2.1.tar.gz | cut -d' ' -f1)	https://github.com/owner/pocket-ui/releases/download/v0.2.1/pocket-ui-brick-0.2.1.tar.gz\" && \"$ST\" list $PO/updates/brick/update.json | grep -q 'install_zip.*releases/download/v0.2.1/card.zip' && tar -tzf $PO/release/pocket-ui-brick-0.2.1.tar.gz | grep -qx 'RELEASE' && [ -s $PO/release/card.zip ] && grep -q 'nothing uploaded' $PB/out"
  check "public: website first, GitHub fallback, and core packages kept outside the regular app" "\"$ST\" list $PO/updates/brick/update.json | grep -q 'https://pocketui.xyz/updates/brick/pocket-ui-brick-0.2.1.tar.gz' && tar -xOf $PO/release/pocket-ui-brick-0.2.1.tar.gz asset-packages.json | grep -q 'cores/test_libretro.so' && ! tar -tzf $PO/release/pocket-ui-brick-0.2.1.tar.gz | grep -q '^cores/' && [ -n \"\$(ls $PO/updates/brick/assets/*.tar.gz)\" ]"
  check "public: nothing goes into the private channel" "! \"$ST\" list $CH/update.json | grep -q '0.2.1'"
  echo https://beta.example/brick-updates/beta/update.json >"$PB/app/update-url"
  check "public: a build that asks a private channel (app/update-url) is refused" "! ppub >$PB/err 2>&1 && grep -q 'must have none' $PB/err"
  rm -f "$PB/app/update-url"; build 0.2.1-brick.8
  check "public: a beta version is refused" "! ppub >$PB/err 2>&1 && grep -q 'plain X.Y.Z' $PB/err"
  build 0.2.1
  printf 'asset-packages.json' >>"$PB/app/bin/pocket-ui"
  check "public: a list signed with a key pocket-ui does not trust is refused" "! STORE_TOOL=$ST POCKETUI_SIGNING_KEY=$PB/other BRICK_PUBLISH_VERIFY_KEY=$PB/key $ROOT/scripts/brick-publish.sh $PB/app --public owner/pocket-ui --out $PB/public2 >/dev/null 2>&1"
else
  echo "skip publishing (no store-tool built; STORE_TOOL=...)"
fi

echo "== stand-ins"
HW=$ROOT/deploy/brick/hwroot/usr/bin
S=$WORK/standin; mkdir -p "$S"
echo 28 >"$S/mixer"
cat >"$S/amixer" <<X
#!/bin/sh
case " \$* " in *" -s "*) while IFS= read -r line; do eval '"\$0" -c 0 -q '"\$line" || exit 1; done; exit ;; esac
case "\$*" in
  *cget*"BT Volume"*) exit 1 ;;
  *cget*) echo "  : values=\$(cat $S/mixer)" ;;
  *cset*"digital volume"*) for a; do v=\$a; done; echo "\$v" >$S/mixer ;;
  *cset*"DAC volume"*) for a; do v=\$a; done; echo "\$v" >$S/dac ;;
  *cset*) echo "\$*" >>$S/switches ;;
esac
X
chmod +x "$S/amixer"
export BRICK_AMIXER=$S/amixer BRICK_STATE=$S/state BRICK_SPEAKER=$S/mute BRICK_VOLUME_RAMP_DELAY=0
mkdir -p "$S/state"; echo 0 >"$S/mute"
printf '#!/bin/sh\necho "wpa $1" >>%s/w\n' "$S" >"$S/wi"; chmod +x "$S/wi"; mkdir -p "$S/rf"
check "wifictl enable/disable keep the player's choice and drive the supplicant" "BRICK_KILLALL=true BRICK_PIDOF=$WORK/no-pidof BRICK_WPA_INIT=$S/wi BRICK_IFCONFIG=true BRICK_UDHCPC=true BRICK_RFKILL=$S/rf $HW/wifictl disable && [ -e $S/state/wifi-off ] && grep -q 'wpa stop' $S/w && BRICK_PIDOF=$WORK/no-pidof BRICK_WPA_INIT=$S/wi BRICK_IFCONFIG=true BRICK_UDHCPC=true BRICK_RFKILL=$S/rf $HW/wifictl enable && [ ! -e $S/state/wifi-off ] && grep -q 'wpa start' $S/w"
# The chip's "digital volume" is an attenuation (63 = quietest; on the real Brick louder keys made it quieter), "DAC volume" 160 = normal.
chip() { cat "$S/mixer"; }
check "louder is a lower chip value: 20 % > 50 % > 100 % in attenuation order (up gets louder)" "$HW/volume 20 && a=\$(chip) && $HW/volume 50 && b=\$(chip) && $HW/volume 100 && c=\$(chip) && [ \$a -gt \$b ] && [ \$b -gt \$c ]"
check "volume 50 sets 20 of 63, 100 is 0 (loudest), 200 is clamped to 100" "$HW/volume 50 && [ \"\$(chip)\" = 20 ] && $HW/volume 100 && [ \"\$(chip)\" = 0 ] && $HW/volume 200 && [ \"\$(chip)\" = 0 ]"
check "every step up never raises the chip value (monotonic, 0..100)" "p=63; for v in \$(seq 0 100); do $HW/volume \$v; c=\$(chip); [ \$c -le \$p ] || exit 1; p=\$c; done"
check "max is loud: 100 % leaves DAC volume at 160" "$HW/volume 100 && [ \"\$(cat $S/dac)\" = 160,160 ]"
check "0 is silent: chip at 63 and DAC volume 0,0; raising from zero restores DAC" "$HW/volume 0 && [ \"\$(chip)\" = 63 ] && [ \"\$(cat $S/dac)\" = 0,0 ] && $HW/volume 1 && [ \"\$(cat $S/dac)\" = 160,160 ]"
check "volume refuses a non-number" "! $HW/volume abc 2>/dev/null"
check "pactl gives the remembered percentage" "$HW/volume 35 && $HW/pactl get-sink-volume @DEFAULT_SINK@ | grep -q ' 35% '"
# The low steps were lifted (docs/TRIMUI-BRICK-HW.md, curve 3): chip 28 is 19 % now, 30 % sits at chip 25. Nothing remembered, neither level.
check "pactl gives a percentage from the chip when nothing is remembered (chip 28 = 19 %)" "rm -f $S/state/brick-volume $S/state/brick-volume-curve2 $S/state/brick-volume-curve3; echo 28 >$S/mixer; $HW/pactl get-sink-volume @DEFAULT_SINK@ | grep -q ' 19% '"
check "pactl and volume agree in both directions (every percent comes back)" "for v in 0 10 25 50 75 90 100; do $HW/volume \$v; rm $S/state/brick-volume; r=\$($HW/pactl get-sink-volume @DEFAULT_SINK@ | sed 's/.*\/ *\([0-9]*\)%.*/\1/'); [ \$((r - v)) -le 1 ] && [ \$((v - r)) -le 1 ] || exit 1; done"
check "mute and unmute use the speaker and jack switches and leave the level" "$HW/volume 60; l=\$(chip); : >$S/switches; $HW/volume mute && grep -q 'HpSpeaker Switch off' $S/switches && grep -q 'Headphone Switch off' $S/switches && $HW/volume unmute && tail -n 2 $S/switches | grep -q 'on' && [ \"\$(chip)\" = \$l ]"
check "volume restore puts the remembered level and the outputs on" "$HW/volume 80; : >$S/switches; $HW/volume restore; [ \"\$(chip)\" = 8 ] && grep -q 'HpSpeaker Switch on' $S/switches"
printf '#!/bin/sh\necho "wlan0     Link encap:Ethernet\n          inet addr:192.0.2.41  Bcast:192.0.2.255  Mask:255.255.255.0"\n' >"$S/ifconfig"; chmod +x "$S/ifconfig"
check "ip stand-in: with only ifconfig, 'ip -4 -o addr show wlan0' still gives the address (it decides whether covers are fetched)" "BRICK_IP_DIRS= BRICK_IFCONFIG_BIN=$S/ifconfig $HW/ip -4 -o addr show wlan0 | grep -q 'inet 192.0.2.41/'"
check "ip stand-in: a wlan0 without an address gives no line" "printf '#!/bin/sh\\necho wlan0 Link encap\\n' >$S/ifconfig2; chmod +x $S/ifconfig2; [ -z \"\$(BRICK_IP_DIRS= BRICK_IFCONFIG_BIN=$S/ifconfig2 $HW/ip -4 -o addr show wlan0)\" ]"
check "brightness set is remembered" "$HW/brightness set 45 && [ \"\$(cat $S/state/brick-brightness)\" = 45 ]"
check "systemctl is-active says inactive" "[ \"\$($HW/systemctl is-active bluetooth)\" = inactive ]"
check "systemctl with an unknown verb fails loudly" "! $HW/systemctl restart smbd 2>/dev/null"
check "network time: no ntpd says NTP=no, and a sync try fails without a note" "rm -f $S/state/ntp-synced; BRICK_NTPD=$S/no-ntpd $HW/timedatectl show -p NTP -p NTPSynchronized | grep -q '^NTP=no' && ! BRICK_NTPD=$S/no-ntpd $HW/systemctl restart systemd-timesyncd 2>/dev/null && [ ! -e $S/state/ntp-synced ]"
printf '#!/bin/sh\nexit 0\n' >"$S/ntpd-ok"; chmod +x "$S/ntpd-ok"
check "network time: offered, unsynced, then synced after the one-shot" "[ \"\$(BRICK_NTPD=$S/ntpd-ok $HW/timedatectl show -p NTP -p NTPSynchronized | tr '\n' ' ')\" = 'NTP=yes NTPSynchronized=no ' ] && BRICK_NTPD=$S/ntpd-ok $HW/systemctl restart systemd-timesyncd && [ \"\$(BRICK_NTPD=$S/ntpd-ok $HW/timedatectl show -p NTP -p NTPSynchronized | tr '\n' ' ')\" = 'NTP=yes NTPSynchronized=yes ' ]"
check "timedatectl with another question fails loudly" "! $HW/timedatectl set-timezone Europe/Berlin 2>/dev/null"

echo "== Remote access: brick-ssh with the card's dropbear (a host build of the same source and patch), as $USER"
DB=${BRICK_TEST_DROPBEAR:-}
if [ -z "$DB" ] && command -v zig >/dev/null 2>&1; then
  BRICK_JOBS=2 nice -n 19 "$ROOT/scripts/brick-remote-parts.sh" --dropbear x86_64-linux-musl "$WORK/db" >"$WORK/db.log" 2>&1 && DB=$WORK/db/bin/dropbearmulti
fi
if [ -n "$DB" ] && [ -x "$DB" ] && command -v sshpass >/dev/null 2>&1 && command -v ssh >/dev/null 2>&1; then
  R=$WORK/remote; mkdir -p "$R/app/bin" "$R/data/storage/.config/pocket-ui/remote" "$R/run"
  cp "$DB" "$R/app/bin/dropbearmulti"
  PORT=$((20000 + $$ % 20000))
  export BRICK_APP=$R/app BRICK_DATA=$R/data BRICK_RUN=$R/run BRICK_SSH_PORT=$PORT BRICK_STATE=$R/data/storage/.config/pocket-ui
  SSHO="-o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR -o ConnectTimeout=5 -p $PORT"
  pwlogin() { sshpass -p "$1" ssh $SSHO -o PubkeyAuthentication=no -o PreferredAuthentications=password "$USER@127.0.0.1" "${2:-true}"; }
  check "no password made yet: dropbear starts for keys only and refuses every password" "$HW/brick-ssh start && ! pwlogin 'otter-cedar-lamp-315' 2>/dev/null"
  $HW/brick-ssh stop
  echo 'otter-cedar-lamp-315' >"$R/data/storage/.config/pocket-ui/remote/password"
  check "systemctl start sshd starts it, is-active says so" "$HW/systemctl start sshd && $HW/systemctl is-active --quiet sshd"
  check "the device's password logs in" "pwlogin 'otter-cedar-lamp-315'"
  check "another password does not" "! pwlogin 'otter-cedar-lamp-316' 2>/dev/null && ! pwlogin '' 2>/dev/null"
  check "the session's home is the RAM folder, PATH starts with the card's tools" "[ \"\$(pwlogin otter-cedar-lamp-315 'echo \$HOME')\" = $R/run/ssh/home ] && pwlogin otter-cedar-lamp-315 'echo \$PATH' | grep -q '^/usr/bin:/bin\|^/tmp/pocket-ui/bin:'"
  check "the host key is kept on the card" "[ -s $R/data/storage/.config/pocket-ui/remote/dropbear_ed25519_host_key ]"
  key1=$(cksum <"$R/data/storage/.config/pocket-ui/remote/dropbear_ed25519_host_key")
  ssh-keygen -q -t ed25519 -N '' -f "$R/k" && cp "$R/k.pub" "$R/data/authorized_keys"
  check "a key dropped into pocket-ui/authorized_keys works after the next switch-on, the same host key" "$HW/systemctl stop sshd && rm -rf $R/run/ssh && $HW/systemctl start sshd && ssh $SSHO -o BatchMode=yes -o IdentitiesOnly=yes -i $R/k $USER@127.0.0.1 true && [ \"\$(cksum <$R/data/storage/.config/pocket-ui/remote/dropbear_ed25519_host_key)\" = \"$key1\" ]"
  ssh-keygen -q -t ed25519 -N '' -f "$R/k2" && cat "$R/k2.pub" >>"$R/data/authorized_keys"
  check "a key added while Remote access is on works within seconds, without a switch-off" "sleep 5 && ssh $SSHO -o BatchMode=yes -o IdentitiesOnly=yes -i $R/k2 $USER@127.0.0.1 true"
  sshpass -p 'otter-cedar-lamp-315' ssh $SSHO -o PubkeyAuthentication=no "$USER@127.0.0.1" 'sleep 60' >/dev/null 2>&1 &
  SESSION=$!; sleep 2
  check "switching off stops the listener and ends an open session" "$HW/systemctl stop sshd && ! $HW/systemctl is-active --quiet sshd && sleep 1 && ! kill -0 $SESSION 2>/dev/null && ! pwlogin otter-cedar-lamp-315 2>/dev/null && [ ! -e $R/run/ssh/keys.pid ]"
  kill $SESSION 2>/dev/null; wait $SESSION 2>/dev/null
  unset BRICK_APP BRICK_DATA BRICK_RUN BRICK_SSH_PORT; export BRICK_STATE=$S/state
else
  echo "skip Remote access (needs zig or BRICK_TEST_DROPBEAR, sshpass, ssh)"
fi

echo "== Tailscale: brick-tailscale with a stand-in tailscaled"
T=$WORK/ts; mkdir -p "$T/app/bin" "$T/data" "$T/run"
cat >"$T/app/bin/tailscaled" <<'X'
#!/bin/sh
# Stand-in: takes the flags the card passes, makes the socket, writes a login into its state folder, waits.
for a; do case $a in --statedir=*) sd=${a#*=};; --socket=*) so=${a#*=};; --tun=*) tun=${a#*=};; esac; done
echo "tailscaled $*" >>"$BRICK_TEST_LOG"
if [ "$tun" = tailscale0 ] && [ -n "${FAKE_NO_TUN:-}" ]; then echo "CreateTUN: no such device" >&2; exit 1; fi
exec python3 -I -c '
import os, socket, sys, time
sd, so = sys.argv[1], sys.argv[2]
s = socket.socket(socket.AF_UNIX); s.bind(so); s.listen(1)
time.sleep(0.5)
old = open(os.path.join(sd, "tailscaled.state")).read() if os.path.exists(os.path.join(sd, "tailscaled.state")) else ""
open(os.path.join(sd, "tailscaled.state"), "w").write(old or "node-key-1")
time.sleep(600)' "$sd" "$so"
X
cat >"$T/app/bin/tailscale" <<'X'
#!/bin/sh
# Stand-in CLI: the signed-in node's status as tailscale prints it (the firmware's host name unless FAKE_TS_NAME), and `set` written down.
case "$*" in
  *"status --json --peers=false"*) printf '{\n  "BackendState": "Running",\n  "Self": {\n    "ID": "n1",\n    "HostName": "%s",\n    "DNSName": "x."\n  }\n}\n' "${FAKE_TS_NAME:-$(hostname)}"; exit 0 ;;
  *" set "*) echo "tailscale $*" >>"$BRICK_TEST_LOG"; exit 0 ;;
esac
echo "tailscale $*"
X
chmod +x "$T/app/bin/tailscaled" "$T/app/bin/tailscale"
export BRICK_APP=$T/app BRICK_DATA=$T/data BRICK_RUN=$T/run BRICK_TS_SYNC_EVERY=1 BRICK_TEST_LOG=$T/log BRICK_TUN_DEV=$T/no-tun
check "no tunnel device: userspace networking, the proxy for the updater, our socket and state folder" "$HW/systemctl start tailscaled && $HW/systemctl is-active --quiet tailscaled && [ \"\$(cat $T/run/tailscale-mode)\" = userspace ] && grep -q -- '--tun=userspace-networking --outbound-http-proxy-listen=127.0.0.1:1055' $T/log && grep -q -- '--statedir=$T/run/tailscale --socket=$T/run/tailscaled.sock' $T/log"
sleep 3
check "the login reaches the card within seconds" "[ \"\$(cat $T/data/storage/.cache/tailscale/tailscaled.state 2>/dev/null)\" = node-key-1 ]"
check "signed in with the firmware's host name: renamed pocket-brick once, without a new login" "[ \"\$(grep -c -- 'set --hostname=pocket-brick' $T/log)\" = 1 ] && ! grep -q -- ' up' $T/log"
check "the tailscale command talks to the card's daemon" "$HW/tailscale status | grep -q -- '--socket=$T/run/tailscaled.sock status'"
check "stop: daemon and watcher gone" "$HW/systemctl stop tailscaled && ! $HW/systemctl is-active --quiet tailscaled && ! kill -0 \$(cat $T/run/tailscale-sync.pid 2>/dev/null || echo 999999) 2>/dev/null"
echo node-key-7 >"$T/data/storage/.cache/tailscale/tailscaled.state"; rm -rf "$T/run"; mkdir -p "$T/run"
check "after a restart (RAM empty) the card's login is put back before the daemon starts" "$HW/brick-tailscale start && sleep 1 && [ \"\$(cat $T/run/tailscale/tailscaled.state)\" = node-key-7 ]"
$HW/brick-tailscale stop
: >"$T/log"
check "a name of its own (chosen on the Tailscale page) is kept" "FAKE_TS_NAME=my-brick $HW/brick-tailscale start && sleep 3 && ! grep -q -- 'set --hostname' $T/log"
check "already running (an update restarted pocket-ui): start again leaves the daemon, starts a new watcher, and that one names the node" "d1=\$(cat $T/run/tailscaled.pid); w1=\$(cat $T/run/tailscale-sync.pid); : >$T/log; $HW/brick-tailscale start && sleep 3 && [ \"\$(cat $T/run/tailscaled.pid)\" = \"\$d1\" ] && [ \"\$(cat $T/run/tailscale-sync.pid)\" != \"\$w1\" ] && ! kill -0 \$w1 2>/dev/null && grep -q -- 'set --hostname=pocket-brick' $T/log && ! grep -q '^tailscaled ' $T/log"
$HW/brick-tailscale stop
: >"$T/log"
check "a tunnel device that does not work: falls back to userspace by itself" "BRICK_TUN_DEV=/dev/null FAKE_NO_TUN=1 $HW/brick-tailscale start && grep -q -- '--tun=tailscale0' $T/log && grep -q -- '--tun=userspace-networking' $T/log && [ \"\$(cat $T/run/tailscale-mode)\" = userspace ]"
$HW/brick-tailscale stop; : >"$T/log"
check "a working tunnel device is used" "BRICK_TUN_DEV=/dev/null $HW/brick-tailscale start && [ \"\$(cat $T/run/tailscale-mode)\" = tun ] && ! grep -q userspace-networking $T/log"
$HW/brick-tailscale stop; : >"$T/log"; echo userspace >"$T/data/tailscale-mode"
check "pocket-ui/tailscale-mode on the card forces userspace" "BRICK_TUN_DEV=/dev/null $HW/brick-tailscale start && ! grep -q 'tun=tailscale0' $T/log"
$HW/brick-tailscale stop
check "nothing of ours is left running" "! $HW/brick-tailscale status"
unset BRICK_APP BRICK_DATA BRICK_RUN BRICK_TS_SYNC_EVERY BRICK_TEST_LOG BRICK_TUN_DEV

echo "== Sync: brick-syncthing with a stand-in syncthing"
Y=$WORK/st; mkdir -p "$Y/app/bin" "$Y/data/storage/.config/pocket-ui/sync" "$Y/run"
cat >"$Y/app/bin/syncthing" <<'X'
#!/bin/sh
# Stand-in: `generate` writes a config shaped like Syncthing's (a device with its own <address>), `serve` writes a key and waits.
echo "syncthing $*" >>"$BRICK_TEST_LOG"
case $1 in
  generate)
    for a; do case $a in --home) nx=1;; *) [ -n "${nx:-}" ] && { home=$a; nx=; };; esac; done
    cat >"$home/config.xml" <<'C'
<configuration version="51">
    <device id="AAAAAAA" name="other"><address>dynamic</address></device>
    <gui enabled="true" tls="false" sendBasicAuthPrompt="false">
        <address>127.0.0.1:8384</address>
        <user>pocket</user>
    </gui>
    <options>
        <urAccepted>0</urAccepted>
        <crashReportingEnabled>true</crashReportingEnabled>
        <startBrowser>true</startBrowser>
        <autoUpgradeIntervalH>12</autoUpgradeIntervalH>
    </options>
</configuration>
C
    [ -s "$home/cert.pem" ] || echo cert-1 >"$home/cert.pem"; [ -s "$home/key.pem" ] || echo key-1 >"$home/key.pem"
    cat >/dev/null; exit 0 ;;
  serve) exec sleep 600 ;;
esac
X
chmod +x "$Y/app/bin/syncthing"
export BRICK_APP=$Y/app BRICK_DATA=$Y/data BRICK_RUN=$Y/run BRICK_SYNC_EVERY=1 BRICK_TEST_LOG=$Y/log
check "no password file: it does not start (never an open interface)" "! $HW/systemctl start syncthing && ! $HW/systemctl is-active --quiet syncthing"
echo 'otter-cedar-lamp-315' >"$Y/data/storage/.config/pocket-ui/sync/password"
check "start: runs serve with RAM home, the card's database, no browser" "$HW/systemctl start syncthing && $HW/systemctl is-active --quiet syncthing && grep -q -- \"serve --home $Y/run/syncthing --data $Y/data/storage/.cache/syncthing-db --no-browser --no-restart\" $Y/log"
check "the password goes in on stdin, not on the command line" "! grep -q otter-cedar $Y/log && grep -q -- '--gui-user pocket --gui-password -' $Y/log"
C=$Y/run/syncthing/config.xml
check "web interface: all addresses, TLS, our login" "grep -q '<address>0.0.0.0:8384</address>' $C && grep -q '<gui [^>]*tls=\"true\"' $C"
check "no usage report, no crash report, no browser, no self-upgrade" "grep -q '<urAccepted>-1<' $C && grep -q '<crashReportingEnabled>false<' $C && grep -q '<startBrowser>false<' $C && grep -q '<autoUpgradeIntervalH>0<' $C"
check "a device's own address is left alone" "grep -q '<device id=\"AAAAAAA\" name=\"other\"><address>dynamic</address>' $C"
sleep 2
check "config and keys reach the card (where pocket-ui reads them) within seconds" "[ \"\$(cat $Y/data/storage/.config/syncthing/cert.pem)\" = cert-1 ] && grep -q '0.0.0.0:8384' $Y/data/storage/.config/syncthing/config.xml"
check "stop: syncthing and watcher gone" "$HW/systemctl stop syncthing && ! $HW/systemctl is-active --quiet syncthing && ! kill -0 \$(cat $Y/run/syncthing-watch.pid 2>/dev/null || echo 999999) 2>/dev/null"
echo cert-7 >"$Y/data/storage/.config/syncthing/cert.pem"; rm -rf "$Y/run"; mkdir -p "$Y/run"
check "after a restart (RAM empty) the card's keys are put back: the device keeps its ID" "$HW/systemctl restart syncthing && [ \"\$(cat $Y/run/syncthing/cert.pem)\" = cert-7 ]"
check "restart does not start it twice" "$HW/systemctl start syncthing && [ \$(grep -c '^syncthing serve' $Y/log) -eq 2 ]"
$HW/systemctl stop syncthing
check "nothing of ours is left running" "! $HW/brick-syncthing status"
unset BRICK_APP BRICK_DATA BRICK_RUN BRICK_SYNC_EVERY BRICK_TEST_LOG

echo "== brick-net: the default route comes back"
N=$WORK/net; mkdir -p "$N"
printf 'Iface\tDestination\tGateway\tFlags\nwlan0\t0000000A\t00000000\t0001\n' >"$N/route-lost"
printf 'Iface\tDestination\tGateway\tFlags\nwlan0\t0000000A\t00000000\t0001\nwlan0\t00000000\t0100000A\t0003\n' >"$N/route-ok"
printf 'Iface\tDestination\tGateway\tFlags\n' >"$N/route-none"
trap '' USR1          # inherited by the stand-in below: like udhcpc it lives through SIGUSR1 (its renew)
sleep 600 & UD=$!
trap - USR1
cat >"$N/pidof" <<X
#!/bin/sh
[ -e "$N/no-udhcpc" ] || echo $UD
X
printf '#!/bin/sh\necho "udhcpc $*" >>%s/calls\n' "$N" >"$N/udhcpc"; chmod +x "$N/pidof" "$N/udhcpc"
cat >"$N/iw" <<X
#!/bin/sh
[ -s "$N/ssid" ] && printf 'Connected to 00:00:5e:00:53:01 (on wlan0)\n\tSSID: %s\n\tfreq: 2422\n' "\$(cat "$N/ssid")"
exit 0
X
chmod +x "$N/iw"
net() { BRICK_PROC_ROUTE=$N/$1 BRICK_PIDOF=$N/pidof BRICK_UDHCPC=$N/udhcpc BRICK_IW=$N/iw BRICK_NET_SEEN=$N/seen BRICK_NET_LOG=$N/log "$HW/brick-net" check; }
check "a default route there: nothing done" "! net route-ok && [ ! -s $N/log ]"
check "no address on wlan0: nothing done" "! net route-none && [ ! -s $N/log ]"
check "address but no default route: the running udhcpc renews (SIGUSR1), the log says so" "net route-lost && grep -q 'lease renewed (udhcpc $UD)' $N/log && kill -0 $UD"
: >"$N/no-udhcpc"
check "address, no default route and no udhcpc: one is started" "net route-lost && grep -q 'udhcpc -i wlan0 -b' $N/calls && grep -q 'started one' $N/log"
kill $UD 2>/dev/null; wait $UD 2>/dev/null
# Moved to another Wi-Fi (the real Brick, 06.11.): the old network's address and default route stayed, "Connected" showed the old
# address. A new network name releases the lease (SIGUSR2) and asks for a new one (SIGUSR1), in that order; the log names no network.
rm -f "$N/no-udhcpc" "$N/seen" "$N/got"; : >"$N/log"
sh -c "trap 'echo usr2 >>$N/got' USR2; trap 'echo usr1 >>$N/got' USR1; while :; do sleep 1; done" & UD3=$!
sleep 1
printf '#!/bin/sh\necho %s\n' "$UD3" >"$N/pidof"
echo "Office Wi-Fi" >"$N/ssid"
check "the first network seen is only remembered" "! net route-ok && [ ! -s $N/got ] && [ \"\$(cat $N/seen)\" = 'Office Wi-Fi' ]"
check "the same network again: nothing done" "! net route-ok && [ ! -s $N/got ] && [ ! -s $N/log ]"
: >"$N/ssid"
check "not joined for a moment: nothing done, the last network kept" "! net route-ok && [ ! -s $N/got ] && [ \"\$(cat $N/seen)\" = 'Office Wi-Fi' ]"
echo "Home" >"$N/ssid"
check "another network with the old default route: the old lease released, then a new one asked for" "net route-ok && sleep 1 && [ \"\$(tr '\n' ' ' <$N/got)\" = 'usr2 usr1 ' ] && grep -q 'joined another network' $N/log && ! grep -q 'Home\|Office' $N/log"
kill $UD3 2>/dev/null; wait $UD3 2>/dev/null
# The cause seen on the real Brick: `systemctl suspend` stops and starts wpa_supplicant around the sleep; the udhcpc that ran through it
# must renew afterwards (SIGUSR1), and so must one after `wifictl enable`.
sh -c "trap 'echo usr1 >>$N/got' USR1; while :; do sleep 1; done" & UD2=$!
sleep 1
printf '#!/bin/sh\necho %s\n' "$UD2" >"$N/pidof2"; printf '#!/bin/sh\necho "wpa $1" >>%s/calls\n' "$N" >"$N/wpa"; printf '#!/bin/sh\ncase "$1" in bl-get) echo 100;; esac\n' >"$N/hw"
chmod +x "$N/pidof2" "$N/wpa" "$N/hw"; : >"$N/state"
check "after a suspend the supplicant is started again and the running udhcpc renews its lease" "BRICK_PIDOF=$N/pidof2 BRICK_WPA_INIT=$N/wpa BRICK_HW=$N/hw BRICK_POWER_STATE=$N/state BRICK_RFKILL=$N/no-rfkill $HW/systemctl suspend; sleep 2; grep -q 'wpa stop' $N/calls && grep -q 'wpa start' $N/calls && grep -q mem $N/state && [ -s $N/got ]"
: >"$N/got"
check "wifictl enable renews a running udhcpc as well" "BRICK_PIDOF=$N/pidof2 BRICK_WPA_INIT=$N/wpa BRICK_IFCONFIG=true BRICK_RFKILL=$N/no-rfkill BRICK_STATE=$N $HW/wifictl enable; sleep 2; [ -s $N/got ]"
kill $UD2 2>/dev/null; wait $UD2 2>/dev/null
# Power off puts the lights out (the owner saw them glowing on a Brick that was off or dark).
mkdir -p "$N/led"; for z in l r lr m f1 f2; do echo 2 >"$N/led/effect_$z"; done; echo 1 >"$N/led/effect_enable"
check "systemctl poweroff puts every light out first" "BRICK_LED_ANIM=$N/led BRICK_POWEROFF=true $HW/systemctl poweroff && [ \"\$(cat $N/led/effect_enable $N/led/effect_f1 $N/led/effect_lr | tr -d '\\n')\" = 000 ]"
printf '373737 373737 000000 ' >"$N/led/frame_hex"
check "systemctl poweroff blacks out the driver's raw picture too (23 LEDs)" "BRICK_LED_ANIM=$N/led BRICK_POWEROFF=true $HW/systemctl poweroff && [ \"\$(tr ' ' '\\n' <$N/led/frame_hex | grep -c '^000000\$')\" = 23 ] && ! grep -q 373737 $N/led/frame_hex"
check "start.sh starts the network keeper and stops it on the way out" "grep -q 'brick-net\" watch' $ROOT/deploy/brick/start.sh && [ \"\$(grep -c 'NETPID' $ROOT/deploy/brick/start.sh)\" -ge 4 ]"

echo "== nmcli stand-in (over a fake wpa_cli)"
NM=$HW/nmcli
W2=$WORK/wpa; mkdir -p "$W2"
TAB=$(printf '\t')
printf '0\tCafe\tany\t\n1\tHome\tany\t\n' >"$W2/networks"; echo 1 >"$W2/current"
printf '00:00:5e:00:53:01\t2422\t-65\t[WPA2-PSK-CCMP][ESS]\tHome\n00:00:5e:00:53:02\t2437\t-80\t[WPA2-PSK-CCMP][ESS]\tCafe\n00:00:5e:00:53:03\t2437\t-50\t[ESS]\tOpen: Net\n00:00:5e:00:53:04\t2412\t-70\t[WPA2-PSK-CCMP][ESS]\tNew\n' >"$W2/scan"
printf 'New\tsecret\n' >"$W2/psk-of"
mkdir -p "$W2/rf/rfkill1"; echo wlan >"$W2/rf/rfkill1/type"; echo 0 >"$W2/rf/rfkill1/soft"; echo 0 >"$W2/rf/rfkill1/hard"
export FAKE_WPA=$W2 BRICK_WPA_CLI=$ROOT/scripts/brick-fake-wpa-cli.sh BRICK_RFKILL=$W2/rf BRICK_SCAN_WAIT=0
check "radio wifi says enabled" "[ \"\$($NM radio wifi)\" = enabled ]"
check "radio wifi off, then disabled" "$NM radio wifi off; [ \"\$($NM radio wifi)\" = disabled ] && $NM radio wifi on && [ \"\$($NM radio wifi)\" = enabled ]"
check "terse list for the active network" "$NM -t -f ACTIVE,SSID device wifi list --rescan no | grep -Fxq 'yes:Home' && $NM -t -f ACTIVE,SSID device wifi list --rescan no | grep -Fxq 'no:Cafe'"
check "scan list: name, signal in percent (-65 dBm = 70), security, in use" "$NM -t -f SSID,SIGNAL,SECURITY,IN-USE dev wifi list --rescan yes | grep -qx 'Home:70:WPA2:\*'"
check "scan list: a colon in a name is escaped, an open network has no security" "$NM -t -f SSID,SIGNAL,SECURITY,IN-USE dev wifi list --rescan no | grep -Fxq 'Open\\: Net:100::'"
check "saved networks: name, uuid, type, active, state" "$NM -t -f NAME,UUID,TYPE,ACTIVE,STATE connection show | grep -qx 'Home:brick-net-1:802-11-wireless:yes:activated' && $NM -t -f NAME,UUID,TYPE,ACTIVE,STATE connection show | grep -qx 'Cafe:brick-net-0:802-11-wireless:no:'"
check "the ssid and hidden flag of a saved network" "[ \"\$($NM -e no -g 802-11-wireless.ssid,802-11-wireless.hidden connection show uuid brick-net-1 | tr '\\n' ' ')\" = 'Home no ' ]"
check "device show: state, connection uuid, reason" "[ \"\$($NM -g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0 | sed -n 1p)\" = '100 (connected)' ] && $NM -g GENERAL.STATE,GENERAL.CON-UUID,GENERAL.REASON device show wlan0 | sed -n 2p | grep -qx brick-net-1"
check "a wrong password: exit 4, 'Secrets were required', and the network is not kept" "! $NM -w 2 device wifi connect New password nope hidden no ifname wlan0 2>$WORK/err; [ \$? = 0 ]; grep -q 'Secrets were required' $WORK/err; ! grep -q '	New	' $W2/networks"
check "the right password: joined, saved to the supplicant's file, kept" "$NM -w 3 device wifi connect New password secret ifname wlan0 | grep -q 'successfully activated' && grep -q '	New	' $W2/networks && grep -q '^save_config' $W2/calls"
check "a name that is not in reach: exit 10" "$NM -w 2 device wifi connect Nowhere password x 2>$WORK/err; [ \$? = 10 ] && grep -q 'No network with SSID' $WORK/err"
check "forget by uuid, and forgetting one that is gone says 'unknown connection'" "$NM connection delete uuid brick-net-0 >/dev/null && ! grep -q '	Cafe	' $W2/networks && ! $NM connection delete id Cafe 2>$WORK/err; grep -qi 'unknown connection' $WORK/err"
check "connection modify is accepted" "$NM connection modify Home wifi-sec.psk-flags 0"
echo

echo "== brick-hw (host build, the display device replaced by a file)"
if command -v cc >/dev/null 2>&1; then
  cc -O1 -Wall -Werror -o "$S/brick-hw" "$ROOT/deploy/brick/brick-hw.c" && ok "brick-hw.c compiles without warnings" || bad "brick-hw.c does not compile"
  BRICK_DISP=$S/no-such-disp "$S/brick-hw" bl-get 2>/dev/null; check "bl-get fails (exit 1) when there is no display device" "[ $? = 1 ]"
  mkdir -p "$S/panel"
  ( BRICK_DISP=/dev/null timeout 1 "$S/brick-hw" watch "$S/panel" 40; true )
  check "watch makes max_brightness 255 and brightness 40 % of it" "[ \"\$(cat $S/panel/max_brightness)\" = 255 ] && [ \"\$(cat $S/panel/brightness)\" = 102 ]"
  check "watch keeps a level that is already there" "echo 77 >$S/panel/brightness; ( BRICK_DISP=/dev/null timeout 1 $S/brick-hw watch $S/panel 40; true ); [ \"\$(cat $S/panel/brightness)\" = 77 ]"
else
  echo "skip brick-hw (no cc)"
fi

echo "== Power (long hold: save and quit the game cleanly, then power off)"
if command -v cc >/dev/null 2>&1; then
  PW=$WORK/power; mkdir -p "$PW"
  cat >"$PW/systemctl" <<X
#!/bin/sh
echo "poweroff \$*" >>$PW/log
X
  chmod +x "$PW/systemctl"
  game() { # how: term-saves | term-saves-slowly (its exit save marked, longer than the wait) | term-ignored
    : >"$PW/log"; rm -f "$PW/saving"
    if [ "$1" = term-saves ]; then
      ( trap 'sleep 0.4; echo "game: saved" >>'"$PW"'/log; exit 0' TERM; while :; do sleep 0.1; done ) &
    elif [ "$1" = term-saves-slowly ]; then
      ( trap 'echo "$(cat '"$PW"'/pid) 4242" >'"$PW"'/saving; sleep 2.5; echo "game: saved" >>'"$PW"'/log; rm -f '"$PW"'/saving; exit 0' TERM; while :; do sleep 0.1; done ) &
    else
      ( trap '' TERM; while :; do sleep 0.1; done ) &
    fi
    echo $! >"$PW/pid"; sleep 0.3
  }
  export BRICK_GAME_PIDFILE=$PW/pid BRICK_SYSTEMCTL=$PW/systemctl BRICK_POWER_LOG=$PW/log
  game term-saves; "$HW/brick-power-off"
  check "a running game saves first, then the Brick powers off (that order)" "[ \"\$(grep -v 'power-off:' $PW/log | tr '\\n' ' ')\" = 'game: saved poweroff poweroff ' ] || [ \"\$(grep -v 'power-off:' $PW/log | tr '\\n' ' ')\" = 'game: saved poweroff ' ]"
  check "the game was gone before power off" "! kill -0 \$(cat $PW/pid) 2>/dev/null"
  game term-ignored; BRICK_QUIT_WAIT=1 "$HW/brick-power-off"
  check "a game that ignores the request is killed after the wait, then power off" "grep -q SIGKILL $PW/log && grep -q '^poweroff' $PW/log && ! kill -0 \$(cat $PW/pid) 2>/dev/null"
  export BRICK_SAVING_FILE=$PW/saving
  game term-saves-slowly; BRICK_QUIT_WAIT=1 "$HW/brick-power-off"
  check "a game still writing its marked exit save past the wait is not killed: saved, then power off" "! grep -q SIGKILL $PW/log && [ \"\$(grep -v 'power-off:' $PW/log | head -1)\" = 'game: saved' ] && grep -q '^poweroff' $PW/log"
  game term-ignored; echo "$(cat "$PW/pid") 4242" >"$PW/saving"; BRICK_QUIT_WAIT=1 BRICK_SAVE_LIMIT=2 "$HW/brick-power-off"
  check "a game that only claims to save is still killed at the save limit" "grep -q SIGKILL $PW/log && grep -q '^poweroff' $PW/log && ! kill -0 \$(cat $PW/pid) 2>/dev/null"
  game term-ignored; echo "999999 4242" >"$PW/saving"; BRICK_QUIT_WAIT=1 "$HW/brick-power-off"
  check "a mark of another (gone) process gives no extra wait" "grep -q SIGKILL $PW/log && ! kill -0 \$(cat $PW/pid) 2>/dev/null"
  unset BRICK_SAVING_FILE
  : >"$PW/log"; echo 999999 >"$PW/pid"; "$HW/brick-power-off"
  check "no game running: power off at once" "grep -q 'no game running' $PW/log && grep -q '^poweroff' $PW/log"
  # The key watcher: a FIFO of input events stands in for the PMIC's key (KEY_POWER = 116; 24-byte events).
  mkfifo "$PW/ev"
  press() { python3 -I -c '
import struct, sys, time
f = open(sys.argv[1], "wb", buffering=0)
ev = lambda v: f.write(struct.pack("qqHHi", 0, 0, 1, 116, v))
for hold in sys.argv[2:]:
    ev(1); time.sleep(float(hold)); ev(0); time.sleep(0.3)
time.sleep(1)' "$PW/ev" "$@" & }
  : >"$PW/log"; echo 999999 >"$PW/pid"
  BRICK_POWER_DEV=$PW/ev BRICK_POWER_CMD="echo fired >>$PW/fired" "$S/brick-hw" power-watch 1 &
  WATCH=$!
  press 0.4 1.8 0.2
  sleep 4.5
  check "short and medium presses are not ours, a 1.8 s hold with a 1 s limit fires exactly once" "[ \"\$(wc -l <$PW/fired 2>/dev/null)\" = 1 ]"
  kill $WATCH 2>/dev/null; wait $WATCH 2>/dev/null
  rm -f "$PW/fired"; game term-saves; mkfifo "$PW/ev2" 2>/dev/null
  BRICK_POWER_DEV=$PW/ev2 BRICK_POWER_CMD="$HW/brick-power-off" "$S/brick-hw" power-watch 1 &
  WATCH=$!
  python3 -I -c '
import struct, sys, time
f = open(sys.argv[1], "wb", buffering=0)
f.write(struct.pack("qqHHi", 0, 0, 1, 116, 1)); time.sleep(2.5); f.write(struct.pack("qqHHi", 0, 0, 1, 116, 0)); time.sleep(0.5)' "$PW/ev2"
  check "the whole chain: hold, the game saves and ends, then power off" "grep -q 'game: saved' $PW/log && grep -q '^poweroff' $PW/log && [ \$(grep -n 'game: saved' $PW/log | cut -d: -f1) -lt \$(grep -n '^poweroff' $PW/log | head -1 | cut -d: -f1) ]"
  kill $WATCH 2>/dev/null; wait $WATCH 2>/dev/null
  unset BRICK_GAME_PIDFILE BRICK_SYSTEMCTL BRICK_POWER_LOG
  NOWHERE=$(BRICK_POWER_DEV= BRICK_INPUT_SYS=$PW/none BRICK_POWER_CMD=true "$S/brick-hw" power-watch 1 2>&1); rc=$?
  check "no such input device: says so and exits (1)" "[ $rc = 1 ] && echo '$NOWHERE' | grep -q axp2202-pek"
else
  echo "skip power (no cc)"
fi

echo
if [ "$fails" = 0 ]; then echo "test-brick-card: all passed"; else echo "test-brick-card: $fails FAILED"; exit 1; fi
