#!/bin/sh
# Measures big standalone emulators (PS2: ARMSX2, GameCube and Wii: Dolphin, 3DS: Azahar) on the Mini V2 and the Odin 2 Mini:
# does a game run at full speed?
#   scripts/big-emu-bench.sh [options] ROM[@FPS]...
#     ROM         a game ON THE DEVICE, e.g. /storage/roms/wii/<game>.rvz or /storage/roms/3ds/<game>.3ds. The folder
#                 (ps2, gamecube, wii, 3ds) tells the system. Nothing is copied or kept here; the games stay the owner's.
#     @FPS        the game's own frame rate, 30 or 50 (default 60), when it is not a 60 fps game
#   --minutes M   measured time per game after the warm-up (default 3, at most 5 so one device session stays under 10 min)
#   --warmup S    seconds between the first picture and the measurement (default 20: boot, shader compiles)
#   --dim PCT     screen brightness while it runs (default 5 % of the maximum; a screen that is dark stays dark)
#   --rumble      leave the controller rumble as the player has it (default: OFF during the run, in the emulators' config,
#                 which is put back afterwards: ARMSX2 Pad1 motor scales 0, Dolphin's GC pad rumble unmapped)
#   --out DIR     where the raw runs and report.txt go (default /tmp/big-emu-bench.XXXXXX)
#   --report DIR..  only evaluate earlier run folders (no device)
# Env: POCKETUI_HOST (scripts/local.env) the device; BIGEMU_LOCK the device lock (default /tmp/pocket-device.flock, one flock
#      per game, so never more than one session of <10 min; "none" for no lock); BIGEMU_SYSTEM to force ps2, gamecube, wii or 3ds.
#      BIGEMU_LOCAL=1 runs the device half on THIS machine (the dry run of scripts/test-big-emu-bench.sh; BIG_* variables
#      point it at fake /sys, runemu.sh, ...).
#
# Per game, on the device (one SSH session, under the device lock):
#   start       /usr/bin/runemu.sh <rom> -P<system> --core=<core> --emulator=<emu> --controllers= from /tmp, the call of
#               spawn_game in src/main.rs, with deploy/frametime.so preloaded (every presented frame is stamped);
#               the sink muted and the screen dimmed, both put back at the end
#   measure     warm-up, then --minutes: once a second CPU ticks of the emulator, CPU/GPU clocks, temperatures, fan pwm,
#               battery; frame times from the preload; PipeWire underruns (pw-top ERR) between warm-up end and end
#   guards      the run is ABORTED, quietly (clean quit) when: the fan starts (pwm1 > 0), a CPU/GPU zone passes 75 °C, the
#               battery is at 20 % or less and not charging, the emulator ends by itself, no picture within 60 s
#   quit        like pocket-ui without a state: Dolphin SIGTERM; Azahar a Qt close through sway, then SIGTERM, SIGKILL;
#               runemu.sh is never signalled, it restores the governors itself
#   saves       the emulators' config and save folders are tarred before and put back after (files changed or new are
#               undone and named in the report); pocket-ui itself is not involved, so its data, recents and play log
#               stay as they are and no POCKETUI_DATA is set anywhere
# Afterwards scripts/verify-device.sh. It never suspends, reboots or presses a button on the device; against pocket-ui's
# idle timer (not told about the game) a harmless "b" goes into its command FIFO every 20 s.
# Needs on the device: python3, wpctl, and /storage/.config/pocket-ui/no-autosuspend (the device must not deep-sleep).
# Exit: 0 measured, 2 usage/setup, 3 the device is not in its normal state afterwards or a game did not run, 4 aborted.
set -u
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PY="python3 -I $ROOT/scripts/big-emu-bench.py"
MINUTES=3 WARMUP=20 DIM=5 OUT= REPORT_ONLY=0 RUMBLE=0
die() { echo "big-emu-bench: $*" >&2; exit 2; }
q() { printf "'%s'" "$(printf %s "$1" | sed "s/'/'\\\\''/g")"; }

GAMES=
while [ $# -gt 0 ]; do
  case $1 in
    --minutes) MINUTES=${2:?}; shift 2 ;;
    --warmup) WARMUP=${2:?}; shift 2 ;;
    --dim) DIM=${2:?}; shift 2 ;;
    --out) OUT=${2:?}; shift 2 ;;
    --rumble) RUMBLE=1; shift ;;
    --report) REPORT_ONLY=1; shift ;;
    -h | --help) sed -n '2,/^set -u/p' "$0" | sed '$d; s/^# \{0,1\}//'; $PY thresholds; exit 0 ;;
    -*) die "unknown option $1" ;;
    *) GAMES="$GAMES
$1"; shift ;;
  esac
done
if [ "$REPORT_ONLY" = 1 ]; then
  for d in $GAMES; do set -- "$@" "$d"; done
  exec $PY report "$@"
fi
[ -n "$GAMES" ] || die "no game given (a path on the device, e.g. /storage/roms/wii/<game>.rvz). --help"
case $MINUTES in '' | *[!0-9.]*) die "--minutes wants a number" ;; esac
# 0.1 minute = 6 s is allowed for the dry run; the real limit is the 10-minute device session
SECS=$(awk -v m="$MINUTES" 'BEGIN { printf "%d", m * 60 + 0.5 }')
[ "$SECS" -ge 5 ] || die "--minutes is too short"
[ "$SECS" -le 300 ] || die "--minutes at most 5: one device session (start 60 s + warm-up + run + quit) has to stay under 10 minutes"
case $WARMUP$DIM in *[!0-9]*) die "--warmup and --dim want whole numbers" ;; esac
[ -n "$OUT" ] || OUT=$(mktemp -d "${TMPDIR:-/tmp}/big-emu-bench.XXXXXX")
mkdir -p "$OUT"
WORK=${BIG_WORK:-/tmp/big-emu-bench}

if [ -n "${BIGEMU_LOCAL:-}" ]; then
  dev() { sh -c "$1"; }
  put() { cp "$1" "$2"; }
  get() { (cd "$1" && tar -cf - .) | tar -xf - -C "$2"; }
  FTSO=${BIG_FTSO:-}
else
  . "$ROOT/scripts/host.sh"
  need_host
  SSHO="-o ServerAliveInterval=15 -o ConnectTimeout=15"
  dev() { ssh -n $SSHO "$HOST" "$1"; }
  put() { scp -q $SSHO "$1" "$HOST:$2"; }
  get() { ssh -n $SSHO "$HOST" "cd $(q "$1") && tar -cf - ." | tar -xf - -C "$2"; }
  FTSO=${FRAMETIME_SO:-$ROOT/target/ft/frametime.so}
  if [ ! -f "$FTSO" ] || [ "$FTSO" -ot "$ROOT/deploy/frametime.c" ]; then
    PATH=$HOME/opt/zig:$PATH
    command -v zig >/dev/null || die "deploy/frametime.so is missing and zig is not on PATH (see docs/DEVELOPING.md)"
    mkdir -p "$(dirname "$FTSO")"
    zig cc -target aarch64-linux-gnu.2.38 -shared -fPIC -O2 -o "$FTSO" "$ROOT/deploy/frametime.c" || die "building frametime.so failed"
  fi
fi

# From scripts/local.env (untracked) when set there: the lock file the other device scripts of this machine use.
LOCK=${BIGEMU_LOCK:-/tmp/pocket-device.flock}

# The device half, one file for all games.
DEVSH=$OUT/device.sh
cat >"$DEVSH" <<'EOF'
#!/bin/sh
# Device half of scripts/big-emu-bench.sh: sh device.sh ROM SYSTEM TARGET_FPS MEASURE_SECS WARMUP_SECS DIM_PCT OUTDIR
set -u
ROM=$1 SYSTEM=$2 TARGET=$3 MSECS=$4 WARM=$5 DIM=$6 OUT=$7
SYS=${BIG_SYS:-/sys}
STORAGE=${BIG_STORAGE:-/storage}
RUNEMU=${BIG_RUNEMU:-/usr/bin/runemu.sh}
FTDIR=${BIG_FTDIR:-/tmp/ft}
FTSO=${BIG_FTSO-/tmp/big-emu-bench/frametime.so}
BAK=${BIG_BACKUP:-$STORAGE/.big-emu-bench-backup}
CWD=${BIG_CWD:-/tmp}
FIFO=${BIG_FIFO:-/tmp/pocket-ui.in}
NOSLEEP=${BIG_NOSLEEP_FILE:-$STORAGE/.config/pocket-ui/no-autosuspend}
STARTWAIT=${BIG_STARTWAIT:-60}
BAT_MIN=${BIG_BAT_MIN:-20} FAN_LIMIT=${BIG_FAN_LIMIT:-0} TEMP_LIMIT=${BIG_TEMP_LIMIT:-75}
SAVE_MAX_MB=${BIG_SAVE_MAX_MB:-2000}
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/var/run/0-runtime-dir} WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1}
export SWAYSOCK=${SWAYSOCK:-$XDG_RUNTIME_DIR/sway-ipc.0.sock}
# What each emulator's folders are (config, data, states). Absent ones are removed again if the run creates them.
# PS2 memory cards sit among the games in roms/ps2: only the cards are tarred, never the disc images.
SAVE_DIRS=${BIG_SAVE_DIRS:-"$STORAGE/.config/dolphin-emu $STORAGE/.local/share/dolphin-emu $STORAGE/.cache/dolphin-emu $STORAGE/.config/azahar $STORAGE/.config/azahar-emu $STORAGE/.local/share/azahar-emu $STORAGE/.config/ARMSX2 $STORAGE/roms/bios/GC $STORAGE/roms/ps2/Mcd*.ps2 $STORAGE/roms/savestates/wii $STORAGE/roms/savestates/3ds $STORAGE/roms/savestates/gamecube $STORAGE/roms/savestates/ps2"}
RUMBLE_OFF=${BIG_RUMBLE_OFF:-1}

case $SYSTEM in
  wii) EMU=dolphin CORE=dolphin-sa-wii PNAME=dolphin-emu-nogui ;;
  3ds) EMU=azahar CORE=azahar-sa PNAME=azahar ;;
  gamecube) EMU=dolphin CORE=dolphin-sa-gc PNAME=dolphin-emu-nogui ;;
  ps2) EMU=armsx2 CORE=armsx2-sa PNAME=armsx2-qt ;;
  *) echo "no PS2, GameCube, Wii or 3DS game: $ROM" >&2; exit 2 ;;
esac
PNAME=${BIG_PNAME:-$PNAME}

# The host's SSH session may end (a killed script, a dropped Wi-Fi): then this half still goes on to the end and puts
# everything back, and writes to a pipe that is gone must not kill it (SIGPIPE) in the middle of putting back.
trap '' PIPE
STATUS=failed REASON="did not finish" SEEN=
MUTED= BL= BL_OLD= PID= QUIT= SAVES_CHANGED= XRUNS= XNODES= MONO0=0 MONO1=0 BAT0= BAT1= LAUNCHED= RESTORED=1
mkdir -p "$OUT"
say() { echo "$*" >&2; }
mono() { python3 -c 'import time; print(time.monotonic())'; }

# ---- the emulator's folders -------------------------------------------------------------------------------
rel() { echo "${1#/}"; }
backup_saves() {
  mkdir -p "$BAK" || return 1
  : >"$BAK/present"; : >"$BAK/absent"
  for d in $SAVE_DIRS; do
    if [ -e "$d" ]; then rel "$d" >>"$BAK/present"; else rel "$d" >>"$BAK/absent"; fi
  done
  kb=0
  for p in $(cat "$BAK/present"); do kb=$((kb + $(du -sk "/$p" | cut -f1))); done
  [ "$kb" -le $((SAVE_MAX_MB * 1024)) ] || { say "the emulators' folders are $((kb / 1024)) MB (limit $SAVE_MAX_MB): narrow BIG_SAVE_DIRS"; rm -rf "$BAK"; return 1; }
  free=$(df -k "$(dirname "$BAK")" | awk 'NR == 2 {print $4}')
  [ "$free" -gt $((kb * 2 + 102400)) ] || { say "not enough free space for a backup of $((kb / 1024)) MB"; rm -rf "$BAK"; return 1; }
  # shellcheck disable=SC2046
  if [ -s "$BAK/present" ]; then tar -C / -cf "$BAK/saves.tar" $(cat "$BAK/present") || { rm -rf "$BAK"; return 1; }
    tar -tf "$BAK/saves.tar" >/dev/null || { rm -rf "$BAK"; return 1; }
  else : >"$BAK/empty"; fi
  touch "$BAK/stamp"; sleep 1
}
# Files the run changed or created are undone; what was there stays where it is. The backup goes last.
restore_saves() {
  [ -f "$BAK/stamp" ] || return 0
  # The emulators rewrite their config on every start; only changes elsewhere (saves, states, data) are worth a word.
  n=0
  for p in $(cat "$BAK/present"); do
    for f in $(find "/$p" -newer "$BAK/stamp" ! -type d 2>/dev/null | head -50); do
      case $f in */.config/*) continue ;; esac
      n=$((n + 1)); [ "$n" -le 3 ] && SAVES_CHANGED="$SAVES_CHANGED${SAVES_CHANGED:+, }$f"
    done
  done
  [ "$n" -gt 3 ] && SAVES_CHANGED="$SAVES_CHANGED, ... ($n files)"
  if [ -s "$BAK/present" ]; then
    tar -C / -xf "$BAK/saves.tar" || { say "RESTORE FAILED: the backup stays in $BAK"; RESTORED=0; return 1; }
    tar -tf "$BAK/saves.tar" | sed 's:/$::' | sort >"$BAK/listing"
    for p in $(cat "$BAK/present"); do find "/$p" | sed 's:^/::'; done | sort >"$BAK/now"
    awk 'NR == FNR {a[$0]; next} !($0 in a)' "$BAK/listing" "$BAK/now" | while IFS= read -r extra; do rm -rf "/$extra"; done
  fi
  for p in $(cat "$BAK/absent"); do
    if [ -e "/$p" ]; then
      rm -rf "/$p"
      case $p in */.config/*) ;; *) SAVES_CHANGED="$SAVES_CHANGED${SAVES_CHANGED:+, }new folder /$p" ;; esac
    fi
  done
  rm -rf "$BAK"
}

# ---- the readings -------------------------------------------------------------------------------------------
first() { for f in "$@"; do [ -e "$f" ] && { echo "$f"; return; }; done; }
zone() { for z in "$SYS"/class/thermal/thermal_zone*; do read -r t <"$z/type" 2>/dev/null && [ "$t" = "$1" ] && { echo "$z/temp"; return; }; done; }
PWM=$(first "$SYS"/class/hwmon/hwmon*/pwm1)
GPUF=$(first "$SYS"/class/devfreq/*gpu*/cur_freq)
# The hottest CPU / GPU zone counts (the Mini V2 has cpu4-bottom / gpu-top, the Odin 2 Mini cpu0..7 / gpuss-0..7).
zones() { for z in "$SYS"/class/thermal/thermal_zone*; do read -r t <"$z/type" 2>/dev/null && case $t in $1) echo "$z/temp" ;; esac; done; }
ZCPU=$(zones 'cpu[0-9]*-thermal') ZGPU=$(zones 'gpu*-thermal')
hottest() { m=; for f in $1; do read -r v <"$f" 2>/dev/null; [ -n "$v" ] && { [ -z "$m" ] || [ "$v" -gt "$m" ]; } && m=$v; done; echo "$m"; }
BATD=$SYS/class/power_supply/battery
MIDP=$SYS/devices/system/cpu/cpufreq/policy4; [ -d "$MIDP" ] || MIDP=$SYS/devices/system/cpu/cpufreq/policy3
NCPU=$(grep -c '^cpu[0-9]' /proc/stat)
rd() { v=; [ -n "$1" ] && read -r v <"$1" 2>/dev/null; echo "$v"; }
ticks() {
  t=0
  for p in $(pidof "$PNAME"); do
    read -r line <"/proc/$p/stat" 2>/dev/null || continue
    set -- ${line##*) }
    t=$((t + ${12} + ${13}))
  done
  echo "$t"
}
alltick() { read -r _ u n s i io irq sirq st _ </proc/stat; echo $(((u + n + s + i + io + irq + sirq + st) / NCPU)); }
# runemu.sh is a script: BusyBox's pidof (the device) finds it by name, procps' (a host) wants -x.
runemu_up() { [ -n "$(pidof runemu.sh 2>/dev/null)" ] || [ -n "$(pidof -x runemu.sh 2>/dev/null)" ]; }
GUARD=
# One line of samples.csv; sets GUARD to the reason to stop, if there is one.
sample() {
  pwm=$(rd "$PWM"); cap=$(rd "$BATD/capacity"); bst=$(rd "$BATD/status"); cm=$(hottest "$ZCPU"); gm=$(hottest "$ZGPU")
  br=$(rd "${BL:+$BL/brightness}")
  echo "$1,$2,$(ticks),$(alltick),$(rd "$SYS/devices/system/cpu/cpufreq/policy0/scaling_cur_freq"),$(rd "$MIDP/scaling_cur_freq"),$(rd "$SYS/devices/system/cpu/cpufreq/policy7/scaling_cur_freq"),$(rd "$GPUF"),$cm,$gm,$pwm,$cap,$bst,$br,$(rd "$BATD/current_now"),$(rd "$BATD/voltage_now")" >>"$OUT/samples.csv"
  BAT1=$cap
  # a dark screen stays dark, whatever lights it (pocket-ui's own wake-up)
  [ -n "$BL" ] && [ "${BL_OLD:-1}" -eq 0 ] && [ "${br:-0}" -gt 0 ] && echo 0 >"$BL/brightness"
  [ -n "$pwm" ] && [ "$pwm" -gt "$FAN_LIMIT" ] && GUARD="the fan started (pwm1 $pwm)"
  [ -n "$cm" ] && [ "$cm" -gt $((TEMP_LIMIT * 1000)) ] && GUARD="CPU at $((cm / 1000)) °C"
  [ -n "$gm" ] && [ "$gm" -gt $((TEMP_LIMIT * 1000)) ] && GUARD="GPU at $((gm / 1000)) °C"
  [ -n "$cap" ] && [ "$cap" -le "$BAT_MIN" ] && [ "$bst" != Charging ] && GUARD="battery at $cap %"
  if [ -n "$(pidof "$PNAME")" ]; then SEEN=1
  elif [ -n "$SEEN" ]; then GUARD="the emulator ended by itself"
  elif [ -n "$LAUNCHED" ] && [ "$1" = s ] && [ "$2" -ge 5 ] && ! runemu_up; then GUARD="runemu.sh ended without starting the emulator"; fi
  return 0
}
# A press on pocket-ui's FIFO wakes a screen that is dark (it puts its own brightness back), so a dark screen gets none
# and is held dark below.
nudge() { [ "${BL_OLD:-1}" -gt 0 ] && [ -p "$FIFO" ] && timeout 1 sh -c "echo b >$FIFO" 2>/dev/null; }
pwerr() { command -v pw-top >/dev/null || return 0; pw-top -b -n 2 2>/dev/null | awk '$2 ~ /^[0-9]+$/ && $9 ~ /^[0-9]+$/ {print $2, $9, $NF}'; }

# ---- put everything back -----------------------------------------------------------------------------------
stop_emulator() {
  [ -n "$(pidof "$PNAME")" ] || { QUIT=${QUIT:-gone}; return 0; }
  if [ "$SYSTEM" = 3ds ] && command -v swaymsg >/dev/null; then
    for p in $(pidof "$PNAME"); do swaymsg "[pid=$p] kill" >/dev/null 2>&1; done
    waitgone 8 && { QUIT=${QUIT:-clean}; return 0; }
  else
    kill -TERM $(pidof "$PNAME") 2>/dev/null
    waitgone 10 && { QUIT=${QUIT:-clean}; return 0; }
  fi
  kill -TERM $(pidof "$PNAME") 2>/dev/null
  waitgone 8 && { QUIT=${QUIT:-term}; return 0; }
  kill -KILL $(pidof "$PNAME") 2>/dev/null
  waitgone 3; QUIT=kill
}
waitgone() { n=0; while [ -n "$(pidof "$PNAME")" ] && [ "$n" -lt "$1" ]; do sleep 1; n=$((n + 1)); done; [ -z "$(pidof "$PNAME")" ]; }
done_once=
cleanup() {
  [ -z "$done_once" ] || return 0
  done_once=1
  [ -n "$LAUNCHED" ] && stop_emulator
  # runemu.sh puts the governors back when its emulator ends; it is never signalled
  n=0; while runemu_up && [ "$n" -lt 20 ]; do sleep 1; n=$((n + 1)); done
  runemu_up && [ -n "$LAUNCHED" ] && say "warning: runemu.sh still runs 20 s after the emulator"
  if [ -d "$FTDIR" ]; then
    big=$(ls -S "$FTDIR" 2>/dev/null | head -1)
    [ -n "$big" ] && [ ! -s "$OUT/frames.txt" ] && cp "$FTDIR/$big" "$OUT/frames.txt"
  fi
  restore_saves
  [ -n "$BL" ] && [ -n "$BL_OLD" ] && [ "$(rd "$BL/brightness")" != "$BL_OLD" ] && echo "$BL_OLD" >"$BL/brightness"
  [ -n "$MUTED" ] && wpctl set-mute @DEFAULT_AUDIO_SINK@ "$MUTED" >/dev/null 2>&1
  rm -rf "$FTDIR"
  {
    echo "game=$(basename "$ROM")"; echo "system=$SYSTEM"; echo "target=$TARGET"; echo "status=$STATUS"; echo "reason=$REASON"
    echo "mono_start=$MONO0"; echo "mono_end=$MONO1"; echo "xruns=$XRUNS"; echo "xrun_nodes=$XNODES"
    echo "bat_start=$BAT0"; echo "bat_end=$BAT1"; echo "quit=$QUIT"; echo "saves_changed=$SAVES_CHANGED"
    echo "saves_restored=$RESTORED"; echo "rumble_off=$RUMBLE_OFF"
  } >"$OUT/meta"
  # ARMSX2's first SIGTERM only shuts the VM down (its memory cards are flushed); the second ends it: "term" is its normal Quit
  [ "$SYSTEM" = ps2 ] && [ "$QUIT" = term ] && QUIT=clean
  [ "$STATUS" = ok ] && [ "$QUIT" != clean ] && say "note: the emulator needed more than a clean quit ($QUIT)"
}
trap cleanup EXIT
trap 'REASON=interrupted; exit 130' INT TERM HUP

# ---- checks before anything changes --------------------------------------------------------------------------
fail() { REASON=$1; say "$1"; exit "${2:-3}"; }
[ -f "$ROM" ] || fail "no such file on the device: $ROM" 2
[ -e "$NOSLEEP" ] || fail "$NOSLEEP is missing: the device could deep-sleep during the run (docs/DEVICE-TESTS.md, Before)" 2
command -v wpctl >/dev/null || fail "wpctl is missing: the sink can not be muted" 2
command -v python3 >/dev/null || fail "python3 is missing" 2
[ -n "$(pidof "$PNAME" dolphin-emu-nogui armsx2-qt azahar retroarch pocket-core cemu rpcs3-sa)" ] && fail "a game is running already" 2
cap=$(rd "$BATD/capacity"); bst=$(rd "$BATD/status"); BAT0=$cap
[ -n "$cap" ] && [ "$cap" -le "$BAT_MIN" ] && [ "$bst" != Charging ] && fail "battery at $cap % (guard $BAT_MIN %)" 2
if [ -f "$BAK/stamp" ]; then
  say "an earlier run left its backup in $BAK: putting it back first"
  restore_saves || fail "could not restore the earlier backup" 2
fi
backup_saves || fail "could not back up the emulators' folders" 2
RESTORED=1

# ---- no vibration (the emulators' config is in the backup, so it is put back with the rest) ---------------------
rumble_off() {
  case $SYSTEM in
    ps2) f=$STORAGE/.config/ARMSX2/inis/PCSX2.ini
      [ -f "$f" ] && sed -i 's/^\(LargeMotorScale\|SmallMotorScale\) *=.*/\1 = 0/' "$f" ;;
    gamecube | wii)
      # start_dolphin_gc.sh copies a stock profile over GCPadNew.ini at every start: change the profiles it copies from
      for f in "$STORAGE"/.config/dolphin-emu/GCPadNew.ini "$STORAGE"/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.*; do
        [ -f "$f" ] && sed -i 's:^Rumble/Motor *=.*:Rumble/Motor =:' "$f"
      done
      for f in "$STORAGE"/.config/dolphin-emu/WiimoteNew.ini "$STORAGE"/.config/dolphin-emu/WiimoteProfiles/*; do
        [ -f "$f" ] && sed -i 's:^Rumble/Motor *=.*:Rumble/Motor =:' "$f"
      done ;;
  esac
  case $SYSTEM in
    ps2) say "rumble: $(grep -h 'MotorScale' "$STORAGE/.config/ARMSX2/inis/PCSX2.ini" | tr '\n' ' ')" ;;
    gamecube | wii) say "rumble: $(grep -h '^Rumble/Motor' "$STORAGE"/.config/dolphin-emu/GCPadNew.ini "$STORAGE"/.config/dolphin-emu/GamecubeControllerProfiles/GCPadNew.ini.* 2>/dev/null | sort | uniq -c | tr '\n' ' ')" ;;
  esac
}
[ "$RUMBLE_OFF" = 1 ] && rumble_off

# ---- mute and dim ----------------------------------------------------------------------------------------------
MUTED=$(wpctl get-volume @DEFAULT_AUDIO_SINK@ 2>/dev/null | grep -q MUTED && echo 1 || echo 0)
wpctl set-mute @DEFAULT_AUDIO_SINK@ 1
BL=$(first "$SYS"/class/backlight/*/brightness); BL=${BL%/brightness}
if [ -n "$BL" ]; then
  BL_OLD=$(rd "$BL/brightness"); mx=$(rd "$BL/max_brightness"); new=$((${mx:-100} * DIM / 100)); [ "$new" -ge 1 ] || new=1
  # a dark screen (idle) stays dark: the dimmed one would wake it
  [ "${BL_OLD:-0}" -gt 0 ] && echo "$new" >"$BL/brightness"
fi

# ---- start the way pocket-ui does ------------------------------------------------------------------------------
echo "phase,t,proc_ticks,all_ticks,little_khz,big_khz,prime_khz,gpu_hz,cpu_mc,gpu_mc,pwm,bat_pct,bat_status,bright,cur_ua,volt_uv" >"$OUT/samples.csv"
rm -rf "$FTDIR"; mkdir -p "$FTDIR"
cd "$CWD" || fail "no $CWD" 3
LAUNCHED=1
nudge
if [ -n "$FTSO" ]; then export LD_PRELOAD=$FTSO; fi
nohup "$RUNEMU" "$ROM" "-P$SYSTEM" "--core=$CORE" "--emulator=$EMU" --controllers= >"$OUT/runemu.log" 2>&1 </dev/null &
unset LD_PRELOAD
i=0
while [ $i -lt "$STARTWAIT" ]; do
  sleep 1; i=$((i + 1))
  sample s "$i"
  [ -n "$GUARD" ] && break
  # the first pictures: the preload writes in blocks of 30 frames
  for f in "$FTDIR"/*; do [ -s "$f" ] && break 2; done
done
tick=0
if [ -n "$GUARD" ]; then
  STATUS=aborted REASON=$GUARD
elif [ ! -s "$(ls -S "$FTDIR"/* 2>/dev/null | head -1)" ]; then
  STATUS=failed REASON="no picture within $STARTWAIT s (runemu.log: $(tail -c 300 "$OUT/runemu.log" | tr '\n' ' '))"
else
  # warm-up
  while [ "$tick" -lt "$WARM" ] && [ -z "$GUARD" ]; do
    sleep 1; tick=$((tick + 1)); sample w "$tick"
    [ $((tick % 20)) -eq 0 ] && nudge
  done
  if [ -n "$GUARD" ]; then STATUS=aborted REASON=$GUARD; else
    pwerr >"$OUT/pw0"
    MONO0=$(mono); tick=0
    while [ "$tick" -lt "$MSECS" ] && [ -z "$GUARD" ]; do
      sleep 1; tick=$((tick + 1)); sample m "$tick"
      [ $((tick % 20)) -eq 0 ] && nudge
    done
    MONO1=$(mono)
    pwerr >"$OUT/pw1"
    if [ -n "$GUARD" ]; then STATUS=aborted REASON=$GUARD; else STATUS=ok REASON=; fi
    if [ -s "$OUT/pw1" ]; then
      awk 'NR == FNR {a[$1] = $2; next} {d = $2 - ($1 in a ? a[$1] : 0); if (d > 0) {t += d; n = n " " $3 "(" d ")"}} END {print t + 0; print n}' "$OUT/pw0" "$OUT/pw1" >"$OUT/pw.sum"
      XRUNS=$(sed -n 1p "$OUT/pw.sum"); XNODES=$(sed -n 2p "$OUT/pw.sum")
    fi
  fi
fi
[ "$STATUS" = ok ] || say "$STATUS: $REASON"
[ "$STATUS" = failed ] && exit 3
[ "$STATUS" = aborted ] && exit 4
exit 0
EOF

# ---- the host side ---------------------------------------------------------------------------------------------
run_game() { # <index> <rom> <target>  (runs under the device lock)
  idx=$1 rom=$2 target=$3
  sysn=${BIGEMU_SYSTEM:-}
  if [ -z "$sysn" ]; then
    case $rom in
      */roms/wii/* | */wii/*) sysn=wii ;;
      */roms/3ds/* | */3ds/*) sysn=3ds ;;
      */roms/ps2/* | */ps2/*) sysn=ps2 ;;
      */roms/gamecube/* | */gamecube/* | */roms/gc/*) sysn=gamecube ;;
    esac
  fi
  [ -n "$sysn" ] || { echo "big-emu-bench: $rom: not in a ps2, gamecube, wii or 3ds folder (BIGEMU_SYSTEM=ps2|gamecube|wii|3ds)" >&2; return 2; }
  rdir=$WORK/run-$idx
  envs=$(env | sed -n 's/^\(BIG_[A-Z0-9_]*\)=\([^ ]*\)$/\1=\2/p' | tr '\n' ' ')
  if [ "$RUMBLE" = 1 ]; then envs="$envs BIG_RUMBLE_OFF=0"; fi
  dev "rm -rf $(q "$rdir"); mkdir -p $(q "$WORK")" || return 3
  put "$DEVSH" "$WORK/device.sh" || return 3
  if [ -n "$FTSO" ]; then put "$FTSO" "$WORK/frametime.so" || return 3; fi
  echo "== $(basename "$rom") ($sysn, target $target fps): start, ${WARMUP} s warm-up, ${SECS} s measured" >&2
  dev "env $envs sh $(q "$WORK/device.sh") $(q "$rom") $sysn $target $SECS $WARMUP $DIM $(q "$rdir") </dev/null"
  rc=$?
  mkdir -p "$OUT/g$idx"
  get "$rdir" "$OUT/g$idx" 2>/dev/null
  dev "rm -rf $(q "$rdir")" >/dev/null 2>&1
  return $rc
}
with_lock() {
  if [ "$LOCK" = none ]; then "$@"; return $?; fi
  command -v flock >/dev/null || die "flock is missing (BIGEMU_LOCK=none to go without)"
  ( flock -w 900 9 || exit 5; "$@" ) 9>"$LOCK"
}

worst=0
idx=0
printf '%s\n' "$GAMES" >"$OUT/games.txt"
while IFS= read -r g <&3; do
  [ -n "$g" ] || continue
  idx=$((idx + 1))
  target=60
  case $g in *@[0-9][0-9]) target=${g##*@}; g=${g%@*} ;; esac
  with_lock run_game "$idx" "$g" "$target"
  rc=$?
  [ $rc -gt $worst ] && worst=$rc
  [ $rc -eq 2 ] && break
  [ $rc -eq 5 ] && { echo "big-emu-bench: the device lock was busy for 15 minutes" >&2; break; }
done 3<"$OUT/games.txt"

# The device in its normal state again? (not for the dry run)
if [ -z "${BIGEMU_LOCAL:-}" ]; then
  "$ROOT/scripts/verify-device.sh" --no-local "$HOST" >&2 || { echo "big-emu-bench: the device is not in its normal state (VERIFY above)" >&2; [ $worst -lt 3 ] && worst=3; }
  dev "rm -rf $(q "$WORK")" >/dev/null 2>&1
fi
dirs=
for d in "$OUT"/g*; do [ -f "$d/meta" ] && dirs="$dirs $d"; done
if [ -n "$dirs" ]; then
  # shellcheck disable=SC2086
  $PY report $dirs | tee "$OUT/report.txt"
  echo "(raw runs in $OUT)"
fi
exit $worst
