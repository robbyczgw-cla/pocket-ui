#!/bin/sh
# How much CPU pocket-ui uses while a game runs behind it and its menu is closed, in the virtual device.
# It should sleep: it wakes for input (HOME), the game's end and timers measured in seconds, nothing else.
#   scripts/idle-cpu.sh [SECONDS [MAX_PERCENT]]     VDEV_NAME as for vdev.sh (default dev); needs `vdev.sh deploy` first
#   STORM=sticks|buttons|none scripts/idle-cpu.sh   what the player does meanwhile (default sticks)
# Starts the first game of the first system on the shelf, waits for it to run, then reads every thread's
# /proc/<pid>/task/<tid>/schedstat (nanoseconds on a CPU) and voluntary context switches (wake-ups) over SECONDS (30).
# STORM=sticks wiggles the virtual pad's left stick about 100 times a second (a real stick in use), buttons presses A and B
# ten times a second. Prints the threads that did anything and the total; exits 1 when the total is above MAX_PERCENT (1.0).
#
# What this found (virtual device, a GBA game on pocket-core, 30 s):
#   before: no input 3.8 % (the UI sound mixer dropped its audio sink without closing it: PipeWire's data-loop thread
#           was woken 160 times a second for good), left stick moving 11.8 % (every pad event redrew the shelf behind the game)
#   after:  no input 0.06 %, left stick moving 0.4 %
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
SECS=${1:-30}
MAX=${2:-1.0}
STORM=${STORM:-sticks}
NAME=${VDEV_NAME:-dev}
C=vdev-$NAME
VDEV="$ROOT/scripts/vdev.sh"

dx() { docker exec "$C" "$@"; }
die() { echo "idle-cpu: $*" >&2; exit 2; }

docker inspect -f '{{.State.Running}}' "$C" 2>/dev/null | grep -q true || die "$C is not running: scripts/vdev.sh up, then deploy"
dx pidof pocket-ui >/dev/null || die "pocket-ui is not running in $C: scripts/vdev.sh deploy"

# A game: A on the system, A on the game. (The shelf starts on the first system.)
if ! dx pidof pocket-core >/dev/null 2>&1; then
  "$VDEV" send a
  sleep 1.5
  "$VDEV" send a
  i=0
  until dx pidof pocket-core >/dev/null 2>&1; do
    i=$((i + 1))
    [ $i -gt 60 ] && die "no game started (is the shelf on a system with a game?)"
    sleep 0.5
  done
fi
# The launch fade, the first frames and the first sounds are over after this.
sleep 8

# The player: through the virtual pad's command pipe, for the length of the measurement and a little more.
docker exec -i "$C" sh -c 'cat >/tmp/idle-storm.sh' <<'EOF'
S=$1
KIND=$2
end=$(( $(date +%s) + S ))
(
  case $KIND in
    sticks)
      while [ "$(date +%s)" -lt "$end" ]; do
        echo "stick 0.6 0.2"
        sleep 0.01
        echo "stick -0.6 -0.2"
        sleep 0.01
      done
      echo "stick 0 0"
      ;;
    buttons)
      while [ "$(date +%s)" -lt "$end" ]; do
        echo "press A 30"
        sleep 0.05
        echo "press B 30"
        sleep 0.05
      done
      ;;
    *) sleep "$S" ;;
  esac
) >/tmp/vpad.cmd
EOF
[ "$STORM" = none ] || docker exec -d "$C" sh /tmp/idle-storm.sh $((SECS + 6)) "$STORM"
[ "$STORM" = none ] || sleep 3

status=0
docker exec -i "$C" sh -s -- "$SECS" "$MAX" "$STORM" <<'EOF' || status=$?
SECS=$1
MAX=$2
STORM=$3
pid=$(pidof pocket-ui | cut -d' ' -f1)
snap() {
  for t in /proc/$pid/task/*; do
    tid=${t##*/}
    set -- $(cat $t/schedstat 2>/dev/null) || continue
    ns=$1
    echo "$tid|$(cat $t/comm 2>/dev/null)|$ns|$(sed -n 's/^voluntary_ctxt_switches:[[:space:]]*//p' $t/status 2>/dev/null)"
  done
}
snap >/tmp/idle-cpu.a
# A kernel without CONFIG_SCHEDSTATS (the Mini V2's) has no schedstat files: n/a and a failure, never 0 % (a pass).
if [ ! -s /tmp/idle-cpu.a ]; then
  echo "pocket-ui pid $pid: CPU n/a, no /proc/$pid/task/*/schedstat (kernel without CONFIG_SCHEDSTATS)"
  exit 2
fi
a=$(cut -d' ' -f1 /proc/uptime)
sleep "$SECS"
snap >/tmp/idle-cpu.b
b=$(cut -d' ' -f1 /proc/uptime)
span=$(echo "$b $a" | awk '{printf "%.2f", $1-$2}')
echo "pocket-ui pid $pid with a game running (player: $STORM), $span s, $(ls /proc/$pid/task | wc -l) threads"
awk -F'|' -v up="$span" -v max="$MAX" '
  NR==FNR { cpu[$1]=$3; sw[$1]=$4; next }
  { d=$3-cpu[$1]; w=$4-sw[$1]; tot+=d; totw+=w
    if (d>0 || w>0) printf "  tid %-6s %-16s %6.3f %%  %5.1f wake-ups/s\n", $1, $2, d/1e9*100/up, w/up }
  END { pct=tot/1e9*100/up; printf "  TOTAL %.3f %%  %.1f wake-ups/s (limit %s %%)\n", pct, totw/up, max; exit (pct > max) }
' /tmp/idle-cpu.a /tmp/idle-cpu.b
EOF
# Leave the container as it was: the game ends (the command pipe blocks without a reader).
dx sh -c 'timeout 3 sh -c "echo quit >/tmp/pocket-core.cmd"' 2>/dev/null || true
exit $status
