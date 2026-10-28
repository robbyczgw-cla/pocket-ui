#!/bin/sh
# Runs pocket-ui in sway in place of EmulationStation. Two ways to start it:
# - As the boot UI (POCKETUI_AS_ESSWAY=1, set by deploy/essway-pocket-ui.conf, installed by scripts/autostart.sh): the
#   launcher IS essway's process. It never stops or starts essway. When EmulationStation is to run, it execs ROCKNIX's
#   start_es.sh, so EmulationStation runs in the same unit; when EmulationStation ends, essway's Restart= starts the
#   launcher, and pocket-ui, again. The mark POCKETUI_ES_ONLY_FILE that `dev.sh stop` leaves makes it exec start_es.sh
#   at once.
# - By hand, detached from SSH: systemd-run --unit=pocketui /storage/pocket-ui/run-pocket-ui.sh
#   It stops the essway unit (EmulationStation) first and starts it again when it hands over to EmulationStation.
#
# pocket-ui's exit code tells the launcher what to do next (src/rescue.rs has the same numbers, and a test checks
# that they match):
#   0    start pocket-ui again (normally): Restart in the rescue page, the power menu's Restart pocket-ui, the command
#        FIFO's "quit", a closed window
#   75   an update was installed: start pocket-ui again
#   77   the player asked for EmulationStation (a dialog with a warning in pocket-ui): hand over to it
#   78   the player asked for the previous version (rescue page): switch to it, then start it normally
#   other, or no picture: a failed start (126/127: the program could not be run at all)
# Exit 0 never means EmulationStation (it did until 2026-10-23).
#
# Stages when pocket-ui keeps failing ("crashes" = a crash, or an end before its first picture):
#   1. A freshly installed version that fails while update-pending is there: back to `previous` at once (update.rs).
#   2. Restart, up to POCKETUI_RESTARTS times with a growing pause; a run that lasted POCKETUI_STABLE_SECS starts the
#      count afresh.
#   3. The last known good version: a version that ran POCKETUI_STABLE_SECS with a picture becomes `known-good` (a link
#      like `current`, kept on disk by update.rs and dev.sh). If `current` is another version, it is switched back to it
#      (and `rolled-back` names the one that failed).
#   4. Safe mode: `pocket-ui --safe`, the rescue page without covers, Curator, achievements, sync and other extras
#      (src/rescue.rs). POCKETUI_SAFE_REASON tells it why. Up to POCKETUI_SAFE_RESTARTS restarts.
#   5. EmulationStation, only if even that fails, if the program cannot be run (missing, not executable, exit
#      126/127) or sway does not answer. The reason goes to the log and to `es-fallback`, which pocket-ui shows once at
#      its next start.
# Black screen guard: a pocket-ui that has shown no picture POCKETUI_FIRST_FRAME_SECS after it started is ended and
# counts as a failed start. pocket-ui notes its first picture in POCKETUI_FRAME_FILE. Every helper the launcher calls
# runs under a time limit (`bounded`), so a hung swaymsg, systemctl or pocket-core never leaves the screen black.
# Before every restart and before EmulationStation, clean_up puts back what a pocket-ui that died may have left behind.
# It stands in for ROCKNIX's start_es.sh, so it also does what start_es.sh and es_settings do before the UI starts
# ("ROCKNIX's UI start" below; on ROCKNIX main that includes putting back the screen scaling a killed game left).
#
# Smooth fan (src/fan_curve.rs, docs/FAN-CURVE.md): while pocket-ui drives the fan, FAN_NOTE says how to give it back
# to the kernel. clean_up does that first thing; during a run, watch_fan does it when the curve's heartbeat (FAN_BEAT)
# stops moving; and a TERM or HUP to the launcher (shutdown, essway stopped) does it before the launcher ends.
#
# The log of this run is in RAM (/var/log). start_logd keeps a copy, the kernel's messages and a sensor sample on
# the card in /storage/.config/pocket-ui/logs, so that they survive a freeze (see start_logd).
#
# The POCKETUI_* paths and times below are for deploy/test-run-pocket-ui.sh; on the device they keep their defaults.

export XDG_RUNTIME_DIR=/var/run/0-runtime-dir
export WAYLAND_DISPLAY=wayland-1
export SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock

ROOT=${POCKETUI_ROOT:-/storage/pocket-ui}
LOG=${POCKETUI_LOG:-/var/log/pocket-ui.log}
# Brightness saved by src/sleep.rs while the screen is dark, and the process group of the running game
# (src/control.rs, note_game).
DARK_FILE=${POCKETUI_DARK_FILE:-/tmp/pocket-ui.brightness}
GAME_FILE=${POCKETUI_GAME_FILE:-/tmp/pocket-ui.game}
QUANTUM=${POCKETUI_QUANTUM:-/usr/sbin/quantum}
RESTARTS=${POCKETUI_RESTARTS:-3}
SAFE_RESTARTS=${POCKETUI_SAFE_RESTARTS:-2}
RESTART_DELAY=${POCKETUI_RESTART_DELAY:-1}
STABLE_SECS=${POCKETUI_STABLE_SECS:-120}
# The black screen guard. Generous: a first start after an update reads the whole library before its first picture.
FIRST_FRAME_SECS=${POCKETUI_FIRST_FRAME_SECS:-45}
FRAME_FILE=${POCKETUI_FRAME_FILE:-/tmp/pocket-ui.frame}
# The process id of the running pocket-ui, for the black screen guard.
UI_PID_FILE=${POCKETUI_UI_PID_FILE:-/tmp/pocket-ui.pid}
# Time limit of one helper call (swaymsg, systemctl, quantum); pocket-core --restore-system gets twice as long.
HELPER_SECS=${POCKETUI_HELPER_SECS:-10}
# How long the game's emulators get to end on SIGTERM, and runemu.sh to clean up after them.
GAME_GRACE=${POCKETUI_GAME_GRACE:-15}
# How long to wait for sway to take connections before the first start (at boot it comes up after essway).
SWAY_WAIT=${POCKETUI_SWAY_WAIT:-30}
# Read before dev.env, which must not change them.
AS_ESSWAY=${POCKETUI_AS_ESSWAY:-}
START_ES=${POCKETUI_START_ES:-/usr/bin/start_es.sh}
ES_ONLY_FILE=${POCKETUI_ES_ONLY_FILE:-/tmp/pocket-ui.es-only}
# Smooth fan's note and heartbeat (src/fan_curve.rs NOTE_FILE, BEAT_FILE), and the watchdog: a look every
# FAN_CHECK_SECS, the fan handed back after FAN_STALE_CHECKS looks without a new beat.
FAN_NOTE=${POCKETUI_FAN_NOTE:-/tmp/pocket-ui-fan-curve.restore}
FAN_BEAT=${POCKETUI_FAN_BEAT:-/tmp/pocket-ui-fan.beat}
FAN_CHECK_SECS=${POCKETUI_FAN_CHECK_SECS:-2}
FAN_STALE_CHECKS=${POCKETUI_FAN_STALE_CHECKS:-3}

# ROCKNIX's UI start (rocknix_ui_env, rocknix_ui_chores, scanout_reset_stale below): its shell profile and functions,
# start_es.sh's settings file, system.cfg, the time zone files and the folder of compiled locales.
ROCKNIX_PROFILE=${POCKETUI_ROCKNIX_PROFILE:-/etc/profile}
ROCKNIX_FUNCTIONS=${POCKETUI_ROCKNIX_FUNCTIONS:-/etc/profile.d/001-functions}
ES_SETTINGS=${POCKETUI_ES_SETTINGS:-/usr/bin/es_settings}
SYSTEM_CFG=${POCKETUI_SYSTEM_CFG:-/storage/.config/system/configs/system.cfg}
TZ_CACHE=${POCKETUI_TZ_CACHE:-/storage/.cache}
LOCALE_DIR=${POCKETUI_LOCALE_DIR:-/storage/.config/emulationstation/locale}

# The exit codes above.
CODE_UPDATE=75
CODE_OPEN_ES=77
CODE_PREVIOUS=78

log() { echo "launcher: $*" >>"$LOG"; }

# Runs a helper ($2...) with a time limit of $1 seconds, then SIGKILL 2 s later (Sol finding SOL-07).
bounded() {
  secs=$1
  shift
  if command -v timeout >/dev/null 2>&1; then
    timeout -k 2 "$secs" "$@"
  else
    "$@"
  fi
}

# Essway mode: EmulationStation takes over this process, and so the essway unit. $1: why.
run_es() {
  log "$1, EmulationStation takes over"
  cd "${HOME:-/}" 2>/dev/null || cd /
  exec "$START_ES"
}

# EmulationStation without pocket-ui having asked for it: noted for pocket-ui's next start (`rescue::take_es_note`). $1: why.
es_fallback() {
  echo "$1" >"$ROOT/es-fallback" 2>/dev/null
  hand_over "$1"
}

# --- ROCKNIX's UI start ------------------------------------------------------------------------------------------
# essway.service runs start_es.sh, which loads es_settings and then starts EmulationStation. The launcher takes the
# place of start_es.sh, so it does what those two do before the UI comes up (docs/UPDATES.md, "ROCKNIX's UI start"):
#   es_settings (ROCKNIX 20261001 and later)
#     export SDL_AUDIODRIVER=pulseaudio            rocknix_ui_env (the value is read from es_settings)
#     system.language: LANG, LANGUAGE, LOCPATH     rocknix_ui_env; localedef and import-environment: rocknix_ui_chores
#     system.timezone: /storage/.cache/timezone,   rocknix_ui_chores, only when the files say another zone
#       system_timezone, restart tz-data
#     systemctl import-environment of 999-export   rocknix_ui_chores, in one systemctl call
#     set_kill set "emulationstation"              rocknix_ui_chores (runemu.sh sets the same after every game)
#   start_es.sh (ROCKNIX main after 20261001, the 075-scanout-offload quirk)
#     prerotate_env <transform> <mode's width>     prerotate_start/prerotate_take: pocket-ui and what it starts get the
#                                                  frontend's pre-rotation, as EmulationStation and its games did
#     scanout_reset_stale                          scanout_reset_stale: before the first start and in clean_up, so a
#                                                  game that was killed doesn't leave the screen scaling and sharpness
# A step whose function or file the running system doesn't have is skipped: an older or newer ROCKNIX loses only that
# step. Speed: ROCKNIX's profile is loaded (in bash, as its scripts do) only for a step that needs a function from it:
# on 20261001 never before the first picture. The chores wait for pocket-ui's first picture and run in the background.

# The shell functions ROCKNIX's 001-functions defines, as " name name ... " (rocknix_scan), and whether $1 is one.
ROCKNIX_FNS=
rocknix_has() {
  case $ROCKNIX_FNS in *" $1 "*) return 0 ;; esac
  return 1
}

# Runs shell code $1 in bash with ROCKNIX's profile loaded (its functions and the device's variables, as runemu.sh
# has them), under the helpers' time limit.
in_rocknix() {
  command -v bash >/dev/null 2>&1 || return 127
  bounded "$HELPER_SECS" bash -c ". \"\$0\" >/dev/null 2>&1 </dev/null; $1" "$ROCKNIX_PROFILE"
}

# A system.cfg value as get_setting reads a key without a system: the first `key=` line.
cfg_value() {
  awk -v k="$1=" 'index($0, k) == 1 { print substr($0, length(k) + 1); exit }' "$SYSTEM_CFG" 2>/dev/null
}

# One look at ROCKNIX's files before the first start, in one awk for speed (each program started costs about half a
# millisecond): every function 001-functions defines (`name()`, `function name()`, `function name {`), its
# SCANOUT_OWNER_FILE, es_settings' SDL_AUDIODRIVER and system.cfg's system.language. Files that aren't there are left
# out (BusyBox awk stops at a missing one).
rocknix_scan() {
  set --
  for f in "$ROCKNIX_FUNCTIONS" "$ES_SETTINGS" "$SYSTEM_CFG"; do [ -f "$f" ] && set -- "$@" "$f"; done
  [ $# -gt 0 ] || return 0
  awk -v fns="$ROCKNIX_FUNCTIONS" -v es="$ES_SETTINGS" -v cfg="$SYSTEM_CFG" '
    FILENAME == fns && (/^[ \t]*function[ \t]+[A-Za-z_]/ || /^[ \t]*[A-Za-z_][A-Za-z0-9_]*[ \t]*\(\)/) {
      l = $0; sub(/^[ \t]*(function[ \t]+)?/, "", l); sub(/[^A-Za-z0-9_].*$/, "", l); print "fn " l
    }
    FILENAME == fns && /^SCANOUT_OWNER_FILE=/ { print "owner " substr($0, 20) }
    FILENAME == es && /^[ \t]*export[ \t]+SDL_AUDIODRIVER=/ { v = $0; sub(/^.*SDL_AUDIODRIVER=/, "", v); sub(/[ \t]+$/, "", v); print "sdl " v }
    FILENAME == cfg && index($0, "system.language=") == 1 && !lang { lang = 1; print "lang " substr($0, 17) }
  ' "$@" 2>/dev/null
}

# The file runemu.sh writes the process id of the game that changed the screen's scaling into (ROCKNIX main:
# SCANOUT_OWNER_FILE in 001-functions). Empty when the running system has no scanout_reset_stale.
SCANOUT_OWNER=

# start_es.sh's scanout_reset_stale: a game whose runemu.sh did not get to put the scaling and sharpness back
# (pocket-ui crashed, the game was killed) leaves its id in SCANOUT_OWNER; unless that runemu.sh still runs, ROCKNIX
# resets the screen to scale 1 and the display controller's sharpening to its defaults. Costs nothing without the file.
scanout_reset_stale() {
  [ -n "$SCANOUT_OWNER" ] && [ -f "$SCANOUT_OWNER" ] || return 0
  owner=$(cat "$SCANOUT_OWNER" 2>/dev/null)
  in_rocknix scanout_reset_stale
  [ -f "$SCANOUT_OWNER" ] || log "screen scaling and sharpness put back (left by process $owner)"
}

# es_settings' environment, for pocket-ui and everything it starts (EmulationStation passed it on to its games), and
# what the other steps need to know of ROCKNIX's files (rocknix_scan).
rocknix_ui_env() {
  sdl= owner= UI_LANG=
  scan=$(rocknix_scan)
  while IFS= read -r line; do
    case $line in
      "fn "*) ROCKNIX_FNS="$ROCKNIX_FNS ${line#fn } " ;;
      "owner "*) [ -n "$owner" ] || owner=${line#owner } ;;
      "sdl "*) sdl=${line#sdl } ;;
      "lang "*) UI_LANG=${line#lang } ;;
    esac
  done <<EOF
$scan
EOF
  if rocknix_has scanout_reset_stale; then
    SCANOUT_OWNER=${POCKETUI_SCANOUT_OWNER:-${owner:-/run/rocknix/scanout-owner}}
  fi
  case $sdl in '' | *[!A-Za-z0-9_,-]*) ;; *) export SDL_AUDIODRIVER="$sdl" ;; esac
  # The language EmulationStation was set to (pocket-ui sets none; system.language stays from EmulationStation).
  case $UI_LANG in
    '') ;;
    *[!A-Za-z0-9_@.-]*)
      log "system.language \"$UI_LANG\" is not a locale name, ignored"
      UI_LANG=
      ;;
    *)
      unset I18NPATH
      export LANG="$UI_LANG.UTF-8" LANGUAGE="$UI_LANG.UTF-8" LOCPATH="$LOCALE_DIR"
      ;;
  esac
}

# start_es.sh's pre-rotation (ROCKNIX main): where the display plane can't rotate the panel's mode, the GPU rotates
# each frame before scanout, and two Mesa variables say by how much. Worked out by ROCKNIX's own prerotate_env, in the
# background while the launcher waits for sway; prerotate_take exports the result before the first start.
PREROTATE_PID=
PREROTATE_FILE=${TMPDIR:-/tmp}/pocket-ui.prerotate.$$
prerotate_start() {
  rocknix_has prerotate_env || return 0
  in_rocknix 'mode=$(cat /sys/class/drm/card*-"${WLR_CON}"/modes 2>/dev/null | head -n 1)
    prerotate_env "${WLR_CON_TRANSFORM}" "${mode%%x*}"
    echo "vk=${vk_wsi_wayland_prerotate}"
    echo "egl=${egl_wayland_prerotate}"' >"$PREROTATE_FILE" 2>/dev/null &
  PREROTATE_PID=$!
}
prerotate_take() {
  [ -n "$PREROTATE_PID" ] || return 0
  wait "$PREROTATE_PID" 2>/dev/null
  PREROTATE_PID=
  vk=$(sed -n 's/^vk=\([0-9]\{1,3\}\)$/\1/p' "$PREROTATE_FILE" 2>/dev/null)
  egl=$(sed -n 's/^egl=\([0-9]\{1,3\}\)$/\1/p' "$PREROTATE_FILE" 2>/dev/null)
  rm -f "$PREROTATE_FILE"
  unset vk_wsi_wayland_prerotate egl_wayland_prerotate
  [ -n "$vk" ] && export vk_wsi_wayland_prerotate="$vk"
  [ -n "$egl" ] && export egl_wayland_prerotate="$egl"
  log "pre-rotation: ${vk:-none} (ROCKNIX's prerotate_env)"
}

# es_settings' system-wide steps, in the background once pocket-ui shows its first picture (FRAME_FILE), or after
# FIRST_FRAME_SECS: none of them is needed for the start, and each costs a systemctl call or more.
rocknix_ui_chores() {
  i=0
  while [ ! -f "$FRAME_FILE" ] && [ $i -lt $((FIRST_FRAME_SECS * 5)) ]; do
    sleep 0.2
    i=$((i + 1))
  done
  # The time zone files the boot and tz-data.service read, when they say another zone than system.timezone.
  # (pocket-ui's own time zone page goes through steamos-set-timezone, which keeps all three the same.)
  tz=$(cfg_value system.timezone)
  case $tz in '' | *[!A-Za-z0-9_+/-]*) tz= ;; esac
  if [ -n "$tz" ] && [ "$(cat "$TZ_CACHE/timezone" 2>/dev/null)" != "TIMEZONE=$tz" ]; then
    printf 'TIMEZONE=%s' "$tz" >"$TZ_CACHE/timezone" && printf '%s' "$tz" >"$TZ_CACHE/system_timezone" &&
      bounded "$HELPER_SECS" systemctl restart tz-data.service >/dev/null 2>&1 && log "time zone files set to $tz (system.timezone)"
  fi
  # The language's compiled locale, made once, and the language for the services systemd starts.
  if [ -n "$UI_LANG" ]; then
    if [ ! -e "$LOCPATH/$LANG/LC_NAME" ] && command -v localedef >/dev/null 2>&1; then
      I18NPATH=/usr/share/i18n bounded 120 localedef -i "$UI_LANG" -c -v -f UTF-8 "$LOCPATH/$LANG" >/dev/null 2>&1
      log "locale $LANG compiled for system.language"
    fi
    bounded "$HELPER_SECS" systemctl import-environment LANG LOCPATH LANGUAGE >/dev/null 2>&1
  fi
  # The device's quirk variables (999-export) for systemd's services, and the 3-button kill chord's target as
  # EmulationStation leaves it (it ends nothing under pocket-ui; runemu.sh sets the same after every game).
  exports=${ROCKNIX_FUNCTIONS%/*}/999-export
  if [ -f "$exports" ] || rocknix_has set_kill; then
    in_rocknix 'vars=
      [ -f "'"$exports"'" ] && for v in $(grep _ "'"$exports"'" | sed "s/export//g; s/^\W//g; s/\\\\//g"); do
        [[ $v =~ ^[A-Za-z_][A-Za-z0-9_]*$ ]] && [ -n "${!v+x}" ] && vars="$vars $v"
      done
      [ -n "$vars" ] && systemctl import-environment $vars
      declare -F set_kill >/dev/null && set_kill set "emulationstation"
      true' >/dev/null 2>&1
  fi
}

if [ "$AS_ESSWAY" = 1 ]; then
  [ -f "$ES_ONLY_FILE" ] && run_es "$ES_ONLY_FILE is there (dev.sh stop)"
  cd "$ROOT" 2>/dev/null || run_es "no $ROOT"
else
  cd "$ROOT" || exit 1
fi

# Tests: extra environment for pocket-ui, one VAR=value per line (e.g. POCKETUI_SLEEP_MINUTE_SECS=10, see
# src/sleep.rs). Not there in normal use. Read as data, never run: only POCKETUI_*, POCKET_* and GALLIUM_*
# names, a value in one pair of quotes loses them, and everything else on a line stays literal.
load_env() {
  while IFS= read -r line || [ -n "$line" ]; do
    case $line in '' | '#'*) continue ;; esac
    key=${line%%=*}
    val=${line#*=}
    case $key in
      "$line" | *[!A-Za-z0-9_]* | [0-9]*) log "dev.env: ignored a line that is not KEY=VALUE"; continue ;;
      POCKETUI_* | POCKET_* | GALLIUM_*) ;;
      *) log "dev.env: ignored $key (only POCKETUI_*, POCKET_* and GALLIUM_*)"; continue ;;
    esac
    case $val in
      \"*\") val=${val#\"}; val=${val%\"} ;;
      \'*\') val=${val#\'}; val=${val%\'} ;;
    esac
    export "$key=$val"
  done <"$1"
}
[ -f "$ROOT/dev.env" ] && load_env "$ROOT/dev.env"

# Keeps the logs on the card, where a freeze or a watchdog reboot cannot take them (ROCKNIX's /var/log is RAM): a
# copy of $LOG, the kernel's messages and a sample of the sensors every 5 s, three boots' worth, see src/logs.rs
# and docs/user/troubleshooting.md. It is `pocket-core --log-daemon` under the name pocket-logd: a hard link next to
# the real pocket-core, made here. (A symbolic link is not enough: BusyBox's pidof also goes by the program file, and
# `pidof pocket-core` must keep meaning "a game is running".) A logger that cannot start costs nothing else.
LOGD_PID=
start_logd() {
  [ -x "$ROOT/pocket-core" ] || return 0
  # pocket-core is a link into current/, which is a link into releases/<version>/: the link goes where the file is.
  real=$(readlink -f "$ROOT/pocket-core" 2>/dev/null)
  [ -n "$real" ] || real=$ROOT/pocket-core
  logd=${real%/*}/pocket-logd
  if ! [ "$logd" -ef "$real" ]; then
    rm -f "$logd"
    ln "$real" "$logd" 2>/dev/null || cp "$real" "$logd" 2>/dev/null
  fi
  [ -x "$logd" ] || return 0
  "$logd" --log-daemon >>"$LOG" 2>&1 &
  LOGD_PID=$!
}
# Asks it to stop (it fsyncs and notes a clean exit), and waits for it.
stop_logd() {
  [ -n "$LOGD_PID" ] || return 0
  kill "$LOGD_PID" 2>/dev/null
  wait "$LOGD_PID" 2>/dev/null
  LOGD_PID=
}

# Gives the fan back to the kernel as FAN_NOTE says (the same steps as fan_curve::hand_back): the PWM parked at the
# lowest level, the six trips' own temperatures back (the kernel then sets the level they ask for), and the fan off if
# its zone is cooler than the lowest trip. Only files below <sysfs root>/sys are written. $1: why, for the log.
# Platform safety also covers stale notes copied from another card: never write SM8550 fan files.
sm8550() {
  case ${POCKETUI_DEVICE:-} in rp6|nova|odin2|odin2mini|odin2portal|"AYN Odin 2"*|"Retroid Pocket 6"*|"Retroid Pocket Nova"*) return 0 ;; esac
  grep -q 'qcom,sm8550' /sys/firmware/devicetree/base/compatible 2>/dev/null ||
    grep -q 'HW_DEVICE=.*SM8550' /etc/os-release 2>/dev/null
}
fan_handback() {
  sm8550 && return 0
  [ -f "$FAN_NOTE" ] || return 0
  sys=${POCKETUI_FAN_ROOT:-}
  sys=${sys%/}/sys/
  while read -r what a b c; do
    case $a in "$sys"*) ;; *) continue ;; esac
    case $a in *..*) continue ;; esac
    case $what in
      park)
        now=$(cat "$a" 2>/dev/null)
        case $now$b in *[!0-9]*) continue ;; esac
        [ -n "$now" ] && [ "$now" -gt "$b" ] && echo "$b" >"$a" 2>/dev/null
        ;;
      trip)
        case $b in '' | *[!0-9]*) continue ;; esac
        echo "$b" >"$a" 2>/dev/null
        ;;
      off-below)
        t=$(cat "$a" 2>/dev/null)
        case $t in '' | *[!0-9]*) continue ;; esac
        case $b in '' | *[!0-9]*) continue ;; esac
        case $c in "$sys"*) ;; *) continue ;; esac
        [ "$t" -lt "$b" ] && echo 0 >"$c" 2>/dev/null
        ;;
    esac
  done <"$FAN_NOTE"
  rm -f "$FAN_NOTE"
  log "fan: back to ROCKNIX's control ($1)"
}

# Smooth fan's watchdog, in the background during a run: while the note exists, the heartbeat's count must move.
watch_fan() {
  last= same=0
  while :; do
    sleep "$FAN_CHECK_SECS"
    if [ ! -f "$FAN_NOTE" ]; then
      last= same=0
      continue
    fi
    beat=$(cat "$FAN_BEAT" 2>/dev/null)
    beat=${beat%% *}
    if [ -n "$beat" ] && [ "$beat" != "$last" ]; then
      last=$beat same=0
      continue
    fi
    same=$((same + 1))
    if [ $same -ge "$FAN_STALE_CHECKS" ]; then
      fan_handback "the fan curve stopped answering"
      last= same=0
    fi
  done
}

# Processes of group $1, without zombies; with $2 = spare, not runemu.sh itself.
group_members() {
  for stat in /proc/[0-9]*/stat; do
    s=$(cat "$stat" 2>/dev/null) || continue
    rest=${s##*) }
    # state ppid pgrp ...
    # shellcheck disable=SC2086
    set -- $rest
    pid=${stat#/proc/}
    pid=${pid%/stat}
    [ "$1" = Z ] && continue
    [ "$3" = "$group" ] || continue
    if [ "$spare" = spare ] && [ "$(cat "/proc/$pid/comm" 2>/dev/null)" = runemu.sh ]; then
      continue
    fi
    echo "$pid"
  done
}

# Waits up to GAME_GRACE seconds for group_members to come back empty.
wait_group() {
  i=0
  while [ -n "$(group_members)" ] && [ $i -lt $((GAME_GRACE * 5)) ]; do
    sleep 0.2
    i=$((i + 1))
  done
  [ -z "$(group_members)" ]
}

# The game pocket-ui was running when it died: ends its emulators (SIGCONT first, the pause menu and light
# sleep freeze them) and waits for runemu.sh, which puts back the governor, GPU level and CPU cores itself.
stop_game() {
  group=$(cat "$GAME_FILE" 2>/dev/null)
  rm -f "$GAME_FILE"
  case $group in '' | *[!0-9]* | 0 | 1) return ;; esac
  kill -CONT -- "-$group" 2>/dev/null || return
  log "stopping the game left running (process group $group)"
  spare=spare
  for pid in $(group_members); do kill -TERM "$pid" 2>/dev/null; done
  if ! wait_group; then
    log "the game ignored SIGTERM, killing it"
    for pid in $(group_members); do kill -KILL "$pid" 2>/dev/null; done
  fi
  spare=
  wait_group || log "runemu.sh still hasn't finished after ${GAME_GRACE} s"
}

# Puts back what a pocket-ui that ended (crashed, or killed while the screen was dark) may have left behind.
clean_up() {
  # The fan first: the rest can take a while (a game ending).
  fan_handback "pocket-ui ended"
  if [ -f "$DARK_FILE" ]; then
    level=$(cat "$DARK_FILE" 2>/dev/null)
    backlight=${POCKETUI_BACKLIGHT:-$(for d in /sys/class/backlight/*; do [ -f "$d/brightness" ] && echo "$d" && break; done)}
    case $level in
      '' | *[!0-9]* | 0) ;;
      *)
        bounded "$HELPER_SECS" swaymsg output '*' power on >/dev/null 2>&1
        [ -n "$backlight" ] && echo "$level" >"$backlight/brightness" && log "screen back on at brightness $level"
        ;;
    esac
    rm -f "$DARK_FILE"
  fi
  stop_game
  # ROCKNIX's own launches (from EmulationStation) run in games.slice.
  if bounded "$HELPER_SECS" systemctl -q is-active games.slice 2>/dev/null; then
    bounded "$HELPER_SECS" systemctl kill -s CONT games.slice
    bounded "$HELPER_SECS" systemctl stop games.slice
  fi
  # CPU and fan settings of a pocket-core that was killed, and PipeWire's quantum it lowers while playing.
  [ -x "$ROOT/pocket-core" ] && bounded $((HELPER_SECS * 2)) "$ROOT/pocket-core" --restore-system >>"$LOG" 2>&1
  if [ -x "$QUANTUM" ]; then
    bounded "$HELPER_SECS" "$QUANTUM" >/dev/null 2>&1
  else
    bounded "$HELPER_SECS" pw-metadata -n settings 0 clock.force-quantum 0 >/dev/null 2>&1
  fi
  # After stop_game: a runemu.sh that ended normally has put the screen back itself.
  scanout_reset_stale
}

# Hands the screen to EmulationStation: in essway mode by exec, by hand by starting the essway unit. $1: why.
hand_over() {
  clean_up
  [ "$AS_ESSWAY" = 1 ] && run_es "$1"
  log "$1, EmulationStation comes back"
  stop_logd
  bounded 30 systemctl start essway
  exit 0
}

sway_answers() { bounded 2 swaymsg -t get_version >/dev/null 2>&1; }

# A freshly installed version that failed goes back to the previous one.
can_roll_back() { [ -f update-pending ] && [ -L previous ]; }
roll_back() {
  echo "update $(cat update-pending) failed, back to $(readlink previous)" >>"$LOG"
  cat update-pending >rolled-back
  ln -sfn "$(readlink previous)" current
  rm -f update-pending
}

# The release `known-good` points at, if it is not `current` and has a pocket-ui (src/rescue.rs, other_version, is
# the same rule).
other_good() {
  [ -L known-good ] && [ -L current ] || return 1
  g=$(readlink known-good)
  [ "$g" != "$(readlink current)" ] && [ -x "$g/pocket-ui" ] && echo "$g"
}
# The same for `previous`.
other_previous() {
  [ -L previous ] && [ -L current ] || return 1
  p=$(readlink previous)
  [ "$p" != "$(readlink current)" ] && [ -x "$p/pocket-ui" ] && echo "$p"
}
# Points `current` at release $1; $2: the version that failed, for `rolled-back` (empty: the player chose it).
use_release() {
  failed=$2
  [ -n "$failed" ] && echo "$failed" >rolled-back
  ln -sfn "$1" current
  rm -f update-pending
}

# Watches a run of pocket-ui (in the background): no picture after FIRST_FRAME_SECS ends it (the black screen guard);
# a run with a picture that lasts STABLE_SECS makes its version the known good one. $1: --safe in safe mode.
watch_run() {
  i=0
  while :; do
    sleep 1
    i=$((i + 1))
    pid=$(cat "$UI_PID_FILE" 2>/dev/null)
    case $pid in '' | *[!0-9]*) continue ;; esac
    kill -0 "$pid" 2>/dev/null || return 0
    if [ ! -f "$FRAME_FILE" ]; then
      if [ $i -ge "$FIRST_FRAME_SECS" ]; then
        log "no picture ${FIRST_FRAME_SECS} s after the start (black screen): ending pocket-ui"
        kill -TERM "$pid" 2>/dev/null
        j=0
        while kill -0 "$pid" 2>/dev/null && [ $j -lt 15 ]; do sleep 0.2; j=$((j + 1)); done
        kill -KILL "$pid" 2>/dev/null
        return 0
      fi
    elif [ $i -ge "$STABLE_SECS" ]; then
      if [ "$1" != --safe ] && [ -L current ]; then
        cur=$(readlink current)
        if [ "$(readlink known-good 2>/dev/null)" != "$cur" ]; then
          ln -sfn "$cur" known-good && log "${cur#releases/} ran ${STABLE_SECS} s: it is the known good version now"
        fi
      fi
      return 0
    fi
  done
}

# One run of pocket-ui ($@: its arguments), in the foreground (a background job would start with SIGINT and SIGQUIT
# ignored, and pass that on to every game). Sets code, framed (1 when it showed a picture) and ran (seconds).
run_ui() {
  rm -f "$FRAME_FILE" "$UI_PID_FILE"
  watch_run "$@" &
  watcher=$!
  watch_fan &
  fan_watcher=$!
  started=$(date +%s)
  POCKETUI_FRAME_FILE=$FRAME_FILE sh -c 'echo $$ >"$1"; shift; exec "$@"' run-ui "$UI_PID_FILE" ./pocket-ui "$@" >>"$LOG" 2>&1
  code=$?
  ran=$(($(date +%s) - started))
  kill "$watcher" "$fan_watcher" 2>/dev/null
  wait "$watcher" 2>/dev/null
  wait "$fan_watcher" 2>/dev/null
  framed=0
  [ -f "$FRAME_FILE" ] && framed=1
  rm -f "$UI_PID_FILE"
  echo "exit code $code" >>"$LOG"
}

if [ "$AS_ESSWAY" = 1 ]; then
  # An update without a pocket-ui goes back to the previous version, as one that crashes would.
  [ ! -x pocket-ui ] && can_roll_back && roll_back
  # A version without a pocket-ui (a half-finished copy): the known good one, if there is one.
  if [ ! -x pocket-ui ] && good=$(other_good); then
    log "no pocket-ui in $(readlink current), back to ${good#releases/}"
    use_release "$good" "$(readlink current | sed 's#^releases/##')"
  fi
  if [ ! -x pocket-ui ]; then
    echo "pocket-ui is missing or can't be run (no pocket-ui to start in $ROOT)" >"$ROOT/es-fallback" 2>/dev/null
    run_es "no pocket-ui to start in $ROOT"
  fi
else
  bounded 30 systemctl stop essway
fi
: >"$LOG"
# Shutdown or essway stopped: the fan back to the kernel before the launcher goes (after pocket-ui, which gets the
# same signal, has ended).
trap 'fan_handback "the launcher was stopped"; exit 143' TERM HUP
# After the log is empty (it copies from the start) and dev.env is read (POCKETUI_DATA). In essway mode it stays
# in the unit when EmulationStation takes over, and ends with the unit: shutdown, or EmulationStation ending.
start_logd
# ROCKNIX's UI start (above): the environment now, the pre-rotation while sway comes up, the chores after the first
# picture.
rocknix_ui_env
prerotate_start
# At boot essway starts as soon as sway.service has, before sway takes connections: pocket-ui would die with
# NoCompositor. Wait until sway answers (POCKETUI_SWAY_WAIT seconds at most, by the clock: a swaymsg that hangs
# costs its time limit, not more), then start it either way. Asked every 50 ms (it was 250): the shelf comes up as
# soon after sway as it can (instant start, docs/DEVELOPING.md).
deadline=$(($(date +%s) + SWAY_WAIT))
until sway_answers; do
  if [ "$(date +%s)" -ge "$deadline" ]; then
    log "sway not ready after ${SWAY_WAIT} s, starting pocket-ui anyway"
    break
  fi
  sleep 0.05
done
prerotate_take
# A game killed before this launcher started (the last one crashed, essway was restarted) may have left the scaling.
scanout_reset_stale
rocknix_ui_chores &

stage=normal
crashes=0
while :; do
  if [ "$stage" = safe ]; then
    run_ui --safe
  else
    run_ui
  fi
  case $code in
    "$CODE_UPDATE")
      clean_up
      continue
      ;;
    "$CODE_OPEN_ES")
      hand_over "pocket-ui asked for EmulationStation (exit $code)"
      ;;
    "$CODE_PREVIOUS")
      clean_up
      if rel=$(other_good) || rel=$(other_previous); then
        log "switching to ${rel#releases/}, as asked"
        use_release "$rel" ""
      else
        log "asked for the previous version, but there is none: starting this one again"
      fi
      stage=normal
      unset POCKETUI_SAFE_REASON
      crashes=0
      continue
      ;;
    0)
      if [ $framed = 1 ]; then
        # A restart that was asked for (the rescue page's "Restart normally", the command FIFO's quit).
        clean_up
        stage=normal
        unset POCKETUI_SAFE_REASON
        crashes=0
        continue
      fi
      ;;
  esac
  # A failed start: a crash, or an end before the first picture.
  if [ $framed = 1 ]; then why="exit $code"; else why="exit $code, no picture"; fi
  if [ "$stage" = normal ] && can_roll_back; then
    roll_back
    clean_up
    continue
  fi
  # Nothing pocket-ui can do about a sway that is gone.
  if [ $framed = 0 ] && ! sway_answers; then
    es_fallback "pocket-ui could not start: sway does not answer ($why)"
  fi
  [ "$ran" -ge "$STABLE_SECS" ] && crashes=0
  crashes=$((crashes + 1))
  limit=$RESTARTS
  [ "$stage" = safe ] && limit=$SAFE_RESTARTS
  # 126/127: the program cannot be run (a library missing after a system update); trying it again won't help.
  case $code in 126 | 127) limit=0 ;; esac
  if [ $crashes -le "$limit" ]; then
    delay=$((RESTART_DELAY << (crashes - 1)))
    if [ "$stage" = safe ]; then
      log "pocket-ui stopped in safe mode ($why), restart $crashes of $limit in $delay s"
    else
      log "pocket-ui crashed ($why), restart $crashes of $limit in $delay s"
    fi
    clean_up
    sleep "$delay"
    continue
  fi
  clean_up
  if [ $crashes = 1 ]; then times=once; else times="$crashes times in a row"; fi
  crashes=0
  if [ "$stage" = normal ] && good=$(other_good); then
    log "pocket-ui failed $times ($why): back to the known good version ${good#releases/}"
    use_release "$good" "$(readlink current | sed 's#^releases/##')"
    continue
  fi
  if [ "$stage" = normal ]; then
    log "pocket-ui failed $times ($why): starting it in safe mode"
    stage=safe
    export POCKETUI_SAFE_REASON="pocket-ui stopped $times ($why)."
    continue
  fi
  es_fallback "pocket-ui could not start, not even in safe mode ($why)"
done
