#!/bin/sh
# Tests for run-pocket-ui.sh: a fake pocket-ui that crashes, a fake game (a runemu.sh that cleans up after
# its emulator) and fake systemctl, swaymsg, quantum and pocket-core that only log what they were asked.
# Every stage of the launcher has its tests: restart, rollback of an update, the last known good version, safe mode,
# EmulationStation only when pocket-ui can't start or was asked to, the black screen guard, the helpers' time limits.
# ROCKNIX's UI start runs against a fake ROCKNIX profile: 20261001's steps, and ROCKNIX main's pre-rotation and scanout
# reset with the functions copied from ROCKNIX.
#   sh deploy/test-run-pocket-ui.sh
# Needs Linux (/proc). Elsewhere it runs itself in the virtual device's base image, which has ROCKNIX's
# shell and BusyBox (docs/VIRTUAL-DEVICE.md; POCKETUI_TEST_IMAGE picks another image).
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
if [ ! -r /proc/self/stat ]; then
  exec docker run --rm --init -v "$HERE:/deploy:ro" "${POCKETUI_TEST_IMAGE:-pocket-vdev-base:20261001}" /bin/sh /deploy/test-run-pocket-ui.sh
fi
# The device's /bin/sh is bash (ROCKNIX); the launcher relies on it, e.g. `kill -CONT -- -<group>`. A test machine
# whose /bin/sh is dash (Debian, Ubuntu) runs the launcher, the fakes and these tests in bash instead: dash's kill
# takes no `--` (and BusyBox ash's neither). POCKETUI_TEST_SH picks another shell.
TEST_SH=${POCKETUI_TEST_SH:-/bin/sh}
if [ -z "${POCKETUI_TEST_SH:-}" ] && [ "$(basename "$(readlink -f /bin/sh)")" = dash ] && command -v bash >/dev/null; then
  TEST_SH=$(command -v bash)
fi
if [ "${POCKETUI_TEST_SH_RUNNING:-}" != "$TEST_SH" ]; then
  POCKETUI_TEST_SH_RUNNING=$TEST_SH exec "$TEST_SH" "$0" "$@"
fi
# Nothing of the test machine's language: on the device the launcher starts without LANG (systemd sets none).
unset LANG LANGUAGE LC_ALL LOCPATH I18NPATH
LAUNCHER=$HERE/run-pocket-ui.sh
fails=0

check() {
  if eval "$2"; then
    echo "ok   $1"
  else
    echo "FAIL $1"
    fails=$((fails + 1))
  fi
}

# A fresh fake installation in $T. $1: the pocket-ui runs, one word each: an exit code, optionally with ":noframe"
# (it ends before its first picture), ":<seconds>" (it runs that long after its picture), or "hang" (no picture, never
# ends by itself). After the list every run exits 77 (the player asks for EmulationStation), which ends the launcher.
setup() {
  end_groups
  # The previous scenario's folder is done with (every scenario made its own and none was removed: tens of folders
  # in /tmp per run).
  drop_scratch
  T=$(mktemp -d)
  ROOT=$T/root
  mkdir -p "$ROOT" "$T/bin" "$T/backlight"
  printf '%s\n' $1 >"$T/plan"
  : >"$T/calls"
  echo 0 >"$T/backlight/brightness"
  cat >"$T/bin/systemctl" <<EOF
#!$TEST_SH
echo "systemctl \$*" >>"$T/calls"
[ "\$1" != is-active ]
EOF
  cat >"$T/bin/swaymsg" <<EOF
#!$TEST_SH
echo "swaymsg \$*" >>"$T/calls"
EOF
  # ROCKNIX's EmulationStation: notes its process id, to show that the launcher exec'd it.
  cat >"$T/bin/start_es.sh" <<EOF
#!$TEST_SH
echo start_es.sh >>"$T/calls"
echo \$\$ >"$T/es-pid"
EOF
  cat >"$T/bin/quantum" <<EOF
#!$TEST_SH
echo quantum >>"$T/calls"
EOF
  # --log-daemon is the logger (src/logs.rs): it stays until it is asked to stop, like the real one, and notes
  # the name it was started under ($0 is the link pocket-logd) and what it sees of dev.env.
  cat >"$ROOT/pocket-core" <<EOF
#!$TEST_SH
if [ "\$1" = --log-daemon ]; then
  echo "logd \$0 \$*" >>"$T/calls"
  trap 'echo logd-term >>"$T/calls"; exit 0' TERM
  env | grep -E '^POCKETUI_TEST_' | sort >"$T/logd-env"
  [ -f "$T/logd-fails" ] && exit 1
  echo \$\$ >"$T/logd-pid"
  while :; do sleep 0.2; done
fi
echo "pocket-core \$*" >>"$T/calls"
EOF
  # The fake game: runemu.sh runs the emulator in the foreground and cleans up once it has ended, like
  # ROCKNIX's (governor, GPU level, CPU cores). It has to be called runemu.sh: that is the name the launcher spares.
  cat >"$T/runemu.sh" <<EOF
#!$TEST_SH
"$T/emulator"
echo "cleanup after exit \$?" >"$T/runemu-cleanup"
EOF
  cat >"$T/emulator" <<EOF
#!$TEST_SH
trap 'echo term >"$T/emulator-term"; exit 0' TERM
touch "$T/emulator-up"
while :; do sleep 0.2; done
EOF
  # The fake pocket-ui: logs the run (with the version `current` points at and its arguments) and its POCKETUI_TEST_*
  # environment, starts the game on the first run when asked to (and freezes it as the pause menu does), notes its
  # first picture as pocket-ui does (POCKETUI_FRAME_FILE), then exits with the planned code.
  cat >"$T/fake-pocket-ui" <<EOF
#!$TEST_SH
n=\$((\$(cat "$T/runs" 2>/dev/null || echo 0) + 1))
echo \$n >"$T/runs"
echo "pocket-ui run \$n \$(readlink current 2>/dev/null) \$*" >>"$T/calls"
# A test's own steps for this run (\$n), e.g. Smooth fan's note and heartbeat.
[ -f "$T/hook" ] && . "$T/hook"
env | grep -E '^(POCKETUI_TEST_|LD_PRELOAD|POCKETUI_SAFE_REASON|SDL_AUDIODRIVER=|LANG=|LANGUAGE=|LOCPATH=|I18NPATH=|vk_wsi_wayland_prerotate=|egl_wayland_prerotate=)' | sort >"$T/env.\$n"
word=\$(sed -n "\${n}p" "$T/plan")
code=\${word%%:*}
flag=\${word#"\$code"}
flag=\${flag#:}
if [ "\$code" = hang ]; then
  echo \$\$ >"$T/hung-pid"
  while :; do sleep 0.2; done
fi
[ "\$flag" = noframe ] || [ -z "\$POCKETUI_FRAME_FILE" ] || : >"\$POCKETUI_FRAME_FILE"
case \$flag in [0-9]*) sleep "\$flag" ;; esac
if [ \$n = 1 ] && [ -f "$T/with-game" ]; then
  setsid "$T/runemu.sh" &
  pg=\$!
  i=0
  while [ ! -f "$T/emulator-up" ] && [ \$i -lt 50 ]; do sleep 0.1; i=\$((i + 1)); done
  echo \$pg >"$T/game"
  echo \$pg >"$T/game-pgid"
  kill -STOP -- -\$pg
fi
exit \${code:-77}
EOF
  # ROCKNIX's shell side as on 20261001: a profile that notes when it is loaded (in bash, by the launcher's UI-start
  # steps) and loads the functions and the device's variables; set_kill as the only function the launcher may call;
  # es_settings with its sound driver; an empty system.cfg. rocknix_main (below) adds what ROCKNIX main has.
  mkdir -p "$T/rocknix" "$T/cache" "$T/run" "$T/msm" "$T/locale"
  cat >"$T/rocknix/profile" <<EOF
echo "profile \$\$" >>"$T/calls"
. "$T/rocknix/001-functions"
[ -f "$T/rocknix/quirks" ] && . "$T/rocknix/quirks"
EOF
  cat >"$T/rocknix/001-functions" <<EOF
function set_kill() {
  echo "set_kill \$*" >>"$T/calls"
}
EOF
  printf '#!/bin/bash\n. /etc/profile\n\nexport SDL_AUDIODRIVER=pulseaudio\n' >"$T/rocknix/es_settings"
  : >"$T/system.cfg"
  cat >"$T/bin/localedef" <<EOF
#!$TEST_SH
echo "localedef \$* I18NPATH=\$I18NPATH" >>"$T/calls"
EOF
  chmod +x "$T/bin/"* "$ROOT/pocket-core" "$T/runemu.sh" "$T/emulator" "$T/fake-pocket-ui"
  ln -s "$T/fake-pocket-ui" "$ROOT/pocket-ui"
}

# ROCKNIX main (after 20261001): its prerotate_env and scanout functions, copied from 001-functions @ 8b70f69 with the
# run files in $T, and the device variables of the SM8250 quirk 075-scanout-offload. $1: DEVICE_PLANE_ROTATION.
rocknix_main() {
  cat >"$T/rocknix/quirks" <<EOF
DEVICE_PLANE_ROTATION="$1"
DEVICE_PRE_ROTATION="true"
DEVICE_SCANOUT_SCALING="true"
DEVICE_SCANOUT_SHARPNESS="$T/msm"
WLR_CON=DSI-1
WLR_CON_TRANSFORM=270
EOF
  cat >>"$T/rocknix/001-functions" <<'ROCKNIX_MAIN'

function plane_rotation() {
  [ "${DEVICE_PLANE_ROTATION:-false}" != "false" ] && [ ! -e /storage/.config/rotator.disabled ]
}

function plane_rotation_max_height() {
  case "${DEVICE_PLANE_ROTATION}" in
    ''|*[!0-9]*) ;;
    *) echo "${DEVICE_PLANE_ROTATION}" ;;
  esac
}

function plane_rotates() {
  local max
  plane_rotation || return 1
  case "${1}" in
    90|270)
      max=$(plane_rotation_max_height)
      [ -z "${max}" ] || { [ -n "${2}" ] && [ "${2}" -le "${max}" ]; }
      ;;
  esac
}

function prerotate_env() {
  unset vk_wsi_wayland_prerotate egl_wayland_prerotate
  [ "${DEVICE_PRE_ROTATION}" = "true" ] || return 0
  plane_rotates "${1}" "${2}" && return 0
  case "${1}" in
    90)  export vk_wsi_wayland_prerotate=270 egl_wayland_prerotate=270 ;;
    270) export vk_wsi_wayland_prerotate=90  egl_wayland_prerotate=90 ;;
    180) export vk_wsi_wayland_prerotate=180 egl_wayland_prerotate=180 ;;
  esac
  return 0
}

SCANOUT_HINT_FILE=@T@/run/fullscreen-scale-hint
SCANOUT_SCALED_FILE=@T@/run/scanout-scaled-outputs
SCANOUT_OWNER_FILE=@T@/run/scanout-owner

function scanout_reset() {
  local out P=${DEVICE_SCANOUT_SHARPNESS}
  if [ -f "${SCANOUT_SCALED_FILE}" ]; then
    while read -r out; do
      swaymsg output "${out}" scale 1 >/dev/null 2>&1
    done < "${SCANOUT_SCALED_FILE}"
  fi
  if [ -d "${P}" ]; then
    echo Y  > "${P}/dpu_de" 2>/dev/null
    echo 12 > "${P}/dpu_de_sharpen1" 2>/dev/null
    echo 12 > "${P}/dpu_de_sharpen2" 2>/dev/null
    echo N  > "${P}/dpu_de_unscaled" 2>/dev/null
    echo N  > "${P}/dpu_nearest" 2>/dev/null
    echo N  > "${P}/dpu_de_dir" 2>/dev/null
    echo 32 > "${P}/dpu_de_dir_weight" 2>/dev/null
  fi
  rm -f "${SCANOUT_HINT_FILE}" "${SCANOUT_SCALED_FILE}" "${SCANOUT_OWNER_FILE}"
}

function scanout_reset_stale() {
  local pid
  read -r pid 2>/dev/null < "${SCANOUT_OWNER_FILE}" || return 0
  grep -qa runemu "/proc/${pid}/cmdline" 2>/dev/null || scanout_reset
}
ROCKNIX_MAIN
  sed -i "s#@T@#$T#g" "$T/rocknix/001-functions"
}
# A game that scaled the screen and was killed before its runemu.sh could put it back: owner $1 (a process id).
stale_scanout() {
  echo "$1" >"$T/run/scanout-owner"
  echo DSI-1 >"$T/run/scanout-scaled-outputs"
  echo "0.5 $1 DSI-1" >"$T/run/fullscreen-scale-hint"
  echo 30 >"$T/msm/dpu_de_sharpen1"
}
# Waits up to 5 s for shell condition $1 (the UI-start chores run in the background, after the first picture).
wait_for() {
  i=0
  while ! eval "$1" && [ $i -lt 50 ]; do sleep 0.1; i=$((i + 1)); done
  eval "$1"
}

# setsid, where there is one, so launch() can end the launcher's whole process group afterwards.
SETSID=; command -v setsid >/dev/null 2>&1 && SETSID=setsid
# Runs the launcher against $T; gives up after 60 s.
launch() {
  env PATH="$T/bin:$PATH" POCKETUI_ROOT="$ROOT" POCKETUI_LOG="$T/log" POCKETUI_BACKLIGHT="$T/backlight" \
    POCKETUI_DARK_FILE="$T/dark" POCKETUI_GAME_FILE="$T/game" POCKETUI_QUANTUM="$T/bin/quantum" \
    POCKETUI_RESTART_DELAY="${DELAY:-0}" POCKETUI_RESTARTS="${RESTARTS:-3}" POCKETUI_GAME_GRACE=5 \
    POCKETUI_AS_ESSWAY="${ESSWAY:-}" POCKETUI_SWAY_WAIT="${SWAY_WAIT:-30}" POCKETUI_START_ES="$T/bin/start_es.sh" POCKETUI_ES_ONLY_FILE="$T/es-only" \
    POCKETUI_FRAME_FILE="$T/frame" POCKETUI_UI_PID_FILE="$T/ui.pid" POCKETUI_FIRST_FRAME_SECS="${FIRST_FRAME:-30}" \
    POCKETUI_STABLE_SECS="${STABLE:-120}" POCKETUI_SAFE_RESTARTS="${SAFE_RESTARTS:-2}" POCKETUI_HELPER_SECS="${HELPER:-10}" \
    POCKETUI_ROCKNIX_PROFILE="$T/rocknix/profile" POCKETUI_ROCKNIX_FUNCTIONS="$T/rocknix/001-functions" \
    POCKETUI_ES_SETTINGS="$T/rocknix/es_settings" POCKETUI_SYSTEM_CFG="$T/system.cfg" POCKETUI_TZ_CACHE="$T/cache" \
    POCKETUI_LOCALE_DIR="$T/locale" POCKETUI_FAN_ROOT="$T/fansys" POCKETUI_FAN_NOTE="$T/fan.restore" \
    POCKETUI_FAN_BEAT="$T/fan.beat" POCKETUI_FAN_CHECK_SECS="${FANCHECK:-2}" POCKETUI_FAN_STALE_CHECKS=3 \
    $SETSID "$TEST_SH" "$LAUNCHER" &
  pid=$!
  i=0
  while kill -0 $pid 2>/dev/null && [ $i -lt 600 ]; do sleep 0.1; i=$((i + 1)); done
  if kill -0 $pid 2>/dev/null; then
    echo "     (launcher still running after 60 s, killed)"
    kill -9 $pid
  fi
  wait $pid 2>/dev/null
  # Did the logger outlive the launcher (essway mode: EmulationStation takes over the process)? Then end it.
  # The logger runs in the background: under load (a gate next to compiles) the launcher can exec EmulationStation
  # before the fake has written its pid. WAIT_LOGD=1 waits for the pid file, the fake's last step (up to 3 s).
  i=0
  while [ -n "${WAIT_LOGD:-}" ] && [ ! -s "$T/logd-pid" ] && [ $i -lt 30 ]; do sleep 0.1; i=$((i + 1)); done
  logd_left=0
  if [ -f "$T/logd-pid" ] && kill -0 "$(cat "$T/logd-pid")" 2>/dev/null; then
    logd_left=1
    kill "$(cat "$T/logd-pid")" 2>/dev/null
  fi
  # The launcher's process group (setsid): what it left running is ended by the next setup and at the end.
  [ -n "$SETSID" ] && GROUPS_LEFT="$GROUPS_LEFT $pid"
  return 0
}

# Ends what earlier launchers left behind (their background helpers; on the device systemd's cgroup ends them).
# Without this every gate left one or two sleeping launchers behind for good (2026-10-25: about 100, one of them
# holding the merge queue's lock). Not right after launch(): some checks read what the helpers write afterwards.
end_groups() {
  for g in $GROUPS_LEFT; do kill -TERM -"$g" 2>/dev/null; done
  GROUPS_LEFT=
}
GROUPS_LEFT=
# Removes the scenario's folder; a program that was just stopped may still write into it for a moment, which makes the
# first rm fail with "Directory not empty".
drop_scratch() {
  [ -n "${T:-}" ] || return 0
  rm -rf "$T" 2>/dev/null || { sleep 0.3; rm -rf "$T"; }
}
# The last scenario's folder goes too, also when the script is interrupted.
trap 'end_groups; drop_scratch' EXIT

runs() { cat "$T/runs" 2>/dev/null || echo 0; }
# The log line of run $1 (version and arguments).
run_line() { grep "^pocket-ui run $1 " "$T/calls"; }
# A real release layout: releases/<v>/pocket-ui for each of $@ (the fake), current -> the first, pocket-ui -> current/.
releases() {
  rm -f "$ROOT/pocket-ui"
  for v in "$@"; do
    mkdir -p "$ROOT/releases/$v"
    ln -s "$T/fake-pocket-ui" "$ROOT/releases/$v/pocket-ui"
  done
  ln -s "releases/$1" "$ROOT/current"
  ln -s current/pocket-ui "$ROOT/pocket-ui"
}
# EmulationStation took over the launcher's own process (exec), so systemd keeps it in the essway unit.
es_execd() { [ "$(cat "$T/es-pid" 2>/dev/null)" = "$pid" ]; }
no_essway_calls() { ! grep -q "systemctl .*essway" "$T/calls"; }
# Line number of the first call matching $1 (999 if none).
at() { grep -n -m1 -e "$1" "$T/calls" | cut -d: -f1 | grep . || echo 999; }

echo "# a crash restarts pocket-ui instead of going back to EmulationStation"
setup "139 134 77"
launch
check "three runs for two crashes" '[ "$(runs)" = 3 ]'
check "EmulationStation started once, after the last run" '[ "$(grep -c "systemctl start essway" "$T/calls")" = 1 ] && [ "$(at "start essway")" -gt "$(at "pocket-ui run 3")" ]'
check "each crash is logged" '[ "$(grep -c "crashed" "$T/log")" = 2 ]'
check "EmulationStation stopped first" '[ "$(at "systemctl stop essway")" -lt "$(at "pocket-ui run 1")" ]'
check "start_es.sh is not run directly" '! grep -q start_es.sh "$T/calls"'

echo "# a pocket-ui that keeps crashing: N restarts, then safe mode (its own restarts), then EmulationStation"
setup "139 139 139 139 139 139 139 139 139 139 139 139"
RESTARTS=3 SAFE_RESTARTS=2 launch
check "1 + 3 normal runs, then 1 + 2 in safe mode" '[ "$(runs)" = 7 ] && ! run_line 4 | grep -q -- --safe && run_line 5 | grep -q -- "--safe$" && run_line 7 | grep -q -- --safe'
check "safe mode is told why" 'grep -q "^POCKETUI_SAFE_REASON=pocket-ui stopped 4 times in a row (exit 139)" "$T/env.5"'
check "safe mode is logged" 'grep -q "starting it in safe mode" "$T/log"'
check "EmulationStation started, once, after the last safe run" '[ "$(grep -c "systemctl start essway" "$T/calls")" = 1 ] && [ "$(at "start essway")" -gt "$(at "pocket-ui run 7")" ]'
check "giving up is logged, and noted for the next start" 'grep -q "not even in safe mode" "$T/log" && grep -q "not even in safe mode" "$ROOT/es-fallback"'

echo "# restarts back off"
setup "139 139 139"
DELAY=1 RESTARTS=2 launch
check "first restart after 1 s" 'grep -q "in 1 s" "$T/log"'
check "second restart after 2 s" 'grep -q "in 2 s" "$T/log"'
check "3 normal runs, then safe mode" '[ "$(runs)" = 4 ] && run_line 4 | grep -q -- --safe'

echo "# clean-up after a crash during a frozen game in light sleep"
setup "139 77"
touch "$T/with-game"
echo 120 >"$T/dark"
launch
pg=$(cat "$T/game-pgid" 2>/dev/null || echo 0)
check "the frozen emulator was continued and asked to end" '[ -f "$T/emulator-term" ]'
check "runemu.sh was spared and ran its clean-up" 'grep -q "cleanup after exit 0" "$T/runemu-cleanup" 2>/dev/null'
check "nothing of the game is left" '! kill -0 -- -$pg 2>/dev/null'
check "the game file is gone" '[ ! -f "$T/game" ]'
check "the backlight is back at 120" '[ "$(cat "$T/backlight/brightness")" = 120 ]'
check "the saved brightness is gone" '[ ! -f "$T/dark" ]'
check "pocket-core --restore-system before the restart" '[ "$(at "pocket-core --restore-system")" -lt "$(at "pocket-ui run 2")" ]'
check "quantum before the restart" '[ "$(at "^quantum")" -lt "$(at "pocket-ui run 2")" ]'
check "clean-up again before EmulationStation" '[ "$(grep -c "pocket-core --restore-system" "$T/calls")" = 2 ]'

echo "# dev.env: KEY=VALUE lines only, nothing in it runs"
setup "77"
cat >"$ROOT/dev.env" <<EOF
# a comment
POCKETUI_TEST_A=plain
POCKETUI_TEST_B="two words"
POCKETUI_TEST_C='single \$quoted'
touch $T/pwned1
POCKETUI_TEST_D=\$(touch $T/pwned2)
POCKETUI_TEST_E=\`touch $T/pwned3\`
LD_PRELOAD=$T/evil.so
EOF
launch
check "plain value" 'grep -qx "POCKETUI_TEST_A=plain" "$T/env.1"'
check "double quotes stripped" 'grep -qx "POCKETUI_TEST_B=two words" "$T/env.1"'
check "single quotes stripped, kept literally" 'grep -qx "POCKETUI_TEST_C=single \$quoted" "$T/env.1"'
check "a command line does not run" '[ ! -e "$T/pwned1" ]'
check "\$(...) does not run" '[ ! -e "$T/pwned2" ] && grep -q "^POCKETUI_TEST_D=\$(touch" "$T/env.1"'
check "backquotes do not run" '[ ! -e "$T/pwned3" ]'
check "only pocket-ui variables are taken" '! grep -q LD_PRELOAD "$T/env.1"'

echo "# an update that crashes is rolled back"
setup "139 77"
rm "$ROOT/pocket-ui"
mkdir -p "$ROOT/releases/old" "$ROOT/releases/new"
ln -s "$T/fake-pocket-ui" "$ROOT/releases/old/pocket-ui"
ln -s "$T/fake-pocket-ui" "$ROOT/releases/new/pocket-ui"
ln -s current/pocket-ui "$ROOT/pocket-ui"
ln -s releases/new "$ROOT/current"
ln -s releases/old "$ROOT/previous"
echo 1.2.3 >"$ROOT/update-pending"
launch
check "back on the previous version" '[ "$(readlink "$ROOT/current")" = releases/old ]'
check "rolled-back names the version" '[ "$(cat "$ROOT/rolled-back")" = 1.2.3 ]'
check "update-pending is gone" '[ ! -f "$ROOT/update-pending" ]'
check "the previous version ran" '[ "$(runs)" = 2 ]'
check "a rollback is no crash restart" '! grep -q crashed "$T/log"'

echo "# exit 75 (update installed) starts pocket-ui again"
setup "75 77"
launch
check "two runs" '[ "$(runs)" = 2 ]'
check "not counted as a crash" '! grep -q crashed "$T/log"'

echo "# normal mode: a missing pocket-ui still goes through essway, the es-only mark changes nothing"
setup "77"
rm "$ROOT/pocket-ui"
touch "$T/es-only"
launch
check "essway stopped and started again" 'grep -q "systemctl stop essway" "$T/calls" && grep -q "systemctl start essway" "$T/calls"'
check "start_es.sh is not run directly" '! grep -q start_es.sh "$T/calls"'

echo "# essway mode: pocket-ui is essway, EmulationStation (asked for) follows in the same unit"
setup "139 77"
ESSWAY=1 launch
check "two runs" '[ "$(runs)" = 2 ]'
check "never systemctl stop/start essway" 'no_essway_calls'
check "start_es.sh once, after the last run" '[ "$(grep -c start_es.sh "$T/calls")" = 1 ] && [ "$(at start_es.sh)" -gt "$(at "pocket-ui run 2")" ]'
check "start_es.sh exec'd in the launcher's process" 'es_execd'
check "clean-up before the restart and before EmulationStation" '[ "$(grep -c "pocket-core --restore-system" "$T/calls")" = 2 ] && [ "$(at "pocket-core --restore-system")" -lt "$(at "pocket-ui run 2")" ]'

echo "# boot: sway answers only after a few tries, pocket-ui starts once it does"
setup "77"
cat >"$T/bin/swaymsg" <<EOF
#!$TEST_SH
echo "swaymsg \$*" >>"$T/calls"
if [ "\$*" = "-t get_version" ]; then
  n=\$((\$(cat "$T/sway-tries" 2>/dev/null || echo 0) + 1))
  echo \$n >"$T/sway-tries"
  [ \$n -gt 3 ]
fi
EOF
chmod +x "$T/bin/swaymsg"
ESSWAY=1 launch
check "waited for sway" '[ "$(cat "$T/sway-tries" 2>/dev/null)" = 4 ]'
check "pocket-ui started after sway answered" '[ "$(at "pocket-ui run 1")" -gt "$(grep -n -m4 "swaymsg -t get_version" "$T/calls" | tail -1 | cut -d: -f1)" ]'
check "one run, no crash counted" '[ "$(runs)" = 1 ] && ! grep -q crashed "$T/log"'

echo "# boot: sway never answers, pocket-ui is tried anyway after the wait"
setup "77"
cat >"$T/bin/swaymsg" <<EOF
#!$TEST_SH
echo "swaymsg \$*" >>"$T/calls"
[ "\$*" != "-t get_version" ]
EOF
chmod +x "$T/bin/swaymsg"
ESSWAY=1 SWAY_WAIT=1 launch
check "the wait is logged" 'grep -q "sway not ready" "$T/log"'
check "pocket-ui still tried" '[ "$(runs)" = 1 ]'

echo "# essway mode: after safe mode gave up too, start_es.sh"
setup "139 139 139 139 139 139 139 139 139"
ESSWAY=1 RESTARTS=3 SAFE_RESTARTS=1 launch
check "1 + 3 runs, then 1 + 1 in safe mode" '[ "$(runs)" = 6 ] && run_line 5 | grep -q -- --safe && run_line 6 | grep -q -- --safe'
check "giving up is logged" 'grep -q "not even in safe mode" "$T/log"'
check "never systemctl stop/start essway" 'no_essway_calls'
check "start_es.sh exec'd" 'es_execd'
check "the reason is kept for pocket-ui's next start" 'grep -q "not even in safe mode" "$ROOT/es-fallback"'

echo "# essway mode: exit 75 and crash restarts work as before"
setup "75 139 77"
ESSWAY=1 launch
check "three runs" '[ "$(runs)" = 3 ]'
check "only the crash is counted" '[ "$(grep -c crashed "$T/log")" = 1 ]'
check "start_es.sh after the last run" '[ "$(at start_es.sh)" -gt "$(at "pocket-ui run 3")" ] && es_execd'

echo "# essway mode: no pocket-ui to start, start_es.sh at once"
setup "77"
rm "$ROOT/pocket-ui"
ESSWAY=1 launch
check "missing: no run" '[ "$(runs)" = 0 ]'
check "missing: start_es.sh exec'd" 'es_execd'
check "missing: never systemctl stop/start essway" 'no_essway_calls'
check "missing: the reason is logged" 'grep -q "EmulationStation" "$T/log"'
check "missing: and noted for pocket-ui's next start" 'grep -q "missing" "$ROOT/es-fallback"'
setup "77"
rm "$ROOT/pocket-ui"
cp "$T/fake-pocket-ui" "$ROOT/pocket-ui"
chmod -x "$ROOT/pocket-ui"
ESSWAY=1 launch
check "not executable: no run, start_es.sh exec'd" '[ "$(runs)" = 0 ] && es_execd'
setup "77"
rm "$ROOT/pocket-ui"
ln -s current/pocket-ui "$ROOT/pocket-ui"
ESSWAY=1 launch
check "dangling symlink: no run, start_es.sh exec'd" '[ "$(runs)" = 0 ] && es_execd'
setup "77"
rm -rf "$ROOT"
ESSWAY=1 launch
check "no install directory: start_es.sh exec'd" 'es_execd && no_essway_calls'

echo "# essway mode: an update without a pocket-ui is rolled back, not skipped"
setup "77"
rm "$ROOT/pocket-ui"
mkdir -p "$ROOT/releases/old" "$ROOT/releases/new"
ln -s "$T/fake-pocket-ui" "$ROOT/releases/old/pocket-ui"
ln -s current/pocket-ui "$ROOT/pocket-ui"
ln -s releases/new "$ROOT/current"
ln -s releases/old "$ROOT/previous"
echo 1.2.4 >"$ROOT/update-pending"
ESSWAY=1 launch
check "back on the previous version" '[ "$(readlink "$ROOT/current")" = releases/old ]'
check "rolled-back names the version" '[ "$(cat "$ROOT/rolled-back")" = 1.2.4 ] && [ ! -f "$ROOT/update-pending" ]'
check "the previous version ran, then start_es.sh" '[ "$(runs)" = 1 ] && [ "$(at start_es.sh)" -gt "$(at "pocket-ui run 1")" ] && es_execd'

echo "# essway mode: the es-only mark (dev.sh stop) keeps EmulationStation"
setup "77"
touch "$T/es-only"
ESSWAY=1 launch
check "no run, start_es.sh exec'd" '[ "$(runs)" = 0 ] && es_execd'
check "the mark stays (until dev.sh deploy or a reboot)" '[ -f "$T/es-only" ]'
check "no note: EmulationStation was wanted" '[ ! -f "$ROOT/es-fallback" ]'

echo "# the logger (logs on the card): started once, before pocket-ui, under its own name"
setup "139 134 77"
cat >"$ROOT/dev.env" <<EOF
POCKETUI_TEST_DATA=/data/here
EOF
launch
check "the logger started, as pocket-logd, with --log-daemon" 'grep -q "^logd .*/pocket-logd --log-daemon$" "$T/calls"'
check "pocket-logd is a hard link to pocket-core (a symbolic one would still be found by pidof pocket-core)" '[ "$ROOT/pocket-logd" -ef "$ROOT/pocket-core" ] && [ ! -L "$ROOT/pocket-logd" ]'
check "started before the first pocket-ui run" '[ "$(at "^logd ")" -lt "$(at "pocket-ui run 1")" ]'
check "once, not once per crash restart" '[ "$(grep -c "^logd " "$T/calls")" = 1 ]'
check "it sees dev.env (POCKETUI_DATA and the like)" 'grep -qx "POCKETUI_TEST_DATA=/data/here" "$T/logd-env"'
check "asked to stop (SIGTERM) when the launcher ends, before EmulationStation starts again" '[ "$(at "^logd-term")" -lt "$(at "systemctl start essway")" ]'
check "it is gone when the launcher is" '[ "$logd_left" = 0 ]'

echo "# the logger: with the real layout (pocket-core -> current/pocket-core -> releases/<version>/) the link goes next to the file"
setup "77"
mkdir -p "$ROOT/releases/v1"
cp "$ROOT/pocket-core" "$ROOT/releases/v1/pocket-core"
rm "$ROOT/pocket-core" "$ROOT/pocket-ui"
ln -s "$T/fake-pocket-ui" "$ROOT/releases/v1/pocket-ui"
ln -s releases/v1 "$ROOT/current"
ln -s current/pocket-core "$ROOT/pocket-core"
ln -s current/pocket-ui "$ROOT/pocket-ui"
launch
check "pocket-logd is in the release folder, the same file as pocket-core" '[ "$ROOT/releases/v1/pocket-logd" -ef "$ROOT/releases/v1/pocket-core" ] && [ ! -e "$ROOT/pocket-logd" ]'
check "the logger started and pocket-ui ran" 'grep -q "^logd .*/releases/v1/pocket-logd --log-daemon$" "$T/calls" && [ "$(runs)" = 1 ]'
# A new pocket-core in the same folder (another file): the next start links it again.
rm "$ROOT/releases/v1/pocket-core"
cp "$ROOT/releases/v1/pocket-logd" "$ROOT/releases/v1/pocket-core"
: >"$T/calls"
echo 0 >"$T/runs"
launch
check "a replaced pocket-core gets its link again" '[ "$ROOT/releases/v1/pocket-logd" -ef "$ROOT/releases/v1/pocket-core" ] && grep -q "^logd " "$T/calls"'

echo "# the logger: essway mode lets it live on under EmulationStation, the unit ends it"
setup "77"
ESSWAY=1 WAIT_LOGD=1 launch
check "started" 'grep -q "^logd " "$T/calls"'
check "not stopped by the launcher: it is still running after the exec of EmulationStation" '[ "$logd_left" = 1 ] && ! grep -q "^logd-term" "$T/calls"'
check "EmulationStation still took over" 'es_execd'

echo "# the logger: a failing or missing logger changes nothing else"
setup "139 77"
touch "$T/logd-fails"
launch
check "pocket-ui ran and restarted as before" '[ "$(runs)" = 2 ]'
check "EmulationStation came back" 'grep -q "systemctl start essway" "$T/calls"'
setup "77"
rm "$ROOT/pocket-core"
launch
check "no pocket-core: no logger, pocket-ui runs" '[ "$(runs)" = 1 ] && ! grep -q "^logd " "$T/calls"'
setup "77"
rm "$ROOT/pocket-ui"
ESSWAY=1 launch
check "no pocket-ui to start: EmulationStation at once, no logger" 'es_execd && ! grep -q "^logd " "$T/calls"'

echo "# exit 0 (the rescue page's or the power menu's Restart pocket-ui) starts pocket-ui again; EmulationStation only when it is asked for (exit 77)"
setup "0 0 77"
launch
check "three runs, nothing counted as a crash" '[ "$(runs)" = 3 ] && ! grep -q crashed "$T/log"'
check "EmulationStation only after the run that asked for it" '[ "$(grep -c "systemctl start essway" "$T/calls")" = 1 ] && [ "$(at "start essway")" -gt "$(at "pocket-ui run 3")" ]'
check "clean-up before each restart and before EmulationStation" '[ "$(grep -c "pocket-core --restore-system" "$T/calls")" = 3 ]'
check "asking is logged, and leaves no note" 'grep -q "asked for EmulationStation" "$T/log" && [ ! -f "$ROOT/es-fallback" ]'
setup "0 77"
ESSWAY=1 launch
check "essway mode: exit 0 restarts, exit 77 execs start_es.sh" '[ "$(runs)" = 2 ] && es_execd && no_essway_calls'

echo "# exit 0 before the first picture is a failed start, not a restart"
setup "0:noframe 0:noframe 77"
RESTARTS=3 launch
check "both counted as crashes" '[ "$(grep -c "crashed (exit 0, no picture)" "$T/log")" = 2 ]'

echo "# the black screen guard: no picture after FIRST_FRAME seconds ends pocket-ui, a failed start"
setup "hang 77"
FIRST_FRAME=2 launch
check "the hung run was ended" '! kill -0 "$(cat "$T/hung-pid")" 2>/dev/null'
check "logged as a black screen" 'grep -q "no picture 2 s after the start (black screen)" "$T/log"'
check "counted as a crash, then the next run" 'grep -q "crashed (exit 143, no picture)" "$T/log" && [ "$(runs)" = 2 ]'
setup "0:4 77"
FIRST_FRAME=2 launch
check "a run with its picture is left alone after FIRST_FRAME" '[ "$(runs)" = 2 ] && ! grep -q "black screen" "$T/log" && ! grep -q crashed "$T/log"'

echo "# a version that ran STABLE seconds with a picture is the known good one"
setup "0:3 77"
releases v1
STABLE=2 launch
check "known-good -> releases/v1" '[ "$(readlink "$ROOT/known-good")" = releases/v1 ]'
check "logged" 'grep -q "v1 ran 2 s: it is the known good version now" "$T/log"'
setup "0:noframe 77"
releases v1
STABLE=1 launch
check "a short run is not" '[ ! -e "$ROOT/known-good" ]'
setup "139 139 139 139 0:3 77"
releases v1
STABLE=2 RESTARTS=3 launch
check "a long run in safe mode is not either" '[ ! -e "$ROOT/known-good" ] && run_line 5 | grep -q -- --safe'

echo "# a crash loop goes back to the last known good version, then safe mode"
setup "139 139 139 139 139 139 139 139 77"
releases new old
ln -s releases/old "$ROOT/known-good"
RESTARTS=3 launch
check "four runs of the new version, then the known good one" 'run_line 4 | grep -q "releases/new $" && run_line 5 | grep -q "releases/old $"'
check "current -> releases/old, rolled-back names the new one" '[ "$(readlink "$ROOT/current")" = releases/old ] && [ "$(cat "$ROOT/rolled-back")" = new ]'
check "logged" 'grep -q "back to the known good version old" "$T/log"'
check "then safe mode on the known good version" 'run_line 9 | grep -q "releases/old --safe$" && ! run_line 8 | grep -q -- --safe'
check "the known good version stays on disk" '[ -x "$ROOT/releases/old/pocket-ui" ] && [ -x "$ROOT/releases/new/pocket-ui" ]'

echo "# safe mode: Restart normally (exit 0) goes back to a normal start"
setup "139 139 139 139 0 77"
RESTARTS=3 launch
check "run 5 in safe mode, run 6 normal again" 'run_line 5 | grep -q -- --safe && ! run_line 6 | grep -q -- --safe'
check "the reason is not passed on to the normal run" '! grep -q POCKETUI_SAFE_REASON "$T/env.6"'

echo "# exit 78 (the rescue page's Use previous version)"
setup "78 77"
releases new old good
ln -sfn releases/old "$ROOT/previous"
ln -s releases/good "$ROOT/known-good"
launch
check "the known good version first" '[ "$(readlink "$ROOT/current")" = releases/good ] && run_line 2 | grep -q "releases/good $"'
check "no rolled-back note: it was chosen" '[ ! -f "$ROOT/rolled-back" ]'
setup "78 77"
releases new old
ln -sfn releases/old "$ROOT/previous"
launch
check "else the previous one" '[ "$(readlink "$ROOT/current")" = releases/old ] && run_line 2 | grep -q "releases/old $"'
setup "78 77"
releases only
launch
check "with neither, the same version again" '[ "$(readlink "$ROOT/current")" = releases/only ] && [ "$(runs)" = 2 ] && grep -q "there is none" "$T/log"'

echo "# a program that can't be run (exit 127): no restarts, safe mode, then EmulationStation"
setup "127:noframe 127:noframe 77"
ESSWAY=1 RESTARTS=3 launch
check "one normal run, one in safe mode" '[ "$(runs)" = 2 ] && run_line 2 | grep -q -- --safe'
check "EmulationStation, with a note" 'es_execd && grep -q "not even in safe mode (exit 127, no picture)" "$ROOT/es-fallback"'

echo "# sway gone after a failed start: EmulationStation at once"
setup "1:noframe 77"
cat >"$T/bin/swaymsg" <<EOS
#!$TEST_SH
echo "swaymsg \$*" >>"$T/calls"
[ "\$*" != "-t get_version" ] || [ ! -f "$T/runs" ]
EOS
chmod +x "$T/bin/swaymsg"
ESSWAY=1 launch
check "one run, then start_es.sh" '[ "$(runs)" = 1 ] && es_execd'
check "the reason is noted" 'grep -q "sway does not answer" "$ROOT/es-fallback"'

echo "# every helper has a time limit: a hung pocket-core --restore-system, swaymsg and systemctl"
setup "139 77"
cat >"$T/bin/hang" <<EOS
#!$TEST_SH
echo "\$(basename "\$0") \$*" >>"$T/calls"
sleep 300
EOS
chmod +x "$T/bin/hang"
printf 'echo "pocket-core $*" >>"%s"\nsleep 300\n' "$T/calls" >>"$ROOT/pocket-core"
cp "$T/bin/hang" "$T/bin/systemctl"
cp "$T/bin/hang" "$T/bin/swaymsg"
echo 120 >"$T/dark"
t0=$(date +%s)
ESSWAY=1 HELPER=1 SWAY_WAIT=2 launch
check "both runs happened and the launcher ended by itself, in time" '[ "$(runs)" = 2 ] && es_execd && [ $(($(date +%s) - t0)) -lt 40 ]'
check "the hung helpers were called" 'grep -q "pocket-core --restore-system" "$T/calls" && grep -q "^systemctl " "$T/calls" && grep -q "^swaymsg output" "$T/calls"'

echo "# ROCKNIX's UI start on 20261001: es_settings' steps, nothing of ROCKNIX's profile before the first picture"
setup "77"
cat >"$T/system.cfg" <<EOF
system.hostname=ROCKNIX
system.timezone=Europe/Vienna
EOF
printf 'TIMEZONE=UTC' >"$T/cache/timezone"
ESSWAY=1 launch
check "SDL_AUDIODRIVER from es_settings, for pocket-ui and its games" 'grep -qx "SDL_AUDIODRIVER=pulseaudio" "$T/env.1"'
check "no language set: no LANG from the launcher" '! grep -q "^LANG=" "$T/env.1"'
check "no pre-rotation on 20261001" '! grep -q prerotate "$T/env.1"'
check "the profile is loaded only after the first run started" 'wait_for "grep -q ^profile \"\$T/calls\"" && [ "$(at "^profile")" -gt "$(at "pocket-ui run 1")" ]'
check "set_kill set emulationstation, as es_settings leaves it" 'wait_for "grep -qx \"set_kill set emulationstation\" \"\$T/calls\""'
check "the time zone files follow system.timezone, tz-data restarted" 'wait_for "grep -qx \"systemctl restart tz-data.service\" \"\$T/calls\"" && [ "$(cat "$T/cache/timezone")" = TIMEZONE=Europe/Vienna ] && [ "$(cat "$T/cache/system_timezone")" = Europe/Vienna ]'
check "no scanout reset without scanout_reset_stale" '! grep -q "scale 1" "$T/calls" && ! grep -q "sharpness put back" "$T/log"'
check "EmulationStation still exec'd" 'es_execd'
setup "77"
echo "system.timezone=Europe/Vienna" >"$T/system.cfg"
printf 'TIMEZONE=Europe/Vienna' >"$T/cache/timezone"
launch
sleep 1
check "the same zone: tz-data is not restarted" 'wait_for "grep -q set_kill \"\$T/calls\"" && ! grep -q "tz-data" "$T/calls"'

echo "# ROCKNIX's UI start: the language EmulationStation was set to"
setup "77"
echo "system.language=de_DE" >"$T/system.cfg"
launch
check "LANG, LANGUAGE and LOCPATH for pocket-ui" 'grep -qx "LANG=de_DE.UTF-8" "$T/env.1" && grep -qx "LANGUAGE=de_DE.UTF-8" "$T/env.1" && grep -qx "LOCPATH=$T/locale" "$T/env.1"'
check "the locale is compiled once, where LOCPATH points" 'wait_for "grep -q ^localedef \"\$T/calls\"" && grep -q "^localedef -i de_DE -c -v -f UTF-8 $T/locale/de_DE.UTF-8 I18NPATH=/usr/share/i18n" "$T/calls"'
check "and passed on to systemd" 'wait_for "grep -qx \"systemctl import-environment LANG LOCPATH LANGUAGE\" \"\$T/calls\""'
setup "77"
echo "system.language=de_DE" >"$T/system.cfg"
mkdir -p "$T/locale/de_DE.UTF-8" && : >"$T/locale/de_DE.UTF-8/LC_NAME"
launch
check "a compiled locale is not compiled again" 'wait_for "grep -q \"import-environment LANG\" \"\$T/calls\"" && ! grep -q ^localedef "$T/calls"'
setup "77"
echo 'system.language=de_DE;touch /tmp/x' >"$T/system.cfg"
launch
check "a language that is not a locale name is ignored" '! grep -q "^LANG=" "$T/env.1" && grep -q "is not a locale name" "$T/log"'

echo "# ROCKNIX's UI start: the quirk variables of 999-export go to systemd, the set ones only"
setup "77"
printf 'export  OS_VERSION \\\n\tDEVICE_PRE_ROTATION \\\n\tDEVICE_NOT_SET_HERE \\\n\tWLR_CON\n' >"$T/rocknix/999-export"
rocknix_main true
launch
check "one import-environment with the variables the profile set" 'wait_for "grep -q \"import-environment.*DEVICE_PRE_ROTATION\" \"\$T/calls\"" && grep -qx "systemctl import-environment DEVICE_PRE_ROTATION WLR_CON" "$T/calls"'

echo "# ROCKNIX main: pre-rotation as start_es.sh works it out"
setup "77"
rocknix_main false
ESSWAY=1 launch
check "plane can't rotate: pocket-ui gets the frontend's pre-rotation (270 -> 90)" 'grep -qx "vk_wsi_wayland_prerotate=90" "$T/env.1" && grep -qx "egl_wayland_prerotate=90" "$T/env.1"'
check "worked out before the first run, and logged" '[ "$(at "^profile")" -lt "$(at "pocket-ui run 1")" ] && grep -q "pre-rotation: 90" "$T/log"'
setup "77"
rocknix_main true
launch
check "plane rotates (the Mini V2's default): no pre-rotation" '! grep -q prerotate "$T/env.1" && grep -q "pre-rotation: none" "$T/log"'

echo "# ROCKNIX main: a game killed before it could put the screen's scaling back"
setup "77"
rocknix_main true
stale_scanout 999999
ESSWAY=1 launch
check "scale 1 again, before the first run" 'grep -qx "swaymsg output DSI-1 scale 1" "$T/calls" && [ "$(at "output DSI-1 scale 1")" -lt "$(at "pocket-ui run 1")" ]'
check "sharpening back to the driver's default, run files gone" '[ "$(cat "$T/msm/dpu_de_sharpen1")" = 12 ] && [ ! -e "$T/run/scanout-owner" ] && [ ! -e "$T/run/fullscreen-scale-hint" ]'
check "logged" 'grep -q "sharpness put back (left by process 999999)" "$T/log"'
setup "77"
rocknix_main true
printf 'sleep 30\n' >"$T/runemu-busy.sh"
sh "$T/runemu-busy.sh" &
busy=$!
stale_scanout $busy
launch
kill $busy 2>/dev/null
check "a runemu.sh that still runs keeps its scaling" '[ -f "$T/run/scanout-owner" ] && ! grep -q "scale 1" "$T/calls" && [ "$(cat "$T/msm/dpu_de_sharpen1")" = 30 ]'
setup "77"
stale_scanout 999999
launch
check "the file alone (no scanout_reset_stale in 001-functions) changes nothing" '[ -f "$T/run/scanout-owner" ] && ! grep -q "scale 1" "$T/calls"'

echo "# ROCKNIX main: clean_up resets what a crashed pocket-ui's game left, after the game has ended"
setup "139 77"
rocknix_main true
touch "$T/with-game"
# This game's runemu.sh scales the screen and dies without putting it back (as a SIGKILL would leave it).
cat >"$T/runemu.sh" <<EOF
#!$TEST_SH
echo \$\$ >"$T/run/scanout-owner"
echo DSI-1 >"$T/run/scanout-scaled-outputs"
"$T/emulator"
echo "cleanup after exit \$?" >"$T/runemu-cleanup"
EOF
launch
check "reset in clean_up, after the game, before the restart" 'grep -qx "swaymsg output DSI-1 scale 1" "$T/calls" && [ "$(at "output DSI-1 scale 1")" -lt "$(at "pocket-ui run 2")" ] && [ ! -e "$T/run/scanout-owner" ]'

echo "# ROCKNIX's UI start without ROCKNIX's files (another system, a test machine): nothing breaks"
setup "0 77"
rm -f "$T/rocknix/001-functions" "$T/rocknix/es_settings" "$T/system.cfg"
launch
check "two runs, EmulationStation after" '[ "$(runs)" = 2 ] && grep -q "systemctl start essway" "$T/calls"'
check "no SDL_AUDIODRIVER invented" '! grep -q SDL_AUDIODRIVER "$T/env.1"'

echo "# the drop-in deploy/essway-pocket-ui.conf"
CONF=$HERE/essway-pocket-ui.conf
line_of() { grep -n -m1 -e "$1" "$CONF" 2>/dev/null | cut -d: -f1 | grep . || echo 999; }
unit_section() { sed -n '/^\[Unit\]/,/^\[Service\]/p' "$CONF"; }
check "it exists" '[ -f "$CONF" ]'
check "ExecStart= is cleared before it is set" '[ "$(line_of "^ExecStart=$")" -lt "$(line_of "^ExecStart=.")" ] && [ "$(line_of "^ExecStart=.")" != 999 ]'
check "exactly one new ExecStart" '[ "$(grep -c "^ExecStart=." "$CONF" 2>/dev/null)" = 1 ]'
check "Environment=POCKETUI_AS_ESSWAY=1" 'grep -qx "Environment=POCKETUI_AS_ESSWAY=1" "$CONF"'
service_section() { sed -n '/^\[Service\]/,$p' "$CONF"; }
check "no start limit in [Unit]: a failed essway is a black screen" 'unit_section | grep -qx "StartLimitIntervalSec=0" && ! grep -q "^StartLimitBurst" "$CONF"'
check "instead a growing pause before each restart, up to a minute" 'service_section | grep -q "^RestartSteps=[1-9]" && service_section | grep -qx "RestartMaxDelaySec=60"'
# Its ExecStart command, with the device paths pointed at $T.
exec_start() {
  cmd=$(sed -n "s/^ExecStart=\/bin\/sh -c '\(.*\)'\$/\1/p" "$CONF" 2>/dev/null | sed "s#/storage/pocket-ui/#$ROOT/#g; s#/usr/bin/start_es.sh#$T/bin/start_es.sh#g")
  [ -n "$cmd" ] && "$TEST_SH" -c "$cmd"
}
setup "77"
printf '#!/bin/sh\necho launcher >>"%s"\n' "$T/calls" >"$ROOT/run-pocket-ui.sh"
chmod +x "$ROOT/run-pocket-ui.sh"
exec_start
check "ExecStart runs the launcher" 'grep -qx launcher "$T/calls" && ! grep -q start_es.sh "$T/calls"'
setup "77"
exec_start
check "ExecStart without a launcher runs start_es.sh" 'grep -qx start_es.sh "$T/calls"'

# --- Smooth fan (src/fan_curve.rs): the launcher gives the fan back to the kernel --------------------------------
# A fake fan below $T/fansys as pocket-ui leaves it while it drives the fan: the six active trips at the backstop
# (83500), the PWM at 120, the zone at $1 millidegrees; and the note in fan_curve::Note's format.
fan_driven() {
  Z=$T/fansys/sys/class/thermal/thermal_zone6
  H=$T/fansys/sys/class/hwmon/hwmon3
  mkdir -p "$Z" "$H"
  echo 120 >"$H/pwm1"
  echo "$1" >"$Z/temp"
  {
    echo "# pocket-ui smooth fan: what to put back (src/fan_curve.rs, deploy/run-pocket-ui.sh)"
    echo "park $H/pwm1 51"
    i=2
    for t in 52000 62000 69000 75000 80000 85000; do
      echo 83500 >"$Z/trip_point_${i}_temp"
      echo "trip $Z/trip_point_${i}_temp $t"
      i=$((i + 1))
    done
    echo "off-below $Z/temp 52000 $H/pwm1"
  } >"$T/fan.restore"
  echo 90000 >"$Z/trip_point_0_temp"
  echo 110000 >"$Z/trip_point_1_temp"
}
trips() { for i in 2 3 4 5 6 7; do cat "$Z/trip_point_${i}_temp"; done | tr '\n' ' '; }
SYSTEM_TRIPS="52000 62000 69000 75000 80000 85000 "

echo "# Smooth fan: pocket-ui crashed while driving the fan, clean_up gives it back before the restart"
setup "139 77"
fan_driven 58000
cat >"$T/hook" <<EOF
if [ \$n = 2 ]; then
  if [ -f "$T/fan.restore" ]; then echo "run 2: note still there" >>"$T/calls"; else echo "run 2: fan handed back" >>"$T/calls"; fi
fi
EOF
launch
check "handed back before the restart" 'grep -q "run 2: fan handed back" "$T/calls"'
check "the six trips are ROCKNIX's again" '[ "$(trips)" = "$SYSTEM_TRIPS" ]'
check "hot and critical untouched" '[ "$(cat "$Z/trip_point_0_temp") $(cat "$Z/trip_point_1_temp")" = "90000 110000" ]'
check "58 °C is above the lowest trip: the PWM parked at 51 for the kernel, not stopped" '[ "$(cat "$H/pwm1")" = 51 ]'
check "the note is gone, and it is logged" '[ ! -f "$T/fan.restore" ] && grep -q "fan: back to ROCKNIX.s control (pocket-ui ended)" "$T/log"'

echo "# Smooth fan: handed back below the lowest trip, the fan is off as the kernel would have it"
setup "77"
fan_driven 47000
launch
check "PWM 0 at 47 °C (lowest trip 52)" '[ "$(cat "$H/pwm1")" = 0 ] && [ "$(trips)" = "$SYSTEM_TRIPS" ]'

echo "# Smooth fan: a note that names other files is not followed"
setup "77"
fan_driven 58000
echo "secret" >"$T/victim"
echo "trip $T/victim 1" >>"$T/fan.restore"
echo "trip $T/fansys/sys/../../victim 1" >>"$T/fan.restore"
launch
check "nothing outside the sysfs root written" '[ "$(cat "$T/victim")" = secret ]'
check "the rest was still handed back" '[ "$(trips)" = "$SYSTEM_TRIPS" ]'

echo "# Smooth fan: the watchdog gives the fan back when the curve's heartbeat stops, while pocket-ui runs"
setup "77:3 77"
fan_driven 58000
cat >"$T/hook" <<EOF
if [ \$n = 1 ]; then
  echo "7 1700000000 curve quiet 551 46" >"$T/fan.beat"
  ( sleep 2.5; if [ -f "$T/fan.restore" ]; then echo "during run 1: note still there" >>"$T/calls"; else echo "during run 1: fan handed back" >>"$T/calls"; fi ) &
fi
EOF
FANCHECK=0.2 launch
check "handed back during the run" 'grep -q "during run 1: fan handed back" "$T/calls"'
check "logged as the watchdog's" 'grep -q "the fan curve stopped answering" "$T/log" && [ "$(trips)" = "$SYSTEM_TRIPS" ]'

echo "# Smooth fan: a curve whose heartbeat moves is left alone"
setup "77:3 77"
fan_driven 58000
cat >"$T/hook" <<EOF
if [ \$n = 1 ]; then
  ( k=0; while [ \$k -lt 40 ]; do k=\$((k + 1)); echo "\$k 1700000000 curve quiet 551 46" >"$T/fan.beat"; sleep 0.1; done ) &
  ( sleep 2.5; [ -f "$T/fan.restore" ] && echo "during run 1: note still there" >>"$T/calls" ) &
fi
EOF
FANCHECK=0.2 launch
check "the note stayed while the beat moved" 'grep -q "during run 1: note still there" "$T/calls" && ! grep -q "stopped answering" "$T/log"'
check "and clean_up gave it back at the end" '[ ! -f "$T/fan.restore" ] && [ "$(trips)" = "$SYSTEM_TRIPS" ]'

echo "# Smooth fan: TERM to the launcher (shutdown, essway stopped) gives the fan back before it ends"
setup "0 77"
fan_driven 58000
cat >"$T/hook" <<EOF
[ \$n = 1 ] && kill -TERM \$PPID
EOF
launch
check "one run only: the launcher ended" '[ "$(runs)" = 1 ]'
check "the fan handed back on the way out" '[ ! -f "$T/fan.restore" ] && [ "$(trips)" = "$SYSTEM_TRIPS" ] && grep -q "the launcher was stopped" "$T/log"'

# SM8550 owns cooling even if a stale SM8250 note was copied onto its card.
echo "# SM8550: leave fan files and recovery notes alone; preserve Zink"
setup "77"
fan_driven 58000
echo 'POCKETUI_DEVICE=odin2mini' >"$ROOT/dev.env"
cat >"$T/hook" <<EOF
echo "mesa \$MESA_LOADER_DRIVER_OVERRIDE" >>"$T/calls"
EOF
export MESA_LOADER_DRIVER_OVERRIDE=zink
launch
unset MESA_LOADER_DRIVER_OVERRIDE
check "SM8550 PWM and thermal trips untouched" '[ "$(cat "$H/pwm1")" = 120 ] && [ "$(trips)" = "83500 83500 83500 83500 83500 83500 " ]'
check "SM8550 recovery note was not applied" '[ -f "$T/fan.restore" ]'
check "Zink environment reached pocket-ui" 'grep -q "mesa zink" "$T/calls"'

end_groups
echo
if [ $fails -eq 0 ]; then
  echo "all launcher tests passed"
else
  echo "$fails launcher test(s) failed"
  exit 1
fi
