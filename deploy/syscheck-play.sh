#!/bin/sh
# Runs pocket-core on one game for a while, takes screenshots and sends test input (device side of
# scripts/syscheck.sh; pocket-core's own FIFO commands, so the shelf behind it never sees a button).
#   syscheck-play.sh SYSTEM CORE ROM STEP...
# Steps: "wait N" seconds, "press BTN [MS]", "axis lx|ly|rx|ry V", "shot NAME" (/tmp/pcs_NAME.png),
#        "fifo TEXT" (a command line for pocket-core: save, load, disc next, ...), "quit" (clean exit, auto-save)
# Environment: BIN (pocket-core path), TRACE=1 (core environment trace), BENCH=N (benchmark seconds, no steps),
#              EXTRA="--rotate 1" (more pocket-core arguments).
SYS=$1; CORE=$2; ROM=$3; shift 3
BIN=${BIN:-/storage/pcsys/pocket-core}
export XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1 SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock
export POCKET_CORE_TEST=1
[ -n "$TRACE" ] && export POCKET_CORE_TRACE=1
LOG=/tmp/pcs_play.log
FIFO=/tmp/pocket-core.cmd

send() { timeout 2 sh -c "echo \"$1\" > $FIFO"; }

if [ -n "$BENCH" ]; then
  "$BIN" "/usr/lib/libretro/${CORE}_libretro.so" "$ROM" --system "$SYS" $EXTRA --bench "$BENCH" >"$LOG" 2>&1 &
else
  "$BIN" "/usr/lib/libretro/${CORE}_libretro.so" "$ROM" --system "$SYS" $EXTRA >"$LOG" 2>&1 &
fi
PID=$!
sleep 2
for step in "$@"; do
  set -- $step
  case "$1" in
    wait) sleep "$2" ;;
    press|axis) send "$step" ;;
    fifo) shift; send "$*" ;;
    shot) grim "/tmp/pcs_$2.png" ;;
    quit) send quit ;;
  esac
done
# Wait for the process to end on its own (bench, quit); otherwise stop it.
n=0
while kill -0 $PID 2>/dev/null && [ $n -lt ${WAITEND:-$((${BENCH:-0} + 4))} ]; do sleep 1; n=$((n + 1)); done
if kill -0 $PID 2>/dev/null; then send quit; sleep 2; kill $PID 2>/dev/null; echo "(had to stop it)"; fi
wait $PID
echo "exit $?"
if [ -n "$TRACE" ]; then cat "$LOG"; else grep -v '^\[opt\]' "$LOG"; fi
