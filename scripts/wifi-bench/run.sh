#!/bin/sh
# The Wi-Fi bench's entry point (docs/WIFI-BENCH.md): real NetworkManager + iwd of ROCKNIX 20261001 and the real
# pocket-ui against simulated radios and access points (mac80211_hwsim + hostapd), in the colima VM of a Mac.
#
#   scripts/wifi-bench/run.sh [--no-build] [--bin DIR] [--label NAME] [SCENARIO...|all]
#
# Scenarios: mesh hotspot portal password password-mesh forget rfkill rfkill-on (default: all). Builds pocket-ui for
# aarch64 here (zigbuild), stages the few files the bench needs on the Docker host (WIFI_BENCH_HOST: the SSH host of a
# Mac with colima, or "local" where Docker runs as root on Linux here; from the environment or scripts/local.env),
# runs bench.sh there and fetches the results to
# $WIFI_BENCH_RESULTS/<stamp>-<label>/ (default: wifi-bench-results/ next to the checkout). Exit 0 when every scenario passed.
# --bin DIR: pocket-ui + pocket-core from DIR instead (another branch's build).
set -eu
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
# WIFI_BENCH_HOST from the environment, else from scripts/local.env (untracked, like POCKETUI_HOST: scripts/host.sh).
[ -n "${WIFI_BENCH_HOST:-}" ] || WIFI_BENCH_HOST=$(sed -n 's/^WIFI_BENCH_HOST=//p' "$ROOT/scripts/local.env" 2>/dev/null | tail -1)
[ -n "$WIFI_BENCH_HOST" ] || { echo "run.sh: set WIFI_BENCH_HOST (an SSH host with colima, or local), e.g. in scripts/local.env" >&2; exit 2; }
HOST=$WIFI_BENCH_HOST
RESULTS=${WIFI_BENCH_RESULTS:-$(cd "$ROOT/.." && pwd)/wifi-bench-results}
TARGET=aarch64-unknown-linux-gnu
build=1 bin=$ROOT/target/$TARGET/release label=$(git -C "$ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null | tr / -)
while [ $# -gt 0 ]; do
  case $1 in
    --no-build) build=0; shift ;;
    --bin) bin=$2; build=0; shift 2 ;;
    --label) label=$2; shift 2 ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) break ;;
  esac
done
[ $# -gt 0 ] || set -- all
stamp=$(date +%Y%m%d-%H%M%S)-$label

if [ $build = 1 ]; then
  export PATH=$HOME/.cargo/bin:$HOME/opt/zig:$PATH
  (cd "$ROOT" && . scripts/release-rustflags.sh && nice -n 10 cargo zigbuild --release --target $TARGET.2.38 --bin pocket-ui --bin pocket-core)
fi
[ -x "$bin/pocket-ui" ] && [ -x "$bin/pocket-core" ] || { echo "run.sh: no pocket-ui/pocket-core in $bin" >&2; exit 1; }

# The staged tree: what vdev.sh, the device image and the bench read, in the repo's layout.
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
(cd "$ROOT" && tar cf - --exclude=scripts/wifi-bench/results scripts/vdev.sh scripts/wifi-bench scripts/portal-standin.py testdata/portal docker deploy/run-pocket-ui.sh deploy/portmaster-setup.sh) | tar xf - -C "$stage"
mkdir -p "$stage/target/$TARGET/release"
cp "$bin/pocket-ui" "$bin/pocket-core" "$stage/target/$TARGET/release/"
echo "pocket-ui $(git -C "$bin" rev-parse --short HEAD 2>/dev/null) ($(git -C "$bin" status --porcelain --untracked-files=no 2>/dev/null | wc -l) changed files) from $bin, md5 $(md5sum "$bin/pocket-ui" | cut -c1-8)" >"$stage/scripts/wifi-bench/BUILD"

mkdir -p "$RESULTS/$stamp"
if [ "$HOST" = local ]; then
  rm -rf "$HOME/wifi-bench-run" && cp -a "$stage" "$HOME/wifi-bench-run"
  set +e; WB_OUT="$RESULTS/$stamp" sh "$HOME/wifi-bench-run/scripts/wifi-bench/bench.sh" "$@"; rc=$?; set -e
else
  rsync -a --delete "$stage/" "$HOST:wifi-bench-run/"
  # The Mac's PATH for colima/docker (Homebrew), the bench writes into wifi-bench-run/results/<stamp>.
  set +e
  ssh "$HOST" "export PATH=/opt/homebrew/bin:/usr/local/bin:\$PATH; cd wifi-bench-run && WB_OUT=\$HOME/wifi-bench-run/results/$stamp sh scripts/wifi-bench/bench.sh $*"
  rc=$?
  set -e
  rsync -a "$HOST:wifi-bench-run/results/$stamp/" "$RESULTS/$stamp/"
fi
cp "$stage/scripts/wifi-bench/BUILD" "$RESULTS/$stamp/BUILD"
echo "run.sh: results in $RESULTS/$stamp"
[ -f "$RESULTS/$stamp/summary.txt" ] && cat "$RESULTS/$stamp/summary.txt"
! grep -q '^FAIL' "$RESULTS/$stamp/summary.txt" 2>/dev/null && [ "$rc" = 0 ]
