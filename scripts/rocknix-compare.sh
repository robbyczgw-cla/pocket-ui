#!/bin/sh
# pocket-ui against stock ROCKNIX (EmulationStation + RetroArch) on a Mini V2, measured from outside the same way for
# both: a press written into the Pocket's pad, the screen recorded from the compositor. docs/COMPARE-ROCKNIX.md.
#
#   rocknix-compare.sh run [--ui es,pocket-ui] [--plan FILE] [--out DIR] [-- DRIVE ARGS]
#       copies the tool over, runs it on the device as the unit pocket-compare (both UIs one after the other; the
#       device script puts everything back, whatever happens), fetches the results, analyses them here, prints the
#       table (DIR/table.md, DIR/<ui>.json) and runs scripts/verify-device.sh --no-local (exit 3 if not VERIFY: ok).
#       DRIVE ARGS go to `rocknix-compare.py drive`: --scenarios start,nav,launch,switch,wake,cpu,play (default all),
#       --systems gbc,gba,snes, --reps 3, --moves 30, --play-secs 300, ... (rocknix-compare.py drive --help)
#   rocknix-compare.sh mode es|pocket-ui      the UI on the screen now (for calibrating the plan); `restore` after
#   rocknix-compare.sh try "RIGHT RIGHT A"   presses, then a screenshot (screenshots/compare-try.jpg)
#                                             (COMPARE_INJECT=target if `probe` found that InputPlumber grabs its source)
#   rocknix-compare.sh shot [NAME]            a screenshot only
#   rocknix-compare.sh probe                  which source key gives which button (stops essway meanwhile)
#   rocknix-compare.sh restore                pocket-ui as the UI again (mark removed, essway restarted), then verify
#   rocknix-compare.sh analyze DIR            analyse fetched results again (DIR/out/<ui>), print the table
#   rocknix-compare.sh video SHOWCASE_DIR MEASURE_DIR [ARGS]
#                                             the side-by-side video from a showcase run (`run -- --scenarios showcase`)
#                                             with the numbers of a measurement run: scripts/rocknix-compare-video.py
#   rocknix-compare.sh selftest               rocknix-compare.py selftest (no device)
#
# The device: POCKETUI_HOST or scripts/local.env (scripts/host.sh). Wrap device commands in scripts/devlock.sh.
# Exit codes: 0 ok, 2 usage or setup, 3 the run or the device check failed.
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
. "$ROOT/scripts/host.sh"
PY=$ROOT/scripts/rocknix-compare.py
W=/tmp/rocknix-compare
WAYLAND="XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1"
die() { echo "compare: $*" >&2; exit 2; }
say() { echo "compare: $*" >&2; }

verify() {
  "$ROOT/scripts/verify-device.sh" --no-local "$HOST" >&2 || { say "the device is not in its normal state (VERIFY above)"; exit 3; }
}

upload() { # the tool as a tar stream, unpacked into $W by the SSH session it is piped into
  tar -C "$ROOT" -cf - scripts/rocknix-compare.py scripts/rocknix-compare-device.sh scripts/rocknix-compare-plan.json
}

analyze_dir() { # $1: local folder with out/<ui>
  d=$1
  files=""
  for ui in es pocket-ui; do
    [ -f "$d/out/$ui/events.jsonl" ] || continue
    python3 -I "$PY" analyze "$d/out/$ui" --out "$d/$ui.json" || say "$ui: analysis failed"
    [ -f "$d/$ui.json" ] && files="$files $d/$ui.json"
  done
  [ -n "$files" ] || die "nothing to analyse in $d/out"
  # shellcheck disable=SC2086
  python3 -I "$PY" table $files | tee "$d/table.md"
}

cmd=${1:-}
[ $# -gt 0 ] && shift
case "$cmd" in
  run)
    UIS="es pocket-ui"
    PLAN=$ROOT/scripts/rocknix-compare-plan.json
    OUT=$ROOT/compare-results/$(date +%Y%m%d-%H%M%S)
    while [ $# -gt 0 ]; do
      case "$1" in
        --ui) UIS=$(echo "$2" | tr ',' ' '); shift 2 ;;
        --plan) PLAN=$2; shift 2 ;;
        --out) OUT=$2; shift 2 ;;
        --) shift; break ;;
        *) die "unknown option $1" ;;
      esac
    done
    python3 -I -c "import json,sys; p=json.load(open(sys.argv[1])); sys.exit(0 if p.get('calibrated') else 1)" "$PLAN" ||
      die "$PLAN is not calibrated yet: check every sequence with 'mode' and 'try' first, then set \"calibrated\": true (docs/COMPARE-ROCKNIX.md)"
    mkdir -p "$OUT"
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/compare.XXXXXX")
    trap 'rm -rf "$tmp"' EXIT
    mkdir -p "$tmp/scripts"
    cp "$PY" "$ROOT/scripts/rocknix-compare-device.sh" "$tmp/scripts/"
    cp "$PLAN" "$tmp/scripts/rocknix-compare-plan.json"
    printf 'UIS="%s"\nDRIVE_ARGS="%s"\nPLAN=scripts/rocknix-compare-plan.json\n' "$UIS" "$*" >"$tmp/run.env"
    say "starting on $HOST: $UIS; $*"
    # One SSH session: the files over, the unit started, its log followed until it is done. A deep sleep may drop
    # the connection: then this asks again every few seconds.
    # shellcheck disable=SC2016
    tar -C "$tmp" -cf - scripts run.env |
      ssh "$HOST" 'set -e; W='"$W"'
        if systemctl is-active -q pocket-compare 2>/dev/null; then echo "compare: a run is going on already" >&2; exit 2; fi
        rm -rf $W; mkdir -p $W; cd $W; tar -xf -
        systemctl reset-failed pocket-compare 2>/dev/null || true
        since=@$(date +%s)
        systemd-run --unit=pocket-compare --setenv=COMPARE_WORK=$W sh $W/scripts/rocknix-compare-device.sh >/dev/null
        log() { journalctl -u pocket-compare --since "$since" -o cat --no-pager 2>/dev/null; }
        n=0; while [ ! -f $W/rc ]; do sleep 2; log | tail -n +$((n+1)); n=$(log | wc -l); done
        exit 0' || say "the connection went away; waiting for the run to end"
    tries=0
    while :; do
      st=$(ssh -o ConnectTimeout=5 "$HOST" "cat $W/rc 2>/dev/null || echo running" 2>/dev/null) || st=running
      [ "$st" != running ] && break
      tries=$((tries + 1))
      [ $tries -gt 1200 ] && die "no end of the run after an hour of asking; look at journalctl -u pocket-compare on the device"
      sleep 5
    done
    ssh "$HOST" "cat $W/out.tgz" >"$tmp/out.tgz"
    tar -xzf "$tmp/out.tgz" -C "$OUT"
    say "run ended with $st; results in $OUT"
    analyze_dir "$OUT" || true
    verify
    [ "$st" = 0 ] || exit 3
    ;;
  mode)
    case "${1:-}" in
      es) ssh "$HOST" 'touch /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway' ;;
      pocket-ui) ssh "$HOST" 'rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway' ;;
      *) die "mode es|pocket-ui" ;;
    esac
    say "the screen shows $1 now; 'rocknix-compare.sh restore' when done"
    ;;
  try | shot)
    mkdir -p "$ROOT/screenshots"
    name=compare-${2:-${cmd}}
    [ "$cmd" = shot ] && name=compare-${1:-shot}
    seq=""
    [ "$cmd" = try ] && seq=${1:?try needs a sequence, e.g. \"RIGHT A\"}
    # shellcheck disable=SC2029
    upload | ssh "$HOST" "mkdir -p $W && cd $W && tar -xf - &&
      { [ -z '$seq' ] || python3 $W/scripts/rocknix-compare.py press '$seq' --inject ${COMPARE_INJECT:-source} \$( [ -f $W/out/codes.json ] && echo --codes $W/out/codes.json ); } &&
      sleep 0.8 && $WAYLAND grim -t jpeg -q 75 -s 0.5 /tmp/compare-shot.jpg"
    scp -q "$HOST:/tmp/compare-shot.jpg" "$ROOT/screenshots/$name.jpg"
    echo "$ROOT/screenshots/$name.jpg"
    ;;
  probe)
    upload | ssh "$HOST" "mkdir -p $W/out && cd $W && tar -xf - && systemctl stop essway; sleep 1;
      python3 $W/scripts/rocknix-compare.py probe --out $W/out/codes.json; rc=\$?;
      rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway; exit \$rc" || say "probe: some buttons not found"
    verify
    ;;
  restore)
    ssh "$HOST" 'systemctl stop pocket-compare 2>/dev/null; rm -f /tmp/pocket-ui.es-only; systemctl reset-failed essway 2>/dev/null; systemctl restart essway'
    verify
    ;;
  analyze)
    analyze_dir "${1:?analyze DIR}"
    ;;
  video)
    sdir=${1:?video SHOWCASE_DIR MEASURE_DIR}; mdir=${2:?video SHOWCASE_DIR MEASURE_DIR}; shift 2
    python3 -I "$ROOT/scripts/rocknix-compare-video.py" "$sdir" --numbers "$mdir" "$@"
    ;;
  selftest)
    python3 -I "$PY" selftest
    ;;
  *)
    sed -n '2,/^set -eu/p' "$0" | sed '$d; s/^# \{0,1\}//'
    [ -n "$cmd" ] && exit 2
    ;;
esac
