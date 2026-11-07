#!/bin/sh
# The TrimUI Brick's first hour on NextUI (docs/TRIMUI-BRICK.md). Written for this project; nothing here comes
# from NextUI or MinUI. POSIX sh: the Brick has no bash, python, timeout or setsid.
#
# From the build machine (scripts/brick-run.sh wraps the lock and ssh; the lock /tmp/brick-device.lock is held for the WHOLE stage):
#   scripts/brick-kit.sh push <host> <folder with the 4 binaries> <folder with lib/*.so.*>      copies the kit, nothing runs
#   scripts/brick-run.sh <stage>                                                               one stage over ssh
# Stages without the screen (run at once, any time):
#   guard     md5 lists of Saves and .userdata: the first call takes the list, the next ones compare with it
#   read      only reads: kernel, glibc, fb, RAM, audio, input, what is writable. Touches nothing
#   binary    the binaries load and start (loader check, --version) and paint a snapshot into a file. Screen untouched
#   viewcheck the time zone and the volume/brightness stand-ins as the programs see them. Changes nothing, no screen
#   rom       copies one small GBA and one SNES game of the owner's library into OUR folder (read only on his side)
# Stages that take the screen (battery >= 20 %, nobody playing, the owner told first; NextUI comes back by itself after each):
#   snapshot  a test picture on fb0 for 5 s
#   input     30 s of the pad's key codes; press every button while it runs
#   ui        the real pocket-ui on the bridge with a made-up library, 120 s to press around in
#   game      pocket-core with NextUI's gpsp / snes9x core (by path) and our COPY of a game, CPU copy, with its numbers
#   pause     game and UI as two windows, 10 x over each other, the pause menu's way
#   pak       start.sh exactly as the pak runs it (60 s, then it ends itself)
#   soak      the audio question: interleaved 60 s snes9x runs, default and bigger ALSA buffer, with underruns per run
#   egl       the GPU: EGL on the Null WSI, GL_VERSION, 20 s of frame times
#   gltest    the GPU on the panel for a person to look at: 20 s moving test, frame times, read-back of the visible page; ends by itself
#   glplay    the real pocket-ui through the GPU for the owner to play with (pause menu, a game, the PS1 start), sound off, at most 8 min
#   glui      the UI through the GPU: same scripted presses on the CPU copy and on GL (made-up library, no sound), then 90 s of the real UI on GL
# How a screen stage runs: the kit writes the stage name into $KIT_ROOT/next-stage and the owner presses A on "pocket-ui" in NextUI's
# Tools list (a pak of ours, Tools/tg5040/pocket-ui.pak). NextUI hands the screen over by its own loop and takes it back when the stage
# ends. NOTHING in the kit ends nextui.elf (a TERM to it powers the Brick off). With no file, the pak starts the real pocket-ui.
#
# Rules it keeps: writes only under $KIT_ROOT (/mnt/SDCARD/.pocket-ui-test) and /tmp, mounts only below /tmp; never Roms, Saves, Bios,
# Cheats, .userdata, .system, .tmp_update, /usr, /etc, /lib; never reboots, powers off or suspends, never touches the volume. A game
# is only used as a copy in our folder, its saves land there. A stage ends at its limit and below 20 % battery.
# KIT_FAKE=1 on a computer: a file as fb0, no NextUI, no battery, host binaries (to test this script).
KIT_ROOT=${KIT_ROOT:-/mnt/SDCARD/.pocket-ui-test}
ROOT=$KIT_ROOT
B=$KIT_ROOT/bin
LOGS=$KIT_ROOT/logs
GUARD=$KIT_ROOT/guard
FAKE=${KIT_FAKE:-}
SD=${SD:-/mnt/SDCARD}
WIDTH=1024
HEIGHT=768
MIN_BATTERY=20
SELF=$KIT_ROOT/brick-kit.sh
RUN=/tmp/pocket-kit

if [ "${1:-}" = push ]; then
  host=${2:?host}; bins=${3:?folder with the binaries}; libs=${4:?folder with lib/}
  here=$(cd "$(dirname "$0")" && pwd)
  ssh_opts="-o ConnectTimeout=10 -o BatchMode=yes"
  # A staging folder with the layout of the kit's folder, sent as one tar (the Brick has no sftp for scp).
  stage=$(mktemp -d)
  mkdir -p "$stage/bin" "$stage/lib" "$stage/logs" "$stage/guard" "$stage/fakelib"
  fake=$(mktemp -d)
  python3 -I "$here/curator-fake-library.py" "$fake" dark >/dev/null 2>&1 || echo "no made-up library (python3 on this computer?)"
  [ -d "$fake/data" ] && cp -R "$fake/data" "$fake/es_systems.cfg" "$stage/fakelib/"
  # The same library for the glui stage, unpacked to /tmp when it runs: a game's name may not be a name on the card's FAT.
  python3 -I "$here/curator-fake-library.py" "$fake/lib" dark /tmp/pocket-kit/lib/roms >/dev/null 2>&1 && sed -i "s#$fake/lib#/tmp/pocket-kit/lib#g" "$fake/lib/es_systems.cfg" && tar -C "$fake" -cf "$stage/fakelib.tar" lib
  cp "$0" "$stage/brick-kit.sh"; cp "$here/brick/start.sh" "$here/brick/env.sh" "$stage/"
  mkdir -p "$stage/pak"; cp "$here/brick/pocket-ui-test.pak/launch.sh" "$stage/pak/launch.sh"
  [ -n "${KIT_TESTCARD:-}" ] && cp "$KIT_TESTCARD" "$stage/testcard.png"
  cp "$bins/pocket-ui" "$bins/pocket-core" "$bins/pocket-fb" "$bins/brick-egl" "$stage/bin/" || exit 1
  [ -f "$bins/brick-hw" ] && cp "$bins/brick-hw" "$stage/bin/"
  cp -R "$here/brick/hwroot" "$stage/hwroot"
  cp -L "$libs"/*.so.* "$stage/lib/" || exit 1
  tar -C "$stage" -cf - . | ssh $ssh_opts "root@$host" "mkdir -p $KIT_ROOT && tar -xf - -C $KIT_ROOT && chmod +x $KIT_ROOT/brick-kit.sh $KIT_ROOT/start.sh $KIT_ROOT/bin/*; du -sh $KIT_ROOT"
  rc=$?
  rm -rf "$fake" "$stage"
  exit $rc
fi

mkdir -p "$LOGS" "$GUARD" "$RUN" "$KIT_ROOT/data" "$KIT_ROOT/storage/roms/savestates" "$KIT_ROOT/storage/roms/bios" "$KIT_ROOT/storage/.config" 2>/dev/null
# shellcheck disable=SC1091
. "$KIT_ROOT/env.sh"
if [ -n "$FAKE" ]; then
  FB=$KIT_ROOT/fakefb
  [ -f "$FB" ] || head -c $((WIDTH * HEIGHT * 4)) /dev/zero >"$FB"
  FB_ARGS="--fb $FB --mode ${WIDTH}x${HEIGHT}x32"
  export POCKET_VIEW=0
else
  FB=/dev/fb0
  FB_ARGS="--fb /dev/fb0"
fi
WL=$WAYLAND_DISPLAY
IPC=$SWAYSOCK

say() { printf '%s %s\n' "$(date +%H:%M:%S)" "$*"; }
ok() { say "RESULT $*"; }
nap() { sleep "$1"; }
up() { cut -d' ' -f1 /proc/uptime | tr -d .; }

# limit <seconds> <command...>: the command, ended (TERM, then KILL) when the time is up. Returns its exit code.
limit() {
  n=$1; shift
  "$@" &
  lp=$!
  ( sleep "$n"; kill -TERM "$lp" 2>/dev/null; sleep 2; kill -KILL "$lp" 2>/dev/null ) >/dev/null 2>&1 &
  lk=$!
  wait "$lp"; lrc=$?
  kill "$lk" 2>/dev/null
  return "$lrc"
}

battery() {
  for f in /sys/class/power_supply/*; do
    [ "$(cat "$f/type" 2>/dev/null)" = Battery ] && [ -r "$f/capacity" ] && { cat "$f/capacity"; return; }
  done
  echo unknown
}
battery_ok() {
  [ -n "$FAKE" ] && return 0
  level=$(battery)
  case "$level" in unknown) say "battery level unknown: stopping"; return 1 ;; esac
  if [ "$level" -lt "$MIN_BATTERY" ]; then say "STOP: battery at $level %, the limit is $MIN_BATTERY %"; return 1; fi
  return 0
}

# Someone is playing (a core runs under NextUI's frontend or any emulator): then nothing takes the screen.
playing() {
  [ -n "$FAKE" ] && return 1
  for p in minarch.elf retroarch drastic ppssppSDL mupen64plus flycast; do
    pidof "$p" >/dev/null 2>&1 && { say "$p is running: the owner is playing, not taking the screen"; return 0; }
  done
  return 1
}

# A process of ours by the pid we kept, and only if it still is the program we started.
stop_saved() {
  for f in "$RUN"/game.pid "$RUN"/ui.pid "$RUN"/fb.pid; do
    [ -e "$f" ] || continue
    pid=$(cut -d' ' -f1 "$f"); name=$(cut -d' ' -f2 "$f")
    if [ -n "$pid" ] && [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = "$name" ]; then
      kill -TERM "$pid" 2>/dev/null
      for _ in 1 2 3 4 5 6 7 8 9 10; do kill -0 "$pid" 2>/dev/null || break; sleep 0.2; done
      kill -KILL "$pid" 2>/dev/null
    fi
    rm -f "$f"
  done
}
# start_kept <tag> <program> <args...>: in the background, its pid and name noted. A program run in the view (chroot) is noted by
# the name of the chroot, so those are started with start_view.
start_kept() {
  tag=$1; shift
  "$@" >>"$LOGS/$tag.out" 2>&1 &
  pid=$!
  printf '%s %s\n' "$pid" "$(basename "$1" | cut -c1-15)" >"$RUN/$tag.pid"
  say "started $tag ($*) pid $pid"
}
start_view() {
  tag=$1; shift
  prog=$(basename "$1" | cut -c1-15)
  if [ "${POCKET_VIEW:-1}" = 0 ]; then start_kept "$tag" "$@"; else start_kept "$tag" chroot /tmp/pocket-view "$@"; fi
  # chroot becomes the program: stop_saved compares the pid's name with this one.
  printf '%s %s\n' "$(cut -d' ' -f1 "$RUN/$tag.pid")" "$prog" >"$RUN/$tag.pid"
}

view_on() {
  [ "${POCKET_VIEW:-1}" = 0 ] && return 0
  mkdir -p /tmp/pocket-view
  for d in dev proc sys tmp lib usr bin sbin etc mnt var run storage; do
    mkdir -p "/tmp/pocket-view/$d"
    src=/$d; [ "$d" = storage ] && src=$KIT_ROOT/storage
    [ -d "$src" ] && { grep -q " /tmp/pocket-view/$d " /proc/mounts || mount --bind "$src" "/tmp/pocket-view/$d"; }
  done
  # A plain bind of /mnt leaves the card's mount behind, and our programs live on the card.
  grep -q " /tmp/pocket-view/mnt/SDCARD " /proc/mounts || mount --bind /mnt/SDCARD /tmp/pocket-view/mnt/SDCARD
  # The loader looks in /lib64, on this system a link to lib.
  [ -e /tmp/pocket-view/lib64 ] || ln -s lib /tmp/pocket-view/lib64
}
view_off() {
  for d in storage/roms/psx storage run var mnt/SDCARD mnt etc sbin bin usr lib tmp sys proc dev; do
    grep -q " /tmp/pocket-view/$d " /proc/mounts 2>/dev/null && umount "/tmp/pocket-view/$d" 2>/dev/null
  done
}

# Idempotent: what a stage started goes away, whatever state it died in.
restore() {
  stop_saved
  view_off
  rm -f "$WL" "$IPC"
}

# Dead-man and battery watch: ends the stage at its limit, and a stage with the screen at the battery floor too (NEED_BATTERY), even
# with the SSH gone. The stages that only read run with any battery.
watch_stage() {
  wlimit=$1; stage_pid=$2
  (
    start=$(date +%s)
    while kill -0 "$stage_pid" 2>/dev/null; do
      now=$(date +%s)
      if [ $((now - start)) -ge "$wlimit" ] || { [ "${NEED_BATTERY:-0}" = 1 ] && ! battery_ok >/dev/null; }; then
        echo "watch: limit or battery, ending the stage" >>"$LOGS/watch.log"
        kill -TERM "$stage_pid" 2>/dev/null
        sleep 3
        kill -KILL "$stage_pid" 2>/dev/null
        KIT_ROOT=$KIT_ROOT KIT_FAKE=$FAKE sh "$SELF" restore >>"$LOGS/watch.log" 2>&1
        exit 0
      fi
      sleep 2
    done
    for f in "$RUN"/*.pid; do
      [ -e "$f" ] && { KIT_ROOT=$KIT_ROOT KIT_FAKE=$FAKE sh "$SELF" restore >>"$LOGS/watch.log" 2>&1; break; }
    done
  ) </dev/null >/dev/null 2>&1 &
}

# begin <stage> <seconds>: log file, clean-up on every way out, the dead-man.
begin() {
  STAGE=$1; slimit=${KIT_LIMIT:-$2}
  say "=== stage $STAGE, limit $slimit s, battery $(battery)"
  # The trap runs when the shell ends, after the stage's redirection to its log is gone (2026-11-02: the first screen stage's log had
  # no "cleaning up" line for that reason alone): so it writes to the log itself.
  trap 'trap - EXIT INT TERM HUP; { say "cleaning up"; restore; [ -f "$RUN/night.on" ] && night_end; say "=== end of $STAGE"; } >>"${LOG:-/dev/null}" 2>&1' EXIT INT TERM HUP
  watch_stage "$slimit" $$
}

# The md5 list of what is the owner's, or the comparison with the list taken before. Report any difference at once.
guard_lists() {
  ( cd "$SD" && find Saves .userdata -type f ! -path ".userdata/*/logs/*" ! -path ".userdata/*/Tailscale/*" ! -name battery_logs.sqlite 2>/dev/null | sort | while IFS= read -r f; do md5sum "$f"; done ) >"$1" 2>/dev/null
}
stage_guard() {
  if [ ! -s "$GUARD/before.md5" ]; then
    guard_lists "$GUARD/before.md5"
    say "guard: list taken ($(wc -l <"$GUARD/before.md5") files); run guard again after a session to compare"
  else
    guard_lists "$GUARD/after.md5"
    if cmp -s "$GUARD/before.md5" "$GUARD/after.md5"; then ok "guard: Saves and .userdata are the same as before"
    else ok "guard: DIFFERENCE (stop and report)"; diff "$GUARD/before.md5" "$GUARD/after.md5" | head -20; fi
  fi
}

# trail: a line on the card (synced) every few seconds around a handover: NextUI, the SSH server, tailscaled, Wi-Fi. It runs detached,
# so when the SSH is gone and the device stays unreachable, the card says afterwards when the network went and what was alive then.
trail() {
  seconds=$1; tag=$2
  # One watcher at a time (the first run piled up one per stage); a new call only extends the time.
  echo $(( $(cut -d. -f1 /proc/uptime) + seconds )) >"$RUN/trail.until"
  [ -f "$RUN/trail.pid" ] && kill -0 "$(cat "$RUN/trail.pid")" 2>/dev/null && return 0
  (
    while [ "$(cut -d. -f1 /proc/uptime)" -lt "$(cat "$RUN/trail.until" 2>/dev/null || echo 0)" ]; do
      printf '%s %s up=%s nextui=%s dropbear=%s tailscaled=%s wlan0=%s %s\n' "$(date +%H:%M:%S)" "$tag" "$(cut -d' ' -f1 /proc/uptime)" \
        "$(pidof nextui.elf | tr ' ' ,)" "$(pidof dropbear | tr ' ' ,)" "$(pidof tailscaled | tr ' ' ,)" "$(cat /sys/class/net/wlan0/operstate 2>/dev/null)" \
        "$(ifconfig wlan0 2>/dev/null | sed -n 's/.*inet addr:\([0-9.]*\).*/\1/p')" >>"$LOGS/trail.log"
      sync
      # While a stage with the screen runs the trail is quiet (a game must not wait for forks and a sync every 3 s): one line a minute.
      if ls "$LOGS"/*.running >/dev/null 2>&1; then sleep 60; else sleep 3; fi
    done
  ) </dev/null >/dev/null 2>&1 &
  echo $! >"$RUN/trail.pid"
}

# A stage that needs the screen. Nothing here ends nextui.elf: a TERM to it makes NextUI power the Brick off (found 2026-11-02, twice).
# The stage is written to $KIT_ROOT/next-stage; the owner presses A on "pocket-ui" in NextUI's Tools list, NextUI starts the pak by its
# own loop (the pak is Tools/tg5040/pocket-ui.pak, our launch.sh), the pak reads and removes the file, runs "_run <stage>", and NextUI
# comes back by itself when it ends. Without a file the pak starts the real pocket-ui.
screen_stage() {
  name=$1
  if [ -n "$FAKE" ]; then "stage_$name"; return; fi
  [ "${KIT_OWNER_PRESENT:-}" = 1 ] || { say "screen stages run only with the owner next to the Brick: set KIT_OWNER_PRESENT=1"; exit 7; }
  battery_ok || exit 4
  playing && exit 5
  # The stage before may have just ended: NextUI's loop brings nextui.elf back within seconds, so wait for it (2026-11-02: a second
  # stage queued at once failed here with rc=6).
  waited=0
  while ! pidof nextui.elf >/dev/null 2>&1 && [ $waited -lt 90 ]; do sleep 1; waited=$((waited + 1)); done
  pidof nextui.elf >/dev/null 2>&1 || { say "nextui.elf is not running after 90 s: nothing to start the pak from, stopping"; exit 6; }
  [ -s "$GUARD/before.md5" ] || stage_guard
  trail 600 "stage-$name"
  printf '%s\n' "$name" >"$KIT_ROOT/next-stage"
  say ">>> OWNER: in NextUI open Tools and press A on \"pocket-ui\" (stage $name waits up to 5 min for that)"
  waited=0
  while [ -s "$KIT_ROOT/next-stage" ] && [ $waited -lt 300 ]; do sleep 1; waited=$((waited + 1)); done
  if [ -s "$KIT_ROOT/next-stage" ]; then rm -f "$KIT_ROOT/next-stage"; say "nobody started the pak in 5 min: the stage is cancelled"; exit 9; fi
  say "the pak took the stage; waiting for it to end"
  waited=0
  while [ -e "$LOGS/$name.running" ] && [ $waited -lt $((${KIT_LIMIT:-300} + 60)) ]; do sleep 1; waited=$((waited + 1)); done
  waited=0
  while ! pidof nextui.elf >/dev/null 2>&1 && [ $waited -lt 60 ]; do sleep 1; waited=$((waited + 1)); done
  if pidof nextui.elf >/dev/null 2>&1; then say "nextui.elf runs again"; else say "WARNING: nextui.elf is not back after 60 s: report to the owner"; fi
  tail -n 40 "$(ls -t "$LOGS/$name"-*.log 2>/dev/null | head -1)" 2>/dev/null
  stage_guard
}

# ---- Night sessions (the owner's permission, he sleeps, nobody looks at the screen, NOTHING may be heard) ---------------------------------
# nextui.elf is never ended (a TERM powers the Brick off). It is paused with SIGSTOP for the session and continued with SIGCONT, by the
# session itself and by a detached dead-man that does it at the limit whatever happens to the SSH.
# SOUND: the speaker and headphone switches of the audiocodec card stay OFF and the DAC volume at 0; no step of any session turns them on
# again (the owner does that, awake, in the morning; 2026-11-03: the "sound back" step of the first night ran a game out loud). A detached
# watcher (`mutewatch`) checks every 5 s, whatever runs, and forces them off; the dead-man of a session reads them first and ENDS EVERYTHING
# of ours if one reads on. Our own volume stand-in is not used in a night session. pocket-ui and pocket-core play to ALSA's "null" device
# (KIT_ALSA=default for a run that needs real underrun numbers, only with the switches proven off): numbers from null are no audio numbers.
# Input comes from pocket-ui's own command FIFO (/tmp/pocket-ui.in), never from the real pad. Needs KIT_NIGHT=1.
MIX="amixer -c 0 -q cset"
mix_get() { amixer -c 0 cget name="$1" 2>/dev/null | sed -n 's/.*: values=\(.*\)/\1/p' | head -1; }
mix_mute() { $MIX name='Headphone Switch' off >/dev/null 2>&1; $MIX name='HpSpeaker Switch' off >/dev/null 2>&1; $MIX name='DAC volume' 0,0 >/dev/null 2>&1; }
# 0 when every switch is off and the DAC volume is 0; says what it read.
mix_is_off() {
  hp=$(mix_get 'Headphone Switch'); sp=$(mix_get 'HpSpeaker Switch'); dv=$(mix_get 'DAC volume')
  [ "$hp" = off ] && [ "$sp" = off ] && [ "$dv" = 0,0 ]
}
mix_report() { echo "headphone=$(mix_get 'Headphone Switch') speaker=$(mix_get 'HpSpeaker Switch') dac=$(mix_get 'DAC volume')"; }

# The programs of ours that make a picture or a sound: pocket-ui, pocket-core, pocket-fb, brick-hw started from our folder.
ours() {
  for n in pocket-ui pocket-core pocket-fb brick-hw; do
    for p in $(pidof "$n" 2>/dev/null); do
      tr '\0' ' ' <"/proc/$p/cmdline" 2>/dev/null | grep -q "$KIT_ROOT/bin/" && echo "$p"
    done
  done
}
kill_ours() {
  for p in $(ours); do kill -TERM "$p" 2>/dev/null; done
  n=0; while [ $n -lt 15 ] && [ -n "$(ours)" ]; do sleep 0.3; n=$((n + 1)); done
  for p in $(ours); do kill -KILL "$p" 2>/dev/null; done
  rm -f "$WL" "$IPC"
}

# mutewatch: one detached watcher for the whole night; `mutewatch stop` ends it, `mutewatch` alone says whether it runs.
mutewatch() {
  case "${1:-}" in
    start)
      [ -f "$RUN/mutewatch.pid" ] && kill -0 "$(cat "$RUN/mutewatch.pid")" 2>/dev/null && { say "mutewatch runs ($(mix_report))"; return 0; }
      mix_mute
      ( while :; do
          if ! mix_is_off; then echo "$(date +%H:%M:%S) forced off (was $(mix_report))" >>"$LOGS/mute.log"; mix_mute; fi
          sleep 5
        done ) </dev/null >/dev/null 2>&1 &
      echo $! >"$RUN/mutewatch.pid"
      say "mutewatch started, pid $! ($(mix_report))" ;;
    stop) [ -f "$RUN/mutewatch.pid" ] && kill "$(cat "$RUN/mutewatch.pid")" 2>/dev/null; rm -f "$RUN/mutewatch.pid"; say "mutewatch stopped" ;;
    *) if [ -f "$RUN/mutewatch.pid" ] && kill -0 "$(cat "$RUN/mutewatch.pid")" 2>/dev/null; then say "mutewatch runs: $(mix_report)"; else say "mutewatch is NOT running: $(mix_report)"; fi ;;
  esac
}

# The governor: the pak path has NextUI's loop put the CPU on "performance" before a pak starts; a night session has no loop, so it does
# the same (the policy's files in sysfs, runtime only) and puts the three values back.
GOVDIR=/sys/devices/system/cpu/cpufreq/policy0
gov_save() { { echo "g=$(cat $GOVDIR/scaling_governor)"; echo "lo=$(cat $GOVDIR/scaling_min_freq)"; echo "hi=$(cat $GOVDIR/scaling_max_freq)"; } >"$RUN/gov.saved"; }
gov_perf() { top=$(tr ' ' '\n' <$GOVDIR/scaling_available_frequencies | grep -v '^$' | sort -n | tail -1); echo performance >$GOVDIR/scaling_governor; echo "$top" >$GOVDIR/scaling_max_freq; echo "$top" >$GOVDIR/scaling_min_freq; }
gov_restore() {
  [ -f "$RUN/gov.saved" ] || return 0
  . "$RUN/gov.saved"
  echo "$hi" >$GOVDIR/scaling_max_freq; echo "$lo" >$GOVDIR/scaling_min_freq; echo "$g" >$GOVDIR/scaling_governor
  rm -f "$RUN/gov.saved"
}

# The end of a session, in every way it can end: ours stopped and PROVEN gone, nextui.elf continued, the governor back. The sound is not
# touched here: it stays off.
night_end() {
  rm -f "$RUN/night.on"
  # The trail of this session too (2026-11-03: a night shell of the trail was still alive after the GPU test).
  [ -f "$RUN/trail.pid" ] && kill "$(cat "$RUN/trail.pid")" 2>/dev/null
  rm -f "$RUN/trail.pid" "$RUN/trail.until"
  kill_ours
  left=$(ours | tr '\n' ' ')
  if [ -n "$left" ]; then say "NIGHT END: programs of ours STILL ALIVE: $left"; else say "night: none of pocket-ui, pocket-core, pocket-fb, brick-hw of ours is alive"; fi
  [ -f "$RUN/night.pid" ] && kill -CONT "$(cat "$RUN/night.pid")" 2>/dev/null
  gov_restore
  say "night: nextui.elf continued, governor back, sound stays off ($(mix_report))"
}
night_session() {
  name=$1; limit=${KIT_LIMIT:-300}
  [ "${KIT_NIGHT:-}" = 1 ] || { say "night sessions need KIT_NIGHT=1"; exit 7; }
  battery_ok || exit 4
  playing && exit 5
  np=$(pidof nextui.elf) || { say "nextui.elf is not running: stopping"; exit 6; }
  [ -f "$RUN/night.on" ] && { say "a night session is already on"; exit 8; }
  [ -n "$(ours)" ] && { say "programs of ours are alive already ($(ours | tr '\n' ' ')): not starting"; exit 8; }
  mutewatch start
  mix_is_off || { say "ABORT: the sound is not off ($(mix_report)): nothing was started"; exit 9; }
  [ -s "$GUARD/before.md5" ] || stage_guard
  echo "$np" >"$RUN/night.pid"; : >"$RUN/night.on"
  [ "${KIT_GOV:-performance}" = performance ] && { gov_save; gov_perf; }
  trail $((limit + 120)) "night-$name"
  # The dead-man, every second: a switch that reads ON ends everything of ours and the session (no retry); at the limit, or when the
  # session's marker is gone, it continues nextui.elf. It never turns the sound on.
  (
    n=0
    while [ -f "$RUN/night.on" ] && [ $n -lt $((limit + 20)) ]; do
      sleep 1; n=$((n + 1))
      if [ -f "$RUN/night.on" ] && ! mix_is_off; then
        echo "$(date +%H:%M:%S) ABORT: sound switch read on ($(mix_report))" >>"$LOGS/mute.log"
        : >"$RUN/night.abort"; mix_mute; kill_ours; break
      fi
    done
    kill_ours; kill -CONT "$np" 2>/dev/null; gov_restore; rm -f "$RUN/night.on"
  ) </dev/null >/dev/null 2>&1 &
  say "night: sound off ($(mix_report)), pausing nextui.elf $np for at most $limit s, audio device ${KIT_ALSA:-null}"
  kill -STOP "$np"
  trap 'trap - EXIT INT TERM HUP; night_end' EXIT INT TERM HUP
  "stage_$name"
  trap - EXIT INT TERM HUP
  night_end
  for _ in 1 2 3 4 5; do pidof nextui.elf >/dev/null 2>&1 && break; sleep 1; done
  say "nextui.elf state: $(sed -n 's/^State:[ \t]*//p' "/proc/$np/status" 2>/dev/null | head -1)"
  [ -f "$RUN/night.abort" ] && { say "SESSION WAS ABORTED: a sound switch read on. Report; no retry."; rm -f "$RUN/night.abort"; }
  stage_guard
}

# live: the real pocket-ui as the pak starts it, for a night session that is driven from outside: `fifo` sends commands to its FIFO, `shot`
# reads the screen back, `stop` ends it. The paint times, the sampler and the PlayStation game (POCKET_PS1) are on.
stage_live() {
  begin live ${KIT_LIMIT:-540}
  rm -f "$RUN/live.stop"
  before=$(wc -l <"$LOGS/pocket-ui.log" 2>/dev/null || echo 0)
  sampler /tmp/pocket-perf.csv ${KIT_LIMIT:-540} &
  smp=$!
  POCKET_CORE_ALSA_DEVICE=${KIT_ALSA:-null} POCKETUI_FRAME_LOG=1 POCKET_MIN_BATTERY=$MIN_BATTERY POCKET_PS1="${POCKET_PS1:-Crash Bandicoot (Europe, Australia)}" sh "$KIT_ROOT/start.sh" nextui &
  up=$!
  n=0
  while kill -0 "$up" 2>/dev/null && [ ! -f "$RUN/live.stop" ] && [ ! -f "$RUN/night.abort" ] && [ $n -lt ${KIT_LIMIT:-540} ]; do sleep 1; n=$((n + 1)); done
  kill -TERM "$up" 2>/dev/null; sleep 4; kill "$smp" 2>/dev/null
  cp /tmp/pocket-perf.csv "$LOGS/perf-live-$(date +%H%M%S).csv" 2>/dev/null
  tail -n +$((before + 1)) "$LOGS/pocket-ui.log" > "$LOGS/live-ui-$(date +%H%M%S).log"
  ok "live ended after $n s"
}

# proof: the 10 s run that proves the sound is off: the real pocket-ui on the null device for 10 s, the switches read at the start,
# every 2 s during, and after. Nothing is played (no game).
stage_proof() {
  begin proof 60
  say "at start: $(mix_report)"
  POCKET_CORE_ALSA_DEVICE=${KIT_ALSA:-null} POCKET_MIN_BATTERY=$MIN_BATTERY sh "$KIT_ROOT/start.sh" nextui &
  up=$!
  n=0
  while [ $n -lt 10 ]; do
    sleep 2; n=$((n + 2))
    say "during ($n s): $(mix_report), ours: $(ours | tr '\n' ' ')"
  done
  kill -TERM "$up" 2>/dev/null; sleep 4
  say "after: $(mix_report), ours: $(ours | tr '\n' ' ')"
  readback proof
  ok "proof done"
}

# hold: a short session: nothing runs, the screen is read back, then everything is continued.
stage_hold() {
  begin hold 60
  sleep 8
  readback hold
  ok "hold done"
}

start_bridge() {
  rm -f "$WL" "$IPC"
  # shellcheck disable=SC2086
  start_kept fb "$B/pocket-fb" --wayland "$WL" --ipc "$IPC" $FB_ARGS
  i=0
  while [ $i -lt 50 ]; do [ -S "$WL" ] && [ -S "$IPC" ] && return 0; sleep 0.1; i=$((i + 1)); done
  say "the bridge's sockets never came up"; return 1
}

sample() {
  say "--- $1"
  free -m | sed -n 1,3p
  [ -f "$RUN/fb.pid" ] && pid=$(cut -d' ' -f1 "$RUN/fb.pid") && grep -E 'VmRSS' "/proc/$pid/status" 2>/dev/null | sed 's/^/pocket-fb /'
  uptime
}

stage_read() {
  begin read 120
  r() { say "\$ $*"; "$@" 2>&1 | head -"${LINES_MAX:-60}"; }
  r uname -a
  r /lib/libc.so.6
  r cat /proc/mounts
  r df -h / /tmp "$SD"
  r free -m
  for f in /sys/class/graphics/fb0/*; do [ -f "$f" ] && [ -r "$f" ] && printf '%s: %s\n' "$f" "$(head -c 200 "$f" 2>/dev/null | tr '\n' ' ')"; done
  r ls -la /dev/fb0 /dev/dri /dev/disp /dev/input /dev/snd
  r cat /proc/bus/input/devices
  r cat /proc/asound/cards
  r cat /etc/asound.conf
  r sh -c "ps | grep -v ' \\['"
  for f in /sys/class/power_supply/*/uevent; do r cat "$f"; done
  r cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor
  r cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_available_frequencies
  r ls -la "$CORES"
  r strings /usr/trimui/bin/MainUI
  ok "read done"
}

stage_binary() {
  begin binary 180
  r() { say "\$ $*"; "$@" 2>&1 | head -40; }
  for f in pocket-ui pocket-core pocket-fb; do r /lib/ld-linux-aarch64.so.1 --list "$B/$f"; done
  r "$B/pocket-core" --version
  r limit 20 "$B/pocket-ui" --version
  head -c $((WIDTH * HEIGHT * 4)) /dev/zero >"$RUN/test.fb"
  rm -f "$WL" "$IPC"
  start_kept fb "$B/pocket-fb" --wayland "$WL" --ipc "$IPC" --fb "$RUN/test.fb" --mode "${WIDTH}x${HEIGHT}x32"
  i=0
  while [ $i -lt 50 ] && [ ! -S "$WL" ]; do sleep 0.1; i=$((i + 1)); done
  t0=$(up)
  POCKETUI_MOCK=1 POCKETUI_LANG=en POCKETUI_SNAPSHOT_SIZE=${WIDTH}x$HEIGHT limit 60 "$B/pocket-ui" --snapshot settings "$RUN/settings.png" 2>&1 | tail -3
  t1=$(up)
  size=$(wc -c <"$RUN/settings.png" 2>/dev/null)
  say "snapshot $size bytes in $(((t1 - t0) * 10)) ms"
  cp -f "$RUN/settings.png" "$KIT_ROOT/settings.png" 2>/dev/null
  if [ "${size:-0}" -gt 0 ]; then ok "binary ok, snapshot in $(((t1 - t0) * 10)) ms"; else ok "binary FAILED, no picture"; fi
  rm -f "$RUN/test.fb"
}

# rom: one official game per system, picked by name and copied into OUR folder (the owner's library is only read; saves of
# these copies stay in our folder).
stage_rom() {
  for spec in "gba|F-Zero - Maximum Velocity (USA, Europe).gba" "snes|F-Zero (USA).sfc"; do
    sys=${spec%%|*}; name=${spec#*|}
    mkdir -p "$KIT_ROOT/storage/roms/$sys"
    [ -f "$KIT_ROOT/storage/roms/$sys/$name" ] && { say "storage/roms/$sys has $name already"; continue; }
    f=$(find "$SD/Roms" -name "$name" -size -40960k 2>/dev/null | head -1)
    if [ -n "$f" ]; then cp "$f" "$KIT_ROOT/storage/roms/$sys/" && say "copied $name to storage/roms/$sys"; else say "no $name found"; fi
  done
  # A PlayStation BIOS, read only from the owner's Bios, into OUR bios folder (the core finds it in the system directory).
  mkdir -p "$KIT_ROOT/storage/roms/bios"
  for b in scph5501.bin scph5502.bin scph1001.bin; do
    f="$SD/Bios/PS/$b"; [ -f "$f" ] && [ ! -f "$KIT_ROOT/storage/roms/bios/$b" ] && cp "$f" "$KIT_ROOT/storage/roms/bios/" && say "copied BIOS $b"
  done
  ok "rom done: $(ls "$KIT_ROOT/storage/roms/gba" "$KIT_ROOT/storage/roms/snes" 2>/dev/null | tr '\n' ' ')"
}

stage_snapshot() {
  begin snapshot 60
  card=$KIT_ROOT/testcard.png
  [ -f "$card" ] || card=$KIT_ROOT/settings.png
  [ -f "$card" ] || { ok "snapshot SKIPPED: no picture, run binary first"; return; }
  # shellcheck disable=SC2086
  limit 20 "$B/pocket-fb" --picture "$card" --hold 5 $FB_ARGS &
  sp=$!
  sleep 2
  readback snapshot
  wait "$sp"
  ok "snapshot exit $?: ask the owner what the screen showed (upright? colours? tearing? which page, left or right?)"
}

stage_input() {
  begin input 60
  say ">>> PRESS EVERY BUTTON NOW, for 30 s: d-pad, A B X Y, L R, L2 R2, Start, Select, Menu"
  decode() {
    # one 24-byte event: type 2 bytes at 16, code 2 bytes at 18, value 4 bytes at 20, all little endian
    while :; do
      h=$(dd if="$1" bs=24 count=1 2>/dev/null | xxd -p -c 24)
      [ -z "$h" ] && break
      t=$(echo "$h" | cut -c35-36)$(echo "$h" | cut -c33-34)
      c=$(echo "$h" | cut -c39-40)$(echo "$h" | cut -c37-38)
      v=$(echo "$h" | cut -c47-48)$(echo "$h" | cut -c45-46)$(echo "$h" | cut -c43-44)$(echo "$h" | cut -c41-42)
      case "$t" in 0001|0003) printf '%s type %d code %d value %d\n' "$(basename "$1")" "0x$t" "0x$c" "0x$v" ;; esac
    done
  }
  for dev in /dev/input/event*; do
    say "listening on $dev: $(cat "/sys/class/input/$(basename "$dev")/device/name" 2>/dev/null)"
    decode "$dev" &
    echo "$!" >>"$RUN/input.pids"
  done
  sleep 30
  for p in $(cat "$RUN/input.pids"); do kill "$p" 2>/dev/null; done
  rm -f "$RUN/input.pids"
  ok "input done: the codes above are what pocket-ui reads (type 1 = key; X 308 and Y 307, A 305, B 304, Menu 316 expected; type 3 = hat/axis)"
}

# sampler <file> <seconds>: every 2 s one line in a file in /tmp: governor, clock, and the CPU ticks each of our programs used in that
# time (user + system, 1 tick = 10 ms), plus the load average. It answers "is the UI busy while a game runs?" and "what clock does the
# game get?" with numbers (2026-11-02: SNES ran at 73 % of the top clock, 63 underruns in 29 s).
sampler() {
  out=$1; total=$2; n=0; : >"$out"
  ticks() { for p in $(pidof "$1" 2>/dev/null); do cut -d' ' -f14,15 "/proc/$p/stat" 2>/dev/null; done | awk '{a+=$1+$2} END {print a+0}'; }
  pu=$(ticks pocket-ui); pc=$(ticks pocket-core); pf=$(ticks pocket-fb)
  while [ $n -lt "$total" ]; do
    sleep 2; n=$((n + 2))
    u=$(ticks pocket-ui); c=$(ticks pocket-core); f=$(ticks pocket-fb)
    printf '%s gov=%s clock=%s ui=%s core=%s fb=%s load=%s\n' "$(date +%H:%M:%S)" "$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)" \
      "$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq)" $((u - pu)) $((c - pc)) $((f - pf)) "$(cut -d' ' -f1 /proc/loadavg)" >>"$out"
    pu=$u; pc=$c; pf=$f
  done
}

# ui: the real pocket-ui with the real shelf (our test copies in storage/roms), as the pak starts it, with the paint times of the
# animations (POCKETUI_FRAME_LOG) and the sampler above. 5 minutes: the shelf, a game (resume from its saved moment), the pause menu,
# Quit, another game. Hold SELECT + START for "Back to NextUI" earlier.
stage_ui() {
  begin ui 330
  before=$(wc -l <"$LOGS/pocket-ui.log" 2>/dev/null || echo 0)
  sampler /tmp/pocket-perf.csv 300 &
  sp=$!
  sample "before the UI"
  POCKETUI_FRAME_LOG=1 POCKET_MIN_BATTERY=$MIN_BATTERY limit 300 sh "$KIT_ROOT/start.sh" nextui
  kill "$sp" 2>/dev/null
  cp /tmp/pocket-perf.csv "$LOGS/perf-$(date +%H%M%S).csv" 2>/dev/null
  sample "after the UI"
  tail -n +$((before + 1)) "$LOGS/pocket-ui.log" | grep -E "^motion:|^cabinet:|panic|device:|library:|audio|underruns|launch|menu shot|pause menu" | tail -80
  grep -E 'pocket-fb:' "$LOGS/pocket-fb.log" | tail -3
  ok "ui done: paint times (motion:), underruns per second of play and the sampler (logs/perf-*.csv) above"
}

# viewcheck: no screen. What the programs see in the view: the time zone, the stand-ins for volume, brightness and the battery.
stage_viewcheck() {
  begin viewcheck 120
  view_on
  say "--- date in the view, and where /etc/localtime leads"
  chroot /tmp/pocket-view sh -c 'date; echo TZ=$TZ; ls -l /etc/localtime; ls -l /tmp/localtime' 2>&1
  say "--- date outside"
  date
  hw=/tmp/pocket-hw-check; rm -rf "$hw"; mkdir -p "$hw/usr/bin"
  for f in "$KIT_ROOT"/hwroot/usr/bin/*; do ln -s "$f" "$hw/usr/bin/$(basename "$f")"; done
  say "--- the stand-ins, read only (no change)"
  BRICK_STATE=/tmp/pocket-hw-check "$hw/usr/bin/pactl" get-sink-volume
  BRICK_STATE=/tmp/pocket-hw-check "$hw/usr/bin/pactl" get-sink-mute
  say "brightness through the display driver: $("$B/brick-hw" bl-get 2>&1)"
  rm -rf "$hw"
  ok "viewcheck done"
}

# game <core file> <game copy> <system> <seconds>: NextUI's core by path (read only), our copy of a game.
game() {
  core=$CORES/$1; rom=$2; system=$3; seconds=$4
  [ -f "$core" ] || { say "no core $core"; return 1; }
  [ -f "$rom" ] || { say "no game $rom (run the rom stage)"; return 1; }
  say "--- $1 with $(basename "$rom"), CPU copy, $seconds s"
  if [ "${POCKET_VIEW:-1}" = 0 ]; then limit $((seconds + 20)) "$B/pocket-core" "$core" "$rom" --system "$system" --bench "$seconds" >"$RUN/game.out" 2>&1
  else limit $((seconds + 20)) chroot /tmp/pocket-view "$B/pocket-core" "$core" "$rom" --system "$system" --bench "$seconds" >"$RUN/game.out" 2>&1; fi
  say "exit $?"
  grep -E 'frames|emulated|audio|first frame|display|panic|no audio|error' "$RUN/game.out" | tail -12
}
first_rom() { ls "$KIT_ROOT/storage/roms/$1/" 2>/dev/null | head -1; }

stage_game() {
  begin game 150
  start_bridge || return
  view_on
  r=$(first_rom gba); [ -n "$r" ] && game gpsp_libretro.so "$KIT_ROOT/storage/roms/gba/$r" gba 30
  sample "after GBA"
  r=$(first_rom snes); [ -n "$r" ] && game snes9x_libretro.so "$KIT_ROOT/storage/roms/snes/$r" snes 20
  stop_saved
  grep -E 'pocket-fb:' "$LOGS/fb.out" | tail -2
  ok "game done: fps, frame ms, late frames, underruns above; ask the owner about picture and sound"
}

# soak: the audio question (snes9x: 5 underruns in one second at 15.7 s, core max 35 ms). Interleaved runs of the same game, so a
# drift in the device cannot pose as a cause: A the defaults, B a bigger ALSA buffer (POCKET_CORE_ALSA_US), C the same as A again. Per
# run: late frames, underruns and their times from pocket-core's own summary; the trail is quiet meanwhile (one line a minute).
stage_soak() {
  begin soak 600
  start_bridge || return
  view_on
  r=$(first_rom snes); [ -n "$r" ] || { ok "soak SKIPPED: no copy of a game (run the rom stage)"; return; }
  n=1
  for round in "A defaults" "B alsa 60 ms" "C defaults" "A defaults" "B alsa 60 ms" "C defaults"; do
    battery_ok || break
    case "$round" in B*) export POCKET_CORE_ALSA_US=60000 ;; *) unset POCKET_CORE_ALSA_US ;; esac
    say "=== run $n: $round, 60 s"
    game snes9x_libretro.so "$KIT_ROOT/storage/roms/snes/$r" snes 60 | grep -E "frames|audio latency|underruns|^$"
    n=$((n + 1))
  done
  unset POCKET_CORE_ALSA_US
  stop_saved
  ok "soak done: compare 'late' and 'underruns' per run above (A and C are the same setting)"
}

# core_cmd <word>: a command to the running pocket-core through its FIFO. Opened without waiting, as pocket-ui does: a game that
# has gone must not hang the stage.
core_cmd() { ( echo "$1" >/tmp/pocket-core.cmd ) & cp=$!; sleep 0.3; kill "$cp" 2>/dev/null; }

stage_pause() {
  begin pause 150
  start_bridge || return
  view_on
  r=$(first_rom gba); [ -n "$r" ] || { ok "pause SKIPPED: no copy of a game in rom/gba (run the rom stage)"; return; }
  start_view game "$B/pocket-core" "$CORES/gpsp_libretro.so" "$KIT_ROOT/storage/roms/gba/$r" --system gba
  nap 3
  POCKETUI_MOCK=1 POCKETUI_LANG=en start_view ui "$B/pocket-ui"
  nap 4
  sample "game and UI both up"
  good=0
  for i in 1 2 3 4 5 6 7 8 9 10; do
    battery_ok || break
    round=1
    "$B/pocket-fb" --ipc-send "$IPC" '[app_id="pocket-ui"] focus' >/dev/null || round=0
    core_cmd pause
    nap 1
    "$B/pocket-fb" --ipc-send "$IPC" '[app_id="^(?!pocket-ui$).*"] focus' >/dev/null || round=0
    core_cmd resume
    nap 1
    say "round $i: $([ $round = 1 ] && echo ok || echo FAILED)"
    [ $round = 1 ] && good=$((good + 1))
  done
  sample "after the rounds"
  stop_saved
  ok "pause $good/10: the screen should have changed every second, ask the owner"
}

stage_pak() {
  begin pak 90
  POCKET_MIN_BATTERY=$MIN_BATTERY limit 60 sh "$KIT_ROOT/start.sh" nextui
  ok "pak: start.sh ran 60 s or ended by itself; its log is $KIT_ROOT/logs/start.log"
}

stage_egl() {
  begin egl 120
  [ -z "$FAKE" ] && say "fb before: pan $(cat /sys/class/graphics/fb0/pan 2>/dev/null)"
  windows="null fbdev"
  [ -n "$FAKE" ] && windows=pbuffer
  best=
  for w in $windows; do
    say "--- window kind: $w"
    ( sleep 8; readback "egl-$w" ) &
    limit 40 "$B/brick-egl" --window "$w" --seconds 20 --size "${WIDTH}x$HEIGHT"
    code=$?
    say "$w: exit $code (0 fine, 3 GLES 2 only, 124 hung, 139 crashed)"
    { [ "$code" = 0 ] || [ "$code" = 3 ]; } && [ -z "$best" ] && best=$w
  done
  if [ -n "$best" ]; then ok "egl works with window kind $best: read GL_VERSION and frames above"; else ok "egl FAILED on every window kind"; fi
}

# gltest: what the owner should see is in docs/TRIMUI-BRICK.md ("The GPU"): red top left, green top right, blue bottom left, white bottom
# right, a white bar sweeping, an orange triangle turning, a yellow bar filling along the bottom; the last 4 s the background is violet.
stage_gltest() {
  begin gltest 60
  [ -z "$FAKE" ] && say "fb before: pan $(cat /sys/class/graphics/fb0/pan 2>/dev/null)"
  if [ -n "$FAKE" ]; then limit 40 "$B/brick-egl" --panel --seconds "${GLTEST_SECONDS:-20}" --window pbuffer --size "${WIDTH}x$HEIGHT" --readback "$KIT_ROOT/readback-gltest.raw"
  else limit 40 "$B/brick-egl" --panel --seconds 20 --window null --size "${WIDTH}x$HEIGHT" --fb "$FB" --readback "$KIT_ROOT/readback-gltest.raw"; fi
  code=$?
  [ -z "$FAKE" ] && say "fb after: pan $(cat /sys/class/graphics/fb0/pan 2>/dev/null)"
  ok "gltest exit $code (0 fine, 3 GLES 2 only, 21 the visible page differs, 137/143 ended at the limit): ask the owner what the panel showed"
}

# press <word> <seconds>: one button through pocket-ui's FIFO. Opened without waiting: a UI that has gone must not hang the stage.
press() { [ -p /tmp/pocket-ui.in ] || return 0; ( echo "$1" >/tmp/pocket-ui.in ) & pp=$!; sleep "$2"; kill "$pp" 2>/dev/null; }
# The same presses for both ways, so their numbers compare: a sweep along the shelf, a shelf opened and left, Settings and a list in it.
presses() {
  for i in 1 2 3 4 5 6 7 8; do press right 0.5; done
  for i in 1 2 3 4 5 6; do press right 0.12; done
  for i in 1 2 3; do press a 1; press down 0.4; press b 1; done
  press start 1.5; press down 0.5; press a 1; press down 0.4; press b 0.8; press b 1
}
# bench_ui <copy|gl>: pocket-ui on mock hardware on the real screen, with the presses above; the numbers of pocket-fb and of the animations.
bench_ui() {
  way=$1; flag=; [ "$way" = gl ] && flag="--gl"
  [ -n "$FAKE" ] && [ "$way" = gl ] && flag="--gl --gl-window pbuffer"
  say "--- $way, 20 s of presses"
  rm -f "$WL" "$IPC" /tmp/pocket-ui.in
  # shellcheck disable=SC2086
  "$B/pocket-fb" --wayland "$WL" --ipc "$IPC" $FB_ARGS $flag >"$RUN/fb-$way.out" 2>&1 &
  fp=$!
  printf '%s pocket-fb\n' "$fp" >"$RUN/fb.pid"
  i=0
  while [ $i -lt 50 ] && { [ ! -S "$WL" ] || [ ! -S "$IPC" ]; }; do sleep 0.1; i=$((i + 1)); done
  rm -rf "$RUN/lib"; [ -f "$KIT_ROOT/fakelib.tar" ] && tar -xf "$KIT_ROOT/fakelib.tar" -C "$RUN"
  # An ALSA configuration that knows only the null device: no sound from a test; POCKETUI_MOCK: no change to the system from one.
  printf 'pcm.!default { type null }\npcm.null { type null }\nctl.!default { type hw card 0 }\n' >"$RUN/no-sound.conf"
  # Inside the view like every pocket-ui here: what it may write to /storage lands in our folder, not on the system's root.
  view_on
  run_ui=; [ "${POCKET_VIEW:-1}" = 0 ] || run_ui="chroot /tmp/pocket-view"
  # shellcheck disable=SC2086
  ALSA_CONFIG_PATH=$RUN/no-sound.conf POCKETUI_MOCK=1 POCKETUI_LANG=en POCKETUI_FRAME_LOG=1 POCKETUI_NO_HISTORY=1 POCKETUI_NO_AUTOSUSPEND=1 POCKETUI_DATA=$RUN/lib/data POCKETUI_ES_SYSTEMS=$RUN/lib/es_systems.cfg \
    $run_ui "$B/pocket-ui" >"$RUN/ui-$way.out" 2>&1 &
  printf '%s pocket-ui\n' "$!" >"$RUN/ui.pid"
  sleep 4
  i=0
  while [ $i -lt 60 ] && [ ! -p /tmp/pocket-ui.in ]; do sleep 0.1; i=$((i + 1)); done
  ( sleep 3; readback "glui-$way"; display_state "glui-$way" ) &
  presses
  stop_saved
  say "$way: $(grep -c 'press to frame' "$RUN/ui-$way.out") presses reached the UI"
  grep -E '^motion: [0-9]+ frames|panicked at' "$RUN/ui-$way.out" | tail -20
  grep -E 'pocket-fb: [0-9]+ frames|while moving|GL |gl: config|gl: GL_RENDERER|pocket-fb: gpu ' "$RUN/fb-$way.out"
  grep -E '^brick:' "$RUN/ui-$way.out"
}

stage_glui() {
  begin glui 220
  # On the card, so the numbers are there whatever happens to the device afterwards.
  bench_ui copy
  battery_ok || return
  bench_ui gl
  rm -rf "$RUN/lib"
  view_off
  battery_ok || return
  if [ -n "$FAKE" ]; then ok "glui fake run done"; return; fi
  if [ "${KIT_NIGHT:-}" = 1 ]; then
    # Nobody here to press: a short run of the real UI through the GPU, no game started (no A), its picture read back.
    say ">>> 30 s of the real pocket-ui through the GPU, presses from its FIFO"
    # ALSA's null device twice over: by name, and as the only device the configuration knows. Nothing of this run reaches a speaker.
    printf 'pcm.!default { type null }\npcm.null { type null }\nctl.!default { type hw card 0 }\n' >"$RUN/no-sound.conf"
    ALSA_CONFIG_PATH=$RUN/no-sound.conf POCKET_CORE_ALSA_DEVICE=${KIT_ALSA:-null} POCKET_BRICK_GL=1 POCKETUI_FRAME_LOG=1 POCKET_MIN_BATTERY=$MIN_BATTERY limit 30 sh "$KIT_ROOT/start.sh" nextui &
    rp=$!
    sleep 8
    readback glui-real
    display_state glui-real
    for i in 1 2 3 4 5 6; do press right 0.5; done
    sleep 2
    display_state glui-real-moved
    wait "$rp"
    grep -E 'GL' "$LOGS/start.log" | tail -3
    tail -n 40 "$LOGS/pocket-fb.log" | grep -E 'pocket-fb: [0-9]+ frames|while moving|GL |pocket-fb: gpu '
    tail -n 200 "$LOGS/pocket-ui.log" | grep -E '^brick:|^motion: [0-9]+ frames' | tail -12
    ok "glui night done: compare 'while moving' for copy and gl above; what the panel looked like only eyes can say"
    return
  fi
  say ">>> 90 s of the real pocket-ui through the GPU: press around, shelf, Settings, back; SELECT + START held ends it earlier"
  POCKET_BRICK_GL=1 POCKETUI_FRAME_LOG=1 POCKET_MIN_BATTERY=$MIN_BATTERY limit 90 sh "$KIT_ROOT/start.sh" nextui
  grep -E 'GL' "$LOGS/start.log" | tail -3
  tail -n 12 "$LOGS/pocket-fb.log" | grep -E 'pocket-fb: [0-9]+ frames|while moving|GL '
  ok "glui done: compare 'while moving' for copy and gl above; ask the owner whether the UI felt quicker, tore or flickered"
}

# display_state <name>: what the display engine scans out and the backlight, from the driver, no pair of eyes needed.
display_state() {
  [ -n "$FAKE" ] && return 0
  say "display $1: pan $(cat /sys/class/graphics/fb0/pan 2>/dev/null)"
  grep -E 'backlight|enable +ch' /sys/class/disp/disp/attr/sys 2>/dev/null | cut -c1-200
}

# glplay: the real pocket-ui as the pak starts it, but through the GPU (page transitions and window crossfades on the GPU, each with its line
# in pocket-fb's log), the paint times of every animation (POCKETUI_FRAME_LOG), F-Zero from our copies and one PlayStation game of the owner's,
# read only. NOTHING is heard: the switches are forced off (mutewatch) and proven off before anything starts, and every program plays to
# ALSA's null device. The owner ends it with SELECT + START held ("Back to NextUI"); at the latest it ends after 8 minutes.
stage_glplay() {
  begin glplay ${KIT_LIMIT:-480}
  mutewatch start
  mix_is_off || { ok "glplay STOPPED before anything started: the sound is not off ($(mix_report))"; return; }
  pan0=$(cat /sys/class/graphics/fb0/pan 2>/dev/null)
  ui0=$(wc -l <"$LOGS/pocket-ui.log" 2>/dev/null || echo 0); fb0=$(wc -l <"$LOGS/pocket-fb.log" 2>/dev/null || echo 0)
  say "fb0 pan $pan0, $(mix_report)"
  printf 'pcm.!default { type null }\npcm.null { type null }\nctl.!default { type hw card 0 }\n' >"$RUN/no-sound.conf"
  ( sleep 15; readback glplay; display_state glplay ) &
  ALSA_CONFIG_PATH=$RUN/no-sound.conf POCKET_CORE_ALSA_DEVICE=${KIT_ALSA:-null} POCKET_BRICK_GL=1 POCKETUI_FRAME_LOG=1 POCKET_MIN_BATTERY=$MIN_BATTERY \
    POCKET_PS1="${POCKET_PS1:-Crash Bandicoot (Europe, Australia)}" sh "$KIT_ROOT/start.sh" nextui
  say "fb0 pan $(cat /sys/class/graphics/fb0/pan 2>/dev/null) (was $pan0), $(mix_report)"
  tail -n +$((fb0 + 1)) "$LOGS/pocket-fb.log" >"$LOGS/glplay-fb-$(date +%H%M%S).log"
  tail -n +$((ui0 + 1)) "$LOGS/pocket-ui.log" >"$LOGS/glplay-ui-$(date +%H%M%S).log"
  tail -n +$((fb0 + 1)) "$LOGS/pocket-fb.log" | grep -E 'GL |pocket-fb: gpu |pocket-fb: [0-9]+ frames|while moving' | tail -40
  tail -n +$((ui0 + 1)) "$LOGS/pocket-ui.log" | grep -E '^brick:|^motion: [0-9]+ frames|^cabinet:|load|first frame' | tail -60
  grep -E 'GL' "$LOGS/start.log" | tail -3
  ok "glplay done"
}

# readback <name>: the visible fb0 page as a raw file in our folder (what was written; not what the panel shows).
readback() {
  [ -n "$FAKE" ] && return 0
  pan=$(cut -d, -f2 /sys/class/graphics/fb0/pan 2>/dev/null); stride=$(cat /sys/class/graphics/fb0/stride 2>/dev/null)
  say "readback $1: pan $pan stride $stride"
  dd if=/dev/fb0 bs="$stride" skip="${pan:-0}" count="$HEIGHT" of="$KIT_ROOT/readback-$1.raw" 2>/dev/null
  say "readback $1: $(wc -c <"$KIT_ROOT/readback-$1.raw") bytes"
}

# A stage with a log file and its output on the screen of whoever called it.
logged() { LOG=$LOGS/$1-$(date +%Y%m%d-%H%M%S).log; "$2" "$1" 2>&1 | tee -a "$LOG"; }
run_stage() { "stage_$1"; }

case "${1:-}" in
  guard) stage_guard ;;
  read|binary|rom|viewcheck) logged "$1" run_stage ;;
  fifo) shift; if [ -f "$RUN/night.on" ] && [ -n "$(pidof pocket-ui)" ] && [ ! -f "$RUN/night.abort" ]; then for c in "$@"; do [ -f "$RUN/night.on" ] || break; echo "$c" >/tmp/pocket-ui.in; sleep 1; done; else say "no session or no pocket-ui: nothing sent"; fi ;;
  mutewatch) shift; mutewatch "$@" ;;
  ours) echo "ours: $(ours | tr '\n' ' ')" ;;
  shot) readback "${2:-shot}" ;;
  stop) : >"$RUN/live.stop" ;;
  night) shift; NLOG=$LOGS/night-$1-$(date +%Y%m%d-%H%M%S).log; night_session "$@" 2>&1 | tee -a "$NLOG" ;;
  snapshot|input|ui|game|pause|pak|egl|soak|gltest|glui|glplay) screen_stage "$1" ;;
  # NextUI's loop calling back: all output goes to the log, the caller reads it afterwards
  _run) case "$2" in snapshot|input|ui|game|pause|pak|egl|soak|gltest|glui|glplay) ;; *) echo "unknown stage $2"; exit 2 ;; esac
    NEED_BATTERY=1; : >"$LOGS/$2.running"; LOG=$LOGS/$2-$(date +%Y%m%d-%H%M%S).log
    battery_ok >>"$LOG" 2>&1 || { rm -f "$LOGS/$2.running"; exit 4; }
    { "stage_$2"; } >>"$LOG" 2>&1; rm -f "$LOGS/$2.running" ;;
  restore) restore; say "restored" ;;
  *) sed -n 2,28p "$0" | cut -c3-; exit 2 ;;
esac
