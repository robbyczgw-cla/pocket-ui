#!/bin/sh
# Tests of the speed guard (scripts/latency.sh, docs/LATENCY.md).
#   scripts/test-latency.sh            the statistics, the comparison and the trace parser of latency.py (no device)
#   scripts/test-latency.sh --vdev     and a short run in the virtual device (VDEV_NAME as for vdev.sh; needs
#                                      `vdev.sh deploy` first): the harness still finds every number and compares
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
python3 "$ROOT/scripts/latency.py" selftest
[ "${1:-}" = --vdev ] || exit 0

tmp=$(mktemp -d "${TMPDIR:-/tmp}/latency-test.XXXXXX")
trap 'rm -rf "$tmp"' EXIT
"$ROOT/scripts/latency.sh" --vdev --quick --out "$tmp/a.json" >"$tmp/out.txt" 2>&1 || { cat "$tmp/out.txt"; exit 1; }
for m in start.cold nav.p50 nav.p95 open.settings open.switcher open.pause_menu open.pause_menu_again open.switcher_game launch.gbc quit.to_shelf switch.game cpu.shelf_idle cpu.behind_game; do
  grep -q "^$m " "$tmp/out.txt" || { cat "$tmp/out.txt"; echo "test-latency: $m missing" >&2; exit 1; }
done
# Against itself: ok. Against a baseline twice as fast: slower, exit 1.
python3 "$ROOT/scripts/latency.py" compare "$tmp/a.json" "$tmp/a.json" >/dev/null
python3 - "$tmp/a.json" "$tmp/fast.json" <<'EOF'
import json, sys
d = json.load(open(sys.argv[1]))
for m in d["metrics"].values():
    m["value"] = m["value"] / 2
json.dump(d, open(sys.argv[2], "w"))
EOF
if python3 "$ROOT/scripts/latency.py" compare "$tmp/a.json" "$tmp/fast.json" --min-ms 0 --min-cpu 0 >/dev/null; then
  echo "test-latency: a regression was not reported" >&2
  exit 1
fi
echo "test-latency: vdev run ok"
