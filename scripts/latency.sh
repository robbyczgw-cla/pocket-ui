#!/bin/sh
# Speed guard: how quickly pocket-ui starts, moves, opens its menus and starts, quits and switches games, and how
# much CPU it uses when it should be idle. What each number means and the baselines: docs/LATENCY.md.
#
#   scripts/latency.sh                          measure in the virtual device (VDEV_NAME as for vdev.sh, default dev),
#                                               print a table, write latency.json
#   POCKETUI_HOST=root@<ip> scripts/latency.sh  the same on a Mini V2 over SSH (stops pocket-ui and starts it again, then
#                                               scripts/verify-device.sh --no-local: exit 3 when the device is not in its
#                                               normal state, e.g. a pocket-ui of the run left behind)
#   scripts/latency.sh --compare                and compare with the baseline of that target (docs/latency/<target>.json);
#                                               exit 1 when a number is more than 15 % slower
#
# Options:
#   --out FILE            where the JSON goes (default: latency.json)
#   --compare             compare with docs/latency/vdev.json or docs/latency/device.json
#   --baseline FILE       compare with FILE
#   --threshold PCT       slower than this fails (default 15); --min-ms MS (3) and --min-cpu POINTS (0.3) are the
#                         smallest differences that count, so 4 -> 5 ms is not a regression
#   --no-confirm          fail at once; by default a failed comparison measures again and judges the pooled runs
#   --save-baseline       write the result as the baseline of the target (commit it with the change that caused it)
#   --quick               one short run (a smoke test of the harness, too noisy to compare)
#   --cold                only starts, each with an empty page cache (drop_caches, as at power-on): start.nocache and
#                         each step of pocket-ui's start (step.library, step.event_loop, ...); docs/LATENCY.md "Cold start"
#   --wake                only deep and light sleep and the wake from them (docs/LATENCY.md "Waking"): on a device it
#                         suspends for real, woken by the RTC alarm, run as the unit pocket-latency so a dropped SSH
#                         connection doesn't end it; in the virtual device a suspend command that blocks stands in
#   --deploy              virtual device: build and deploy the working tree first (vdev.sh deploy)
#   --vdev                the virtual device even when POCKETUI_HOST is set
#   --phases              also fetch the trace (latency-trace.log next to --out) and print where the time of each
#                         Quit, pause menu and switcher went (`latency.py phases`, docs/LATENCY.md "Phases")
#   -- ARGS               passed to `latency.py measure` (--runs, --moves, --systems gbc,gba:megaman, ... see its --help)
#
# Exit codes: 0 ok, 1 slower than the baseline, 2 usage or setup, 3 the measurement itself failed.
# CI: `scripts/vdev.sh deploy && scripts/latency.sh --compare` (docs/LATENCY.md).
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
PY=$ROOT/scripts/latency.py
OUT=latency.json
BASELINE=
COMPARE=0
THRESHOLD=15
MIN_MS=3
MIN_CPU=0.3
CONFIRM=1
SAVE=0
DEPLOY=0
FORCE_VDEV=0
QUICK=0
COLD=0
PHASES=0
WAKE=0

die() { echo "latency: $*" >&2; exit 2; }
say() { echo "latency: $*" >&2; }

while [ $# -gt 0 ]; do
  case "$1" in
    --out) OUT=$2; shift 2 ;;
    --compare) COMPARE=1; shift ;;
    --baseline) BASELINE=$2; COMPARE=1; shift 2 ;;
    --threshold) THRESHOLD=$2; shift 2 ;;
    --min-ms) MIN_MS=$2; shift 2 ;;
    --min-cpu) MIN_CPU=$2; shift 2 ;;
    --no-confirm) CONFIRM=0; shift ;;
    --save-baseline) SAVE=1; shift ;;
    --quick) QUICK=1; shift ;;
    --cold) COLD=1; shift ;;
    --deploy) DEPLOY=1; shift ;;
    --vdev) FORCE_VDEV=1; shift ;;
    --phases) PHASES=1; shift ;;
    --wake) WAKE=1; shift ;;
    --) shift; break ;;
    -h | --help) sed -n '2,/^set -eu/p' "$0" | sed '$d; s/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown option $1 (see --help)" ;;
  esac
done
EXTRA="$*"
[ "$QUICK" = 1 ] && EXTRA="--runs 1 --starts 2 --moves 30 --opens 2 --cycles 1 --cpu-secs 4 --no-warmup $EXTRA"
[ "$COLD" = 1 ] && EXTRA="--cold $EXTRA"

if [ -n "${POCKETUI_HOST:-}" ] && [ "$FORCE_VDEV" = 0 ]; then TARGET=device; else TARGET=vdev; fi
if [ "$WAKE" = 1 ]; then
  if [ $TARGET = vdev ]; then EXTRA="--wake --mock-suspend 4 $EXTRA"; else EXTRA="--wake $EXTRA"; fi
fi
[ -n "$BASELINE" ] || BASELINE=$ROOT/docs/latency/$TARGET.json
COMMIT=$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo unknown)
(cd "$ROOT" && git diff --quiet HEAD -- src 2>/dev/null) || COMMIT="$COMMIT+dirty"
META="--meta target=$TARGET --meta commit=$COMMIT"

# --- the virtual device -----------------------------------------------------------------------------------
NAME=${VDEV_NAME:-dev}
C=vdev-$NAME
VDEV=$ROOT/scripts/vdev.sh

vdev_prepare() {
  if [ "$DEPLOY" = 1 ]; then
    "$VDEV" deploy >/dev/null || die "vdev.sh deploy failed"
  fi
  [ "$(docker inspect -f '{{.State.Running}}' "$C" 2>/dev/null)" = true ] || die "$C is not running: scripts/vdev.sh deploy (or --deploy)"
  docker exec "$C" sh -c "test -x /storage/pocket-ui/pocket-ui" || die "pocket-ui is not deployed in $C: scripts/vdev.sh deploy (or --deploy)"
  docker cp "$PY" "$C:/tmp/latency.py"
}

vdev_measure() { # $1: local JSON file
  "$VDEV" stop
  rc=0
  # shellcheck disable=SC2086
  docker exec "$C" python3 /tmp/latency.py measure --out /tmp/latency.json $META $EXTRA || rc=$?
  [ $rc = 0 ] && docker cp "$C:/tmp/latency.json" "$1"
  [ "$PHASES" = 0 ] || docker cp "$C:/tmp/pocket-trace.log" "$TRACE" >/dev/null || say "no trace to fetch from $C"
  # pocket-ui the normal way again (the launcher, without the trace)
  "$VDEV" deploy --no-build >/dev/null || say "could not start pocket-ui again in $C"
  return $rc
}

# --- a Mini V2 ------------------------------------------------------------------------------------------
device_prepare() {
  . "$ROOT/scripts/host.sh"
  need_host
  scp -q "$PY" "$HOST:/tmp/latency.py"
}

# What the device side does after the measurement, whatever its outcome: nothing the harness started may stay behind.
# essway (or the pocketui unit) is stopped for the whole run, so every pocket-ui and pocket-core still alive now is the
# harness's (latency.py kills its own in Ui.stop, this is the net under it for a crash or a dropped connection). Then
# the normal pocket-ui comes back the way dev.sh deploy starts it.
DEVICE_RESTORE='for p in $(pidof pocket-ui) $(pidof pocket-core); do kill $p 2>/dev/null; done
i=0; while [ -n "$(pidof pocket-ui)$(pidof pocket-core)" ] && [ $i -lt 30 ]; do sleep 0.1; i=$((i+1)); done
for p in $(pidof pocket-ui) $(pidof pocket-core); do kill -9 $p 2>/dev/null; done
if [ -f $DROPIN ]; then rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway;
else systemctl reset-failed pocketui 2>/dev/null; systemd-run --unit=pocketui /storage/pocket-ui/run-pocket-ui.sh >/dev/null; fi'

# After a run on a device: is it back in its normal state (one pocket-ui in the unit, no trace variables, ...)?
# Not compared with a local build: this measures what is installed. Exit 3 when not.
device_verify() {
  "$ROOT/scripts/verify-device.sh" --no-local "$HOST" >&2 || { say "the device is not in its normal state after the run (VERIFY above): fix that before the next one"; exit 3; }
}

device_measure() { # $1: local JSON file
  rc=0
  # Stop pocket-ui in whichever unit it runs (like dev.sh deploy), and EmulationStation, which would hold the screen.
  # Afterwards pocket-ui comes back the way dev.sh deploy starts it.
  # shellcheck disable=SC2029
  ssh "$HOST" "DROPIN=/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
    systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null; systemctl stop essway
    i=0; while pidof pocket-ui >/dev/null && [ \$i -lt 50 ]; do sleep 0.1; i=\$((i+1)); done
    python3 /tmp/latency.py measure --out /tmp/latency.json $META $EXTRA; rc=\$?
    $DEVICE_RESTORE
    exit \$rc" || rc=$?
  [ $rc = 0 ] && scp -q "$HOST:/tmp/latency.json" "$1"
  [ "$PHASES" = 0 ] || scp -q "$HOST:/tmp/pocket-trace.log" "$TRACE" || say "no trace to fetch from $HOST"
  device_verify
  return $rc
}

# --wake on a device: the measurement suspends it, and the SSH connection may not survive that (Wi-Fi goes down and
# comes back). So it runs as a unit of its own on the device, and this side asks every few seconds whether it is done.
device_measure_detached() { # $1: local JSON file
  # The script the unit runs: what device_measure does, with the exit code left in a file.
  # shellcheck disable=SC2087
  ssh "$HOST" "cat > /tmp/latency-run.sh" <<RUN
DROPIN=/storage/.config/system.d/essway.service.d/10-pocket-ui.conf
systemctl stop pocketui 2>/dev/null; systemctl reset-failed pocketui 2>/dev/null; systemctl stop essway
i=0; while pidof pocket-ui >/dev/null && [ \$i -lt 50 ]; do sleep 0.1; i=\$((i+1)); done
python3 /tmp/latency.py measure --out /tmp/latency.json $META $EXTRA >/tmp/latency-run.log 2>&1; rc=\$?
$DEVICE_RESTORE
echo \$rc > /tmp/latency-run.rc
RUN
  ssh "$HOST" 'rm -f /tmp/latency-run.rc; systemctl reset-failed pocket-latency 2>/dev/null; systemd-run --unit=pocket-latency sh /tmp/latency-run.sh >/dev/null' || return 3
  say "running on the device as the unit pocket-latency; it suspends the device (SSH may drop meanwhile)"
  st=running
  tries=0
  while [ "$st" = running ] && [ $tries -lt 720 ]; do
    sleep 5
    tries=$((tries + 1))
    st=$(ssh -o ConnectTimeout=5 "$HOST" 'cat /tmp/latency-run.rc 2>/dev/null || echo running' 2>/dev/null) || st=running
  done
  ssh "$HOST" cat /tmp/latency-run.log >&2 || true
  [ "$st" = 0 ] || return 3
  scp -q "$HOST:/tmp/latency.json" "$1"
  [ "$PHASES" = 0 ] || scp -q "$HOST:/tmp/pocket-trace.log" "$TRACE" || say "no trace to fetch from $HOST"
  device_verify
}

measure() { # $1: local JSON file
  rc=0
  if [ $TARGET = vdev ]; then vdev_measure "$1" || rc=$?
  elif [ "$WAKE" = 1 ]; then device_measure_detached "$1" || rc=$?
  else device_measure "$1" || rc=$?; fi
  [ $rc = 0 ] || { say "the measurement failed (exit $rc); pocket-ui's output: /tmp/latency-pocket-ui.log on the target"; exit 3; }
}

if [ $TARGET = vdev ]; then vdev_prepare; else device_prepare; fi
say "measuring on the $TARGET ($COMMIT)"
TRACE=$(dirname "$OUT")/latency-trace.log
measure "$OUT"
python3 "$PY" table "$OUT"
if [ "$PHASES" = 1 ] && [ -f "$TRACE" ]; then
  echo
  python3 "$PY" phases "$TRACE" || true
fi

if [ "$SAVE" = 1 ]; then
  mkdir -p "$(dirname "$BASELINE")"
  cp "$OUT" "$BASELINE"
  say "saved as the $TARGET baseline: $BASELINE"
fi

[ "$COMPARE" = 1 ] || exit 0
[ -f "$BASELINE" ] || die "no baseline $BASELINE (record one with --save-baseline)"
echo
rc=0
python3 "$PY" compare "$OUT" "$BASELINE" --threshold "$THRESHOLD" --min-ms "$MIN_MS" --min-cpu "$MIN_CPU" || rc=$?
if [ $rc = 1 ] && [ "$CONFIRM" = 1 ]; then
  # Noise or a real regression: measure once more and judge the runs of both together (medians).
  say "slower than the baseline: measuring again to rule out noise"
  second=$(mktemp "${TMPDIR:-/tmp}/latency.XXXXXX")
  measure "$second"
  python3 "$PY" merge "$OUT" "$OUT" "$second"
  rm -f "$second"
  echo
  rc=0
  python3 "$PY" compare "$OUT" "$BASELINE" --threshold "$THRESHOLD" --min-ms "$MIN_MS" --min-cpu "$MIN_CPU" || rc=$?
fi
exit $rc
