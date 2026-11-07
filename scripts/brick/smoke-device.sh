#!/bin/sh
# The device half of scripts/brick-smoke.sh (docs/TRIMUI-BRICK-SMOKE.md): runs ON the Brick (BusyBox sh, no bash, no python, no timeout, whole
# seconds only in sleep), reads the card, the services and sysfs, and with --owner-ok drives pocket-ui through its FIFO. Prints one line per check:
#   RESULT <PASS|FAIL|SKIP> <id> <detail>        CAP <name> <file> <stride> <y>        INFO <text>        DONE
# It writes only below <card>/.pocket-ui-test/smoke and /tmp, plus a per-game settings file for its own test game (removed again).
# Never: reboot, suspend, sound on, pkill. Sound (owner's rule, 04.11.): the run starts only when the speaker and jack switches are off and the
# DAC is 0 (else FAIL 0-mute and nothing else runs); a watcher puts them back off every second for the whole run (each time it had to is a FAIL);
# at the end the Brick must be muted AND dark (the power press over the FIFO, the dark state), checked. With --owner-ok the panel is kept dim
# (brick-hw bl-set, raw 3) while pocket-ui has it on: the picture checks read the framebuffer, not the light.
# Tests: scripts/test-brick-smoke.sh runs this file against a fake tree (R=<dir> moves /sys /proc /tmp /mnt /dev below it).
#   smoke-device.sh [--owner-ok] [--rom FILE] [--system gb] [--expect-version V]

R=${R:-}
SYS=$R/sys; PROC=$R/proc
SDC=${SMOKE_CARD:-$R/mnt/SDCARD}
D=$SDC/pocket-ui
STATE=$D/storage/.config/pocket-ui
RUN=${SMOKE_RUN:-$R/tmp/pocket-ui}
FIFO=${SMOKE_FIFO:-$R/tmp/pocket-ui.in}
CORECMD=${SMOKE_CORECMD:-$R/tmp/pocket-core.cmd}
FB=${SMOKE_FB:-$R/dev/fb0}
BL=${SMOKE_BACKLIGHT:-$RUN/hwroot/sys/class/backlight/panel/brightness}
BATT=${SMOKE_BATTERY:-$SYS/class/power_supply/axp2202-battery}
WORK=$SDC/.pocket-ui-test/smoke
AMIXER=${SMOKE_AMIXER:-amixer}
HW=${SMOKE_HW:-$D/app/bin/brick-hw}
DIM=${SMOKE_DIM:-3}
CPUF=$SYS/devices/system/cpu/cpufreq
LEDS=$SYS/class/led_anim
STEP=${SMOKE_STEP:-1}            # seconds per wait step (the host test makes it small)
OWNER=0; ROM=; SYSTEM=gb; EXPECT=
while [ $# -gt 0 ]; do
  case $1 in
    --owner-ok) OWNER=1 ;;
    --rom) ROM=$2; shift ;;
    --system) SYSTEM=$2; shift ;;
    --expect-version) EXPECT=$2; shift ;;
  esac
  shift
done

res() { printf 'RESULT %s %s %s\n' "$1" "$2" "$3"; }
info() { printf 'INFO %s\n' "$*"; }
nap() { sleep "$(awk -v s="$STEP" -v k="${1:-1}" 'BEGIN { print s * k }')"; }   # k steps (STEP is seconds, the host test makes it small)
rd() { tr -d ' \r\n' <"$1" 2>/dev/null; }                 # a sysfs value without blanks, empty if unreadable
num() { v=$(rd "$1"); case $v in '' | *[!0-9]*) echo 0 ;; *) echo "$v" ;; esac; }

# --- processes, by looking into /proc (BusyBox ps differs between builds)
pids_of() { # comm
  for p in "$PROC"/[0-9]*; do
    [ "$(rd "$p/comm")" = "$1" ] && echo "${p##*/}"
  done
}
has_cmd() { # text that the command line must contain (arguments joined by a blank)
  for p in "$PROC"/[0-9]*; do
    [ "${p##*/}" = "$$" ] && continue
    c=$(tr '\0' ' ' <"$p/cmdline" 2>/dev/null)
    case $c in *"$1"*) return 0 ;; esac
  done
  return 1
}
ticks() { # pid -> utime+stime (clock ticks)
  s=$(sed 's/^.*) //' "$PROC/$1/stat" 2>/dev/null) || { echo 0; return; }
  set -- $s
  echo $(( ${12:-0} + ${13:-0} ))
}
game_running() { [ -n "$(pids_of pocket-core)" ]; }
bl_now() { num "$BL"; }

# --- the FIFO, opened without waiting (a pocket-ui that is gone must not hang the check)
send() { # file word...
  f=$1; shift
  [ -p "$f" ] || [ -e "$f" ] || return 1
  ( echo "$*" >"$f" ) & wp=$!
  nap 1
  kill "$wp" 2>/dev/null
  return 0
}
waitfor() { # seconds condition
  n=0
  while [ $n -lt "$1" ]; do
    eval "$2" && return 0
    nap 1; n=$((n + 1))
  done
  eval "$2"
}

# --- the CPU clusters
cluster_dump() { # one line per cluster: name governor max (the current speed moves, so it is not part of a comparison)
  for p in "$CPUF"/policy*; do
    [ -d "$p" ] || continue
    echo "${p##*/} $(rd "$p/scaling_governor") $(num "$p/scaling_max_freq")"
  done
}
low_freq() { # cluster dir -> its lowest available frequency
  f=$(tr ' ' '\n' <"$1/scaling_available_frequencies" 2>/dev/null | grep '^[0-9]' | sort -n | head -n 1)
  [ -n "$f" ] || f=$(num "$1/cpuinfo_min_freq")
  echo "$f"
}
# check_clusters <id> <what> <want: dark|play|menu>
check_clusters() {
  id=$1; what=$2; want=$3; bad=; seen=0; shown=
  for p in "$CPUF"/policy*; do
    [ -d "$p" ] || continue
    seen=$((seen + 1))
    g=$(rd "$p/scaling_governor"); mx=$(num "$p/scaling_max_freq"); cur=$(num "$p/scaling_cur_freq")
    top=$(num "$p/cpuinfo_max_freq"); low=$(low_freq "$p")
    shown="$shown ${p##*/}:$g,max=$((mx / 1000))MHz,now=$((cur / 1000))MHz"
    case $want in
      dark)
        [ "$g" = powersave ] || bad="$bad ${p##*/} governor $g (want powersave)"
        [ "$mx" = "$low" ] || bad="$bad ${p##*/} max $((mx / 1000)) MHz (want $((low / 1000)))"
        [ "$cur" -le "$low" ] || bad="$bad ${p##*/} runs at $((cur / 1000)) MHz" ;;
      play)
        [ "$g" = performance ] || bad="$bad ${p##*/} governor $g (want performance)"
        [ "$top" -eq 0 ] || [ "$mx" = "$top" ] || bad="$bad ${p##*/} max $((mx / 1000)) of $((top / 1000)) MHz"
        [ "$top" -eq 0 ] || [ $((cur * 2)) -ge "$top" ] || bad="$bad ${p##*/} runs at $((cur / 1000)) MHz of $((top / 1000))" ;;
      dark-menu)
        # The dark menu without a game: no game is held, so pocket-ui leaves the governor; it must rest (not performance, not pinned high).
        case $g in performance|userspace) bad="$bad ${p##*/} governor $g (want a resting one)" ;; esac
        [ "$g" = powersave ] || [ "$top" -eq 0 ] || [ "$(num "$p/scaling_min_freq")" -lt "$top" ] || bad="$bad ${p##*/} pinned at the top" ;;
      menu)
        [ "$g" != powersave ] || bad="$bad ${p##*/} governor powersave"
        [ "$top" -eq 0 ] || [ "$mx" = "$top" ] || bad="$bad ${p##*/} max capped at $((mx / 1000)) of $((top / 1000)) MHz" ;;
    esac
  done
  if [ $seen -eq 0 ]; then res SKIP "$id" "$what: no cpufreq here"
  elif [ -n "$bad" ]; then res FAIL "$id" "$what:$bad |$shown"
  else res PASS "$id" "$what:$shown"; fi
}

# --- the lights (Lights=Off must leave nothing lit)
lights_mode() { sed -n 's/.*"mode"[ ]*:[ ]*"\([a-z]*\)".*/\1/p' "$STATE/leds.json" 2>/dev/null | head -n 1; }
check_leds() { # id what
  id=$1; what=$2
  [ -d "$LEDS" ] || { res SKIP "$id" "$what: no LED driver (led_anim)"; return; }
  m=$(lights_mode); m=${m:-off}
  [ "$m" = off ] || { res SKIP "$id" "$what: Lights is '$m', not Off"; return; }
  bad=; n=0
  for f in "$LEDS"/effect_*; do
    [ -f "$f" ] || continue
    case ${f##*/} in effect_rgb_hex_* | effect_duration_* | effect_cycles_*) continue ;; esac
    n=$((n + 1))
    [ "$(num "$f")" = 0 ] || bad="$bad ${f##*/}=$(rd "$f")"
  done
  if [ -f "$LEDS/frame_hex" ]; then
    n=$((n + 1))
    [ -z "$(tr -d '0 \r\n' <"$LEDS/frame_hex" 2>/dev/null)" ] || bad="$bad frame_hex=$(head -c 40 "$LEDS/frame_hex" | tr -d '\n')"
  fi
  if [ -f "$LEDS/anim_frames_enable" ]; then n=$((n + 1)); [ "$(num "$LEDS/anim_frames_enable")" = 0 ] || bad="$bad anim_frames_enable=$(rd "$LEDS/anim_frames_enable")"; fi
  n=$((n + 1)); [ ! -f "$LEDS/effect_enable" ] || [ "$(num "$LEDS/effect_enable")" = 0 ] || bad="$bad effect_enable=$(rd "$LEDS/effect_enable")"
  for f in "$SYS"/class/leds/sunxi_led*/brightness; do
    [ -f "$f" ] || continue
    n=$((n + 1))
    [ "$(num "$f")" = 0 ] || bad="$bad ${f#$SYS/class/leds/}=$(rd "$f")"
  done
  if [ -n "$bad" ]; then res FAIL "$id" "$what: lit:$bad"; else res PASS "$id" "$what: $n values all 0 (max_scale $(rd "$LEDS/max_scale"))"; fi
}

# --- the framebuffer, read only: the page that is shown (pan "x,y"), 1024 rows
capture() { # name -> CAP line
  [ -r "$FB" ] || { info "capture $1: $FB is not readable"; return 1; }
  stride=4096; [ -r "$SYS/class/graphics/fb0/stride" ] && stride=$(num "$SYS/class/graphics/fb0/stride")
  [ "$stride" -gt 0 ] || stride=4096
  tries=0
  while [ $tries -lt 3 ]; do
    p1=$(rd "$SYS/class/graphics/fb0/pan"); y=${p1#*,}; case $y in '' | *[!0-9]*) y=0 ;; esac
    mkdir -p "$WORK/cap"
    dd if="$FB" of="$WORK/cap/$1.raw" bs="$stride" skip="$y" count=768 2>/dev/null
    p2=$(rd "$SYS/class/graphics/fb0/pan")
    [ "$p1" = "$p2" ] && break
    tries=$((tries + 1))
  done
  printf 'CAP %s %s %s %s\n' "$1" "$WORK/cap/$1.raw" "$stride" "$y"
}

# --- what is the owner's (never changes in a smoke run)
guard_list() {
  ( cd "$SDC" 2>/dev/null || exit 0
    [ ! -d Saves ] || find Saves -type f 2>/dev/null | sort | while IFS= read -r f; do md5sum "$f"; done
    [ ! -d Roms ] || find Roms -type f ! -path '*/.media/*' 2>/dev/null | sort | while IFS= read -r f; do echo "$(wc -c <"$f" | tr -d ' ') $f"; done ) >"$1" 2>/dev/null
}

# --- sound: the mixer's switches and DAC (`volume` stand-in's names); muted = both switches off and DAC 0,0
mix() { "$AMIXER" -c 0 cget name="$1" 2>/dev/null | sed -n 's/^ *: values=//p'; }
sound_state() { echo "HpSpeaker=$(mix 'HpSpeaker Switch') Headphone=$(mix 'Headphone Switch') DAC=$(mix 'DAC volume')"; }
muted() { [ "$(mix 'HpSpeaker Switch')" = off ] && [ "$(mix 'Headphone Switch')" = off ] && [ "$(mix 'DAC volume')" = 0,0 ]; }
mute_now() { for c in 'HpSpeaker Switch' 'Headphone Switch'; do "$AMIXER" -c 0 -q cset name="$c" off >/dev/null 2>&1; done; "$AMIXER" -c 0 -q cset name='DAC volume' 0,0 >/dev/null 2>&1; }

# --- clean-up on every way out: end a game we started, remove the settings file we wrote, then the Brick dark and muted (checked)
GAME=0; SETFILE=; WATCH=
cleanup() {
  trap - EXIT INT TERM HUP
  if [ $GAME = 1 ] && game_running; then send "$CORECMD" quit nosave; waitfor 15 '! game_running'; fi
  [ -z "$SETFILE" ] || rm -f "$SETFILE" 2>/dev/null
  if [ -n "$WATCH" ]; then
    # Dark at the end, always: the dark state over the FIFO (a press on a dark screen would only wake it, so only when it is on).
    if [ "$(bl_now)" != 0 ] && [ -p "$FIFO" ]; then send "$FIFO" power; waitfor 10 '[ "$(bl_now)" = 0 ]'; fi
    touch "$WORK/mute.stop"; nap 2; kill "$WATCH" 2>/dev/null
    muted || mute_now
    if muted; then res PASS 9-end-muted "$(sound_state)"; else res FAIL 9-end-muted "STILL NOT MUTED: $(sound_state)"; fi
    if [ "$(bl_now)" = 0 ]; then res PASS 9-end-dark "backlight 0, pocket-ui in its dark state"; else res FAIL 9-end-dark "the screen is STILL ON ($(bl_now)): press the power key on the Brick"; fi
    # Still (night runs: silent, dark, still): the test games run with rumble off and nothing here writes the motor; its pin must be 0.
    m=$SYS/class/gpio/gpio227/value
    if [ ! -e "$m" ] || [ "$(num "$m")" = 0 ]; then res PASS 9-end-still "motor pin 0 (the test games ran with rumble off)"; else res FAIL 9-end-still "the motor pin reads $(rd "$m")"; fi
    n=$(grep -c . "$WORK/mute.log" 2>/dev/null); n=${n:-0}
    if [ "$n" = 0 ]; then res PASS 9-sound-stayed-off "the watcher never had to mute"; else res FAIL 9-sound-stayed-off "something switched the sound on $n time(s) during the run, muted again at once ($(tail -n 1 "$WORK/mute.log"))"; fi
  fi
  echo DONE
}
trap cleanup EXIT INT TERM HUP
mkdir -p "$WORK"
# ============================== 0. sound off before anything, and a watcher for the whole run
if ! muted; then res FAIL 0-mute "sound is not off ($(sound_state)): nothing was run. Mute first: both switches off, DAC 0"; exit 1; fi
res PASS 0-mute "$(sound_state)"
rm -f "$WORK/mute.stop" "$WORK/mute.log"
( while [ ! -e "$WORK/mute.stop" ]; do
    muted || { echo "$(date +%H:%M:%S) $(sound_state)" >>"$WORK/mute.log"; mute_now; }
    # --owner-ok at night: the panel dim while pocket-ui has it on (the picture checks read the framebuffer)
    [ "$OWNER" = 1 ] && [ "$(bl_now)" != 0 ] && "$HW" bl-set "$DIM" >/dev/null 2>&1
    sleep 1
  done ) &
WATCH=$!
guard_list "$WORK/guard.before"

# ============================== 1. the card's version
REL=$D/app/RELEASE
ver=$(head -n 1 "$REL" 2>/dev/null | tr -d '\r')
vline=$(head -n 1 "$D/app/VERSION" 2>/dev/null)
commit=$(echo "$vline" | sed -n 's/.*commit \([0-9a-f]*\).*/\1/p')
if [ -z "$ver" ]; then res FAIL 1-version "no $REL on the card"
elif [ -n "$EXPECT" ] && [ "$ver" != "$EXPECT" ]; then res FAIL 1-version "card has $ver, expected $EXPECT"
elif [ -e "$STATE/../../pocket-ui/rolled-back" ] || [ -e "$D/storage/pocket-ui/rolled-back" ]; then res FAIL 1-version "$ver, but the last update was ROLLED BACK ($(rd "$D/storage/pocket-ui/rolled-back"))"
elif [ -e "$D/storage/pocket-ui/update-pending" ]; then res FAIL 1-version "$ver is installed but never confirmed itself (update-pending)"
else res PASS 1-version "$ver (commit ${commit:-unknown})"; fi

# ============================== 2. services and the route
miss=; ok=
for pair in "pocket-ui:pocket-ui" "pocket-fb:pocket-fb"; do
  nm=${pair%%:*}
  if [ -n "$(pids_of "$nm")" ]; then ok="$ok $nm"; else miss="$miss $nm"; fi
done
if has_cmd "brick-hw watch "; then ok="$ok brick-hw-watch"; else miss="$miss brick-hw-watch"; fi
if has_cmd "brick-net watch"; then ok="$ok brick-net"; else miss="$miss brick-net"; fi
if grep -q '^tailscale.up=1' "$D/storage/.config/system/configs/system.cfg" 2>/dev/null; then
  if [ -n "$(pids_of tailscaled)" ]; then ok="$ok tailscaled"; else miss="$miss tailscaled(switched on, not running)"; fi
else
  [ -z "$(pids_of tailscaled)" ] || ok="$ok tailscaled(on)"
  info "tailscale is switched off on this card: tailscaled not required"
fi
route=$(awk 'NR>1 && $2=="00000000" {print $1; exit}' "$PROC/net/route" 2>/dev/null)
[ -n "$route" ] || miss="$miss default-route"
if [ -n "$miss" ]; then res FAIL 2-services "missing:$miss | running:$ok | route via ${route:-NONE}"
else res PASS 2-services "running:$ok | default route on ${route}"; fi

# ============================== 3. the clock in the menu
if [ "$(bl_now)" = 0 ] && [ -e "$BL" ]; then res SKIP 3-clock-menu "the screen is dark right now"
elif game_running; then res SKIP 3-clock-menu "a game is running"
else check_clusters 3-clock-menu "menu" menu; fi

# ============================== 4. the lights with Lights=Off
check_leds 4-lights "menu, Lights=Off"

# A Brick left dark (the end state of every session) is woken for 5 and 6 (dim, see above); the clean-up darkens it again.
if [ "$OWNER" = 1 ] && [ -p "$FIFO" ] && ! game_running && [ "$(bl_now)" = 0 ]; then
  send "$FIFO" power
  if waitfor 10 '[ "$(bl_now)" != 0 ]'; then info "woken from the dark state for 5 and 6"; else info "did not wake from the dark state"; fi
fi
# ============================== 5. the dark screen, started the way pocket-ui does it (its FIFO)
if [ $OWNER = 0 ]; then
  res SKIP 5-dark "needs --owner-ok (the screen goes dark for ~25 s)"
elif ! [ -p "$FIFO" ]; then res FAIL 5-dark "no FIFO $FIFO: is pocket-ui running?"
elif game_running; then res SKIP 5-dark "a game is running"
elif [ "$(bl_now)" = 0 ]; then res SKIP 5-dark "the screen is dark already"
else
  before=$(cluster_dump | tr '\n' ';')
  DARKENED=1
  send "$FIFO" power
  if waitfor 10 '[ "$(bl_now)" = 0 ]'; then res PASS 5-dark-goes-dark "backlight 0 after the short power press over the FIFO"
  else res FAIL 5-dark-goes-dark "backlight still $(bl_now) 10 s after 'power'"; fi
  nap 3
  if game_running; then check_clusters 5-dark-clock "dark, a game waiting" dark; else check_clusters 5-dark-clock "dark menu" dark-menu; fi
  check_leds 5-dark-lights "dark, Lights=Off"
  # CPU load of ours over a window: a dark screen must leave them idle (each under 5 % of one CPU)
  hz=$(getconf CLK_TCK 2>/dev/null); hz=${hz:-100}
  win=${SMOKE_WINDOW:-6}
  names="pocket-ui pocket-fb pocket-core"; t0=;
  for n in $names; do for p in $(pids_of $n); do t0="$t0 $n:$p:$(ticks "$p")"; done; done
  sleep "$win"
  bad=; shown=
  for e in $t0; do
    n=${e%%:*}; r=${e#*:}; p=${r%%:*}; a=${r#*:}
    b=$(ticks "$p"); pct=$(( (b - a) * 100 / (hz * win) ))
    shown="$shown $n=${pct}%"
    [ "$pct" -le 5 ] || bad="$bad $n=${pct}%"
  done
  if [ -z "$t0" ]; then res FAIL 5-dark-load "no pocket-ui/pocket-fb/pocket-core process to measure"
  elif [ -n "$bad" ]; then res FAIL 5-dark-load "busy while dark:$bad (all:$shown)"
  else res PASS 5-dark-load "idle over ${win} s:$shown"; fi
  # wake: the power key again (a dark Brick card wakes only by it)
  send "$FIFO" power
  waitfor 10 '[ "$(bl_now)" != 0 ]' || { send "$FIFO" power; waitfor 10 '[ "$(bl_now)" != 0 ]'; }
  if [ "$(bl_now)" != 0 ]; then DARKENED=0; res PASS 5-wake "backlight back at $(bl_now)"; else res FAIL 5-wake "still dark"; fi
  nap 3
  check_clusters 5-wake-clock "awake again" menu
  check_leds 5-wake-lights "awake again, Lights=Off"
  [ "$(cluster_dump | tr '\n' ';')" = "$before" ] || info "clusters before: $before"
fi

# ============================== 6. a short test game: the clock in play, a picture option, Over the game
cap_ok=0
if [ $OWNER = 0 ]; then
  res SKIP 6-game "needs --owner-ok (starts a test game on the screen)"
elif [ -z "$ROM" ] || [ ! -f "$ROM" ]; then res SKIP 6-game "no test game on the Brick (--rom)"
elif ! [ -p "$FIFO" ]; then res FAIL 6-game "no FIFO $FIFO: is pocket-ui running?"
elif game_running; then res SKIP 6-game "a game is running"
elif [ "$(bl_now)" = 0 ]; then res SKIP 6-game "the screen is dark"
else
  pct=$(num "$BATT/capacity")
  if [ -e "$BATT/capacity" ] && [ "$pct" -lt 20 ]; then res SKIP 6-game "battery at $pct % (below 20 %)"
  else
    stem=${ROM##*/}; stem=${stem%.*}
    SETFILE=$STATE/game-settings/$SYSTEM/$stem.json
    mkdir -p "$STATE/game-settings/$SYSTEM"
    gl_line=$(grep 'GL: ' "$D/logs/boot.log" 2>/dev/null | tail -n 1)
    info "bridge: ${gl_line:-no GL line in boot.log}"
    pre=$(cluster_dump | tr '\n' ';')
    # run <name> <settings json> [second capture]
    run_game() {
      printf '%s\n' "$2" >"$SETFILE"
      GAME=1
      send "$FIFO" play "$SYSTEM" "$ROM"
      if ! waitfor 40 'game_running'; then
        res FAIL "6-start-$1" "pocket-core did not start within 40 s (a pocket-ui without the FIFO 'play' command, or the game failed: see logs/boot.log)"
        GAME=0; return 1
      fi
      nap 10
      game_running || { res FAIL "6-start-$1" "the game ended by itself"; GAME=0; return 1; }
      [ "$1" != A ] || check_clusters 6-game-clock "in the game" play
      capture "$1"
      [ "$3" != twice ] || { nap 2; capture "${1}2"; }
      send "$CORECMD" quit nosave
      waitfor 20 '! game_running' || { res FAIL "6-end-$1" "the game did not end within 20 s"; return 1; }
      GAME=0
      nap 3
      return 0
    }
    if run_game A '{"scaling":"native","effect":"none","sharpness":"sharp","show_fps":"off","rumble":"off"}' twice; then
      cap_ok=1
      run_game B '{"scaling":"fullscreen","effect":"none","sharpness":"sharp","show_fps":"off","rumble":"off"}'
      run_game C '{"scaling":"native","effect":"scanlines","sharpness":"sharp","show_fps":"off","rumble":"off"}'
      run_game D '{"scaling":"native","effect":"none","sharpness":"sharp","show_fps":"fps","rumble":"off"}'
      post=$(cluster_dump | tr '\n' ';')
      if [ "$post" = "$pre" ]; then res PASS 6-end-clock "after the games the clock is as before: $post"
      else check_clusters 6-end-clock "after the games (before: $pre)" menu; fi
    fi
  fi
fi

# ============================== 7. the owner's files
guard_list "$WORK/guard.after"
if cmp -s "$WORK/guard.before" "$WORK/guard.after"; then res PASS 7-owner-files "Saves (md5) and Roms (name+size), $(wc -l <"$WORK/guard.after" | tr -d ' ') files, same as before"
else res FAIL 7-owner-files "CHANGED: $(diff "$WORK/guard.before" "$WORK/guard.after" | grep '^[<>]' | head -n 5 | tr '\n' ';')"; fi
rm -f "$WORK/guard.before" "$WORK/guard.after"
[ "$cap_ok" = 1 ] && echo "CAPS ready"
exit 0
