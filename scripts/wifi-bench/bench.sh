#!/bin/sh
# The Wi-Fi bench on the Docker host (docs/WIFI-BENCH.md). Normally started by run.sh, which stages the tree here.
#
#   bench.sh SCENARIO...|all      each scenario on a fresh pair of containers; results/<stamp>/<scenario>/
#   bench.sh up [ONBOARDING]      the bench by hand: containers, radios, services, pocket-ui (stays up)
#   bench.sh down                 remove both containers (logs into results/manual)
#   bench.sh ap ARGS...           ap.sh in the access point container (start/stop/portal/dhcp/status)
#   bench.sh ui KEY...            pocket-ui's FIFO; `bench.sh frame` its visible text; `bench.sh sh CMD` in the device
#   bench.sh type TEXT            TEXT on the on-screen keyboard (0-9 a-z - _ . @)
set -eu
WB_ROOT=$(cd "$(dirname "$0")/../.." && pwd)
export WB_ROOT
SCENARIOS="mesh hotspot portal password password-mesh forget rfkill rfkill-on"

case ${1:-} in
  up | down | ap | ui | type | frame | sh) WB_OUT=$WB_ROOT/results/manual ;;
  *) WB_OUT=${WB_OUT:-$WB_ROOT/results/$(date +%Y%m%d-%H%M%S)} ;;
esac
RUN_DIR=$WB_OUT
. "$WB_ROOT/scripts/wifi-bench/lib.sh"
. "$WB_ROOT/scripts/wifi-bench/scenarios.sh"

case ${1:-} in
  up) bench_up "${2:-0}"; services_up; deploy_ui; say "bench up: $STA + $AP (bench.sh down to remove)" ;;
  down) bench_down ;;
  ap) shift; ax sh /bench/scripts/wifi-bench/ap.sh "$@" ;;
  ui) shift; ui "$@" ;;
  type) type_text "$2" ;;
  frame) frame ;;
  sh) shift; sx sh -c "$*" ;;
  "" | -h | --help) sed -n '2,10p' "$0"; exit 2 ;;
  *)
    [ "$1" = all ] && set -- $SCENARIOS
    mkdir -p "$RUN_DIR"
    for s in "$@"; do
      case " $SCENARIOS " in *" $s "*) ;; *) die "unknown scenario $s ($SCENARIOS)" ;; esac
      OUT=$RUN_DIR/$s; STEP=0; mkdir -p "$OUT"; : >"$OUT/result.txt"
      say "=== $s"
      started=$(date +%s)
      ( "scenario_$(echo "$s" | tr - _)" ) >"$OUT/run.log" 2>&1 || echo "FAIL  the scenario script stopped early (run.log)" >>"$OUT/result.txt"
      bench_down
      verdict=$(grep -q '^FAIL' "$OUT/result.txt" && echo FAIL || echo PASS)
      echo "$verdict $s ($(( $(date +%s) - started )) s)" | tee -a "$RUN_DIR/summary.txt"
      sed 's/^/    /' "$OUT/result.txt"
    done
    echo "results: $RUN_DIR"
    ;;
esac
