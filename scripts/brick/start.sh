#!/bin/sh
# Starts pocket-ui on the TrimUI Brick (docs/TRIMUI-BRICK.md). The only start script: the pak on NextUI's card and, later, a
# standalone card's updater are both thin entries into this. Written for this project; nothing here comes from NextUI or MinUI.
#   start.sh [nextui|standalone]       nextui: the screen is ours until we end, then the caller (NextUI) comes back by itself
# Needs: /dev/fb0, our bin/ and lib/ next to this file. A missing piece is said on the screen and in logs/start.log.
# POCKET_MIN_BATTERY=<percent>: do not draw below it (the kit sets 20). POCKET_VIEW=0: no /storage view (tests).
# POCKET_BRICK_GL=1: the screen through the GPU (docs/TRIMUI-BRICK.md, "The GPU"). Asked first in a process of its own; if that fails, or
# the bridge dies with it, the CPU copy runs and logs/start.log says so. Without the variable: the CPU copy.
ROOT=$(cd "$(dirname "$0")" && pwd)
MODE=${1:-nextui}
mkdir -p "$ROOT/logs" "$ROOT/data" "$ROOT/storage/roms/savestates" "$ROOT/storage/roms/bios" "$ROOT/storage/.config" 2>/dev/null
LOG=$ROOT/logs/start.log
. "$ROOT/env.sh"
FB=${POCKET_FB:-/dev/fb0}
# A plain file as the screen (tests): pocket-fb needs its size then.
FBMODE=; [ -f "$FB" ] && FBMODE="--mode 1024x768x32"
VIEW=/tmp/pocket-view
log() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*" >>"$LOG"; }

# Says it on the screen (pocket-fb paints the text, | is a new line) and in the log; then leaves, and the caller comes back.
die() {
  log "STOP: $1"
  if [ -x "$ROOT/bin/pocket-fb" ] && [ -w "$FB" ]; then
    LD_LIBRARY_PATH=$LD_LIBRARY_PATH "$ROOT/bin/pocket-fb" --message "pocket-ui can't start|$1|$2|(details: logs/start.log in the pocket-ui folder)" --hold "${POCKET_MESSAGE_SECONDS:-8}" --fb "$FB" $FBMODE >>"$LOG" 2>&1
  fi
  exit 1
}

battery=$(cat /sys/class/power_supply/*/capacity 2>/dev/null | head -1)
if [ -n "${POCKET_MIN_BATTERY:-}" ] && [ -n "$battery" ] && [ "$battery" -lt "$POCKET_MIN_BATTERY" ]; then
  die "Battery at $battery %." "Charge it a little, then start again."
fi
[ -c "$FB" ] || [ -f "$FB" ] || die "No screen device ($FB)." "This does not look like a TrimUI Brick."
for f in pocket-ui pocket-core pocket-fb; do
  [ -x "$ROOT/bin/$f" ] || die "The file bin/$f is missing." "Copy the whole pocket-ui folder again."
done
for f in libwayland-client.so.0 libwayland-cursor.so.0 libxkbcommon.so.0; do
  [ -e "$ROOT/lib/$f" ] || die "The library lib/$f is missing." "Copy the whole pocket-ui folder again."
done
[ -d "$CORES" ] || log "no cores folder $CORES: games will not start, the shelf does"
log "=== start ($MODE), battery ${battery:-unknown} %, fb $FB"

STAGE_PIDS=
cleanup() {
  trap - EXIT INT TERM HUP
  # pocket-ui first (it ends its game, which saves), then the bridge; up to 4 s before the KILL.
  for p in $STAGE_PIDS; do kill "$p" 2>/dev/null; done
  n=0
  while [ $n -lt 20 ]; do
    alive=0; for p in $STAGE_PIDS; do kill -0 "$p" 2>/dev/null && alive=1; done
    [ $alive = 0 ] && break
    sleep 0.2; n=$((n + 1))
  done
  for p in $STAGE_PIDS; do kill -KILL "$p" 2>/dev/null; done
  hw_off
  view_off
  rm -f "$WAYLAND_DISPLAY" "$SWAYSOCK"
  log "=== end"
}
trap cleanup EXIT INT TERM HUP

# pocket-ui and pocket-core look for /storage (games, save states, settings); this system has none and its root stays as it
# is. A copy of the system's folders in /tmp, with our own /storage, that the two programs run inside. Only mounts in /tmp.
view_on() {
  [ "${POCKET_VIEW:-1}" = 0 ] && return 0
  mkdir -p "$VIEW" || return 1
  for d in dev proc sys tmp lib usr bin sbin etc mnt var run storage; do
    mkdir -p "$VIEW/$d"
    src=/$d; [ "$d" = storage ] && src=$ROOT/storage
    [ -d "$src" ] || continue
    mount --bind "$src" "$VIEW/$d" || { log "bind $src failed"; view_off; return 1; }
  done
  # A plain bind of /mnt leaves the card's mount behind, and our programs live on the card.
  if [ -d /mnt/SDCARD ]; then mount --bind /mnt/SDCARD "$VIEW/mnt/SDCARD" || { log "bind /mnt/SDCARD failed"; view_off; return 1; }; fi
  if [ -n "${POCKET_PS1:-}" ] && [ -d "/mnt/SDCARD/Roms/Sony PlayStation (PS)/$POCKET_PS1" ]; then
    # One PlayStation game of the owner's, read only, where the shelf looks for PlayStation games; its saves go to storage/saves/psx.
    mkdir -p "$VIEW/storage/roms/psx"
    { mount --bind "/mnt/SDCARD/Roms/Sony PlayStation (PS)/$POCKET_PS1" "$VIEW/storage/roms/psx" && mount -o remount,ro,bind "$VIEW/storage/roms/psx"; } || log "the PlayStation game could not be bound read only"
  fi
  # The loader looks in /lib64, on this system a link to lib.
  [ -e "$VIEW/lib64" ] || ln -s lib "$VIEW/lib64"
}
view_off() {
  [ -d "$VIEW" ] || return 0
  for d in storage/roms/psx storage run var mnt/SDCARD mnt etc sbin bin usr lib tmp sys proc dev; do
    grep -q " $VIEW/$d " /proc/mounts 2>/dev/null && umount "$VIEW/$d" 2>/dev/null
  done
}
in_view() { if [ "${POCKET_VIEW:-1}" = 0 ]; then "$@"; else chroot "$VIEW" "$@"; fi; }

# The shelf's systems and the cores for them: files in OUR folder (the view's /storage), made again at every start so a changed
# line here is enough. The cores are NextUI's own (by path, read only): gpsp, snes9x, gambatte, fceumm, pcsx_rearmed, picodrive.
make_config() {
  c=$ROOT/storage/.config
  mkdir -p "$c/emulationstation" "$c/system/configs" "$ROOT/storage/roms/gb" "$ROOT/storage/roms/gbc" "$ROOT/storage/roms/gba" "$ROOT/storage/roms/nes" "$ROOT/storage/roms/snes" "$ROOT/storage/roms/psx" "$ROOT/storage/roms/megadrive" "$ROOT/storage/roms/mastersystem" "$ROOT/storage/roms/gamegear"
  cat >"$c/emulationstation/es_systems.cfg" <<'CFG'
<systemList>
  <system><name>gb</name><fullname>Game Boy</fullname><path>/storage/roms/gb</path><extension>.gb .zip</extension></system>
  <system><name>gbc</name><fullname>Game Boy Color</fullname><path>/storage/roms/gbc</path><extension>.gbc .zip</extension></system>
  <system><name>gba</name><fullname>Game Boy Advance</fullname><path>/storage/roms/gba</path><extension>.gba .zip</extension></system>
  <system><name>nes</name><fullname>Nintendo Entertainment System</fullname><path>/storage/roms/nes</path><extension>.nes .zip</extension></system>
  <system><name>snes</name><fullname>Super Nintendo</fullname><path>/storage/roms/snes</path><extension>.sfc .smc .zip</extension></system>
  <system><name>psx</name><fullname>PlayStation</fullname><path>/storage/roms/psx</path><extension>.cue .chd .m3u .pbp</extension></system>
  <system><name>megadrive</name><fullname>Sega Mega Drive</fullname><path>/storage/roms/megadrive</path><extension>.md .gen .smd .bin .zip</extension></system>
  <system><name>mastersystem</name><fullname>Sega Master System</fullname><path>/storage/roms/mastersystem</path><extension>.sms .zip</extension></system>
  <system><name>gamegear</name><fullname>Sega Game Gear</fullname><path>/storage/roms/gamegear</path><extension>.gg .zip</extension></system>
</systemList>
CFG
  cat >"$c/system/configs/system.cfg" <<'CFG'
gb.emulator=pocket-core
gb.core=gambatte
gbc.emulator=pocket-core
gbc.core=gambatte
gba.emulator=pocket-core
gba.core=gpsp
nes.emulator=pocket-core
nes.core=fceumm
snes.emulator=pocket-core
snes.core=snes9x
psx.emulator=pocket-core
psx.core=pcsx_rearmed
megadrive.emulator=pocket-core
megadrive.core=picodrive
mastersystem.emulator=pocket-core
mastersystem.core=picodrive
gamegear.emulator=pocket-core
gamegear.core=picodrive
CFG
}
make_config

# probe_gl: "--gl" if the GPU can draw here. brick-egl may crash or hang inside the blob, so it runs apart with a time limit.
probe_gl() {
  [ -x "$ROOT/bin/brick-egl" ] || { log "GL asked for, but no bin/brick-egl: CPU copy"; return; }
  "$ROOT/bin/brick-egl" --probe --window "${POCKET_GL_WINDOW:-null}" >>"$ROOT/logs/pocket-fb.log" 2>&1 &
  gp=$!
  ( sleep 10; kill -KILL "$gp" 2>/dev/null ) >/dev/null 2>&1 &
  gk=$!
  wait "$gp"; gc=$?
  kill "$gk" 2>/dev/null
  if [ "$gc" = 0 ]; then FBGL="--gl --gl-window ${POCKET_GL_WINDOW:-null}"; log "GL: the probe works, the screen goes through the GPU"; else log "GL: the probe failed (exit $gc, see logs/pocket-fb.log): CPU copy"; fi
}
# The bridge and its sockets; false if it died or they never came.
start_bridge() {
  rm -f "$WAYLAND_DISPLAY" "$SWAYSOCK"
  "$ROOT/bin/pocket-fb" --wayland "$WAYLAND_DISPLAY" --ipc "$SWAYSOCK" --fb "$FB" $FBMODE $1 >>"$ROOT/logs/pocket-fb.log" 2>&1 &
  FBPID=$!
  STAGE_PIDS="$STAGE_PIDS $FBPID"
  i=0
  while [ ! -S "$WAYLAND_DISPLAY" ] || [ ! -S "$SWAYSOCK" ]; do
    i=$((i + 1))
    kill -0 "$FBPID" 2>/dev/null || return 1
    [ $i -gt 50 ] && return 2
    sleep 0.1
  done
}
# Volume, brightness and the battery for Settings and the Quick settings. pocket-ui's hardware seam (system_ctl RealHw::with_root,
# POCKETUI_HW_ROOT) looks below a folder of ours in /tmp: a backlight file that brick-hw (ours) writes to the panel through the display
# driver, the battery, and the stand-in programs volume, brightness and pactl (hwroot/usr/bin, plain amixer). Nothing of NextUI's is
# written: not its settings file, not .userdata. NextUI's own keymon and audiomon keep running, so the hardware keys still work.
# What we found on entry is noted, and put back at the end only if WE changed it (a key press of NextUI's own stays).
HW=/tmp/pocket-hw
STATE=$ROOT/state
BATT=${BRICK_BATTERY:-/sys/class/power_supply/axp2202-battery}
orig_vol=; orig_bri=; HWPID=
hw_on() {
  [ -x "$ROOT/bin/brick-hw" ] || { log "no bin/brick-hw: volume and brightness stay as NextUI has them, Settings can't change them"; return 0; }
  rm -rf "$HW"; mkdir -p "$STATE" "$HW/sys/class/backlight/panel" "$HW/sys/class/power_supply" "$HW/usr/bin"
  ln -s "$BATT" "$HW/sys/class/power_supply/battery"
  ln -s /usr/share "$HW/usr/share"; ln -s /etc "$HW/etc"; ln -s /dev "$HW/dev"
  for f in "$ROOT"/hwroot/usr/bin/*; do ln -s "$f" "$HW/usr/bin/$(basename "$f")"; done
  chmod +x "$ROOT"/hwroot/usr/bin/* 2>/dev/null
  orig_vol=$(amixer -c 0 cget name='digital volume' 2>/dev/null | sed -n 's/.*: values=\([0-9]*\).*/\1/p' | head -1)
  orig_bri=$("$ROOT/bin/brick-hw" bl-get 2>/dev/null)
  rm -f "$STATE/brick-volume" "$STATE/brick-brightness"
  # The backlight file starts at what the panel has now, so the watcher changes nothing until Settings does.
  [ -n "$orig_bri" ] && { echo 255 >"$HW/sys/class/backlight/panel/max_brightness"; echo "$orig_bri" >"$HW/sys/class/backlight/panel/brightness"; }
  "$ROOT/bin/brick-hw" watch "$HW/sys/class/backlight/panel" >>"$ROOT/logs/brick-hw.log" 2>&1 &
  HWPID=$!
  STAGE_PIDS="$STAGE_PIDS $HWPID"
  export POCKETUI_HW_ROOT=$HW POCKETUI_BACKLIGHT=$HW/sys/class/backlight/panel POCKETUI_BATTERY=$BATT BRICK_STATE=$STATE BRICK_HW=$ROOT/bin/brick-hw PATH="$HW/usr/bin:$PATH"
  log "hardware stand-ins on: volume $orig_vol of 63, brightness ${orig_bri:-?} of 255"
}
# At the end: our changes go back (see above); the watcher is stopped with the other helpers.
hw_off() {
  [ -n "$HWPID" ] || return 0
  kill "$HWPID" 2>/dev/null
  if [ -f "$STATE/brick-volume" ] && [ -n "$orig_vol" ]; then amixer -c 0 -q cset name='digital volume' "$orig_vol" >/dev/null 2>&1; log "volume put back to $orig_vol"; fi
  if [ -f "$STATE/brick-brightness" ] && [ -n "$orig_bri" ]; then "$ROOT/bin/brick-hw" bl-set "$orig_bri" >/dev/null 2>&1; log "brightness put back to $orig_bri"; fi
  HWPID=
}

rm -f "$WAYLAND_DISPLAY" "$SWAYSOCK"
view_on || die "Could not set up the games folder view." "Details are in logs/start.log."
hw_on
FBGL=
[ "${POCKET_BRICK_GL:-}" = 1 ] && probe_gl
start_bridge "$FBGL"; rc=$?
if [ $rc != 0 ] && [ -n "$FBGL" ]; then
  # The GPU took the bridge with it: once more without.
  log "GL: the bridge stopped with it: CPU copy"
  kill -KILL "$FBPID" 2>/dev/null
  start_bridge ""; rc=$?
fi
[ $rc = 1 ] && die "The screen bridge (pocket-fb) stopped." "See logs/pocket-fb.log."
[ $rc != 0 ] && die "The screen bridge did not come up." "See logs/pocket-fb.log."
# In the background and waited for: a TERM to this script then runs the clean-up at once (waiting in the foreground it ran only after
# pocket-ui had ended on its own, and a stop left pocket-ui behind, 2026-11-03).
# (The command itself in the background, not the function that runs it: its pid is then pocket-ui's, chroot executes it.)
if [ "${POCKET_VIEW:-1}" = 0 ]; then "$ROOT/bin/pocket-ui" >>"$ROOT/logs/pocket-ui.log" 2>&1 &
else chroot "$VIEW" "$ROOT/bin/pocket-ui" >>"$ROOT/logs/pocket-ui.log" 2>&1 &
fi
UIPID=$!
STAGE_PIDS="$UIPID $STAGE_PIDS"
wait "$UIPID"
code=$?
log "pocket-ui ended with $code"
exit 0
