#!/bin/sh
# Checks systems with pocket-core without touching the shelf: runs a game with test input, screenshots,
# saves/loads, and (device only) measures CPU against RetroArch. Two targets:
#   the Mini V2 over SSH (wrap calls in scripts/devlock.sh), or the virtual device (docs/VIRTUAL-DEVICE.md)
#   when VDEV_NAME is set; there, push means "vdev.sh deploy" and bench/ra are not available.
#
#   syscheck.sh push                          build for the device, copy pocket-core + scripts to /storage/pcsys
#   syscheck.sh rom SYSTEM FILE...            copy ROM files to /storage/roms/SYSTEM
#   syscheck.sh play SYSTEM CORE ROM STEP...  run, follow the steps (see deploy/syscheck-play.sh), print the log,
#                                             fetch the screenshots to screenshots/
#   syscheck.sh bench SYSTEM CORE ROM [SECS]  pocket-core CPU over SECS (default 30), measured from 12 s in
#   syscheck.sh ra SYSTEM CORE ROM [SECS]     the same with RetroArch (through ROCKNIX's runemu.sh)
# Example: scripts/devlock.sh me scripts/syscheck.sh play megadrive genesis_plus_gx /storage/roms/megadrive/Game.bin "wait 5" "shot a" quit
set -eu
. "$(dirname "$0")/host.sh"
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DIR=/storage/pcsys
CMD=${1:-}
shift || true

if [ -n "${VDEV_NAME:-}" ]; then
  C=vdev-$VDEV_NAME
  BIN=${BIN:-/storage/pocket-ui/pocket-core}
  rsh() { docker exec "$C" sh -c "$1"; }
  fetch() { docker cp "$C:$1" "$2" >/dev/null; }
  put() { docker cp "$1" "$C:$2" >/dev/null; }
else
  rsh() { ssh "$HOST" "$1"; }
  fetch() { scp -q "$HOST:$1" "$2"; }
  put() { scp -q "$1" "$HOST:$2"; }
fi

case "$CMD" in
  push)
    if [ -n "${VDEV_NAME:-}" ]; then
      "$ROOT/scripts/vdev.sh" deploy >/dev/null
      rsh "mkdir -p $DIR"
      put "$ROOT/deploy/syscheck-play.sh" "$DIR/syscheck-play.sh"
      rsh "chmod +x $DIR/*"
    else
      (cd "$ROOT" && . "$ROOT/scripts/release-rustflags.sh" && cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-core)
      rsh "mkdir -p $DIR"
      scp -q "$ROOT/target/aarch64-unknown-linux-gnu/release/pocket-core" "$ROOT/deploy/syscheck-play.sh" "$ROOT/deploy/bench.sh" "$HOST:$DIR/"
      rsh "chmod +x $DIR/*"
    fi
    ;;
  rom)
    system=$1; shift
    rsh "mkdir -p /storage/roms/$system"
    for f in "$@"; do put "$f" "/storage/roms/$system/$(basename "$f")"; done
    ;;
  play)
    system=$1; core=$2; rom=$3; shift 3
    rsh "rm -f /tmp/pcs_*.png; EXTRA='${EXTRA:-}' TRACE='${TRACE:-}' BIN='${BIN:-}' WAITEND='${WAITEND:-}' $DIR/syscheck-play.sh '$system' '$core' '$rom' $(for s in "$@"; do printf "'%s' " "$s"; done)"
    mkdir -p "$ROOT/screenshots"
    for f in $(rsh 'ls /tmp/pcs_*.png 2>/dev/null' || true); do fetch "$f" "$ROOT/screenshots/$(basename "$f")"; echo "$ROOT/screenshots/$(basename "$f")"; done
    ;;
  bench)
    system=$1; core=$2; rom=$3; secs=${4:-30}
    rsh "EXTRA='${EXTRA:-}' BENCH=$((secs + 16)) $DIR/syscheck-play.sh '$system' '$core' '$rom' >/tmp/pcs_bench.log 2>&1 & sleep 10; $DIR/bench.sh $secs pocket-core 'pocket-core $core'; wait; tail -5 /tmp/pcs_bench.log"
    ;;
  ra)
    system=$1; core=$2; rom=$3; secs=${4:-30}
    # RetroArch resumes from its auto state: remove it so every run starts from the beginning.
    stem=$(basename "$rom"); stem=${stem%.*}
    rsh "rm -f '/storage/roms/savestates/$system/$stem.state.auto' '/storage/roms/savestates/$system/$stem.state.auto.png'"
    rsh "export XDG_RUNTIME_DIR=/var/run/0-runtime-dir WAYLAND_DISPLAY=wayland-1 SWAYSOCK=/var/run/0-runtime-dir/sway-ipc.0.sock; /usr/bin/runemu.sh '$rom' -P$system --core=$core --emulator=retroarch --controllers= >/tmp/pcs_ra.log 2>&1 & sleep 10; $DIR/bench.sh $secs retroarch 'retroarch $core'; pkill -TERM retroarch; sleep 3; pkill -KILL retroarch; true"
    ;;
  *)
    sed -n 2,16p "$0" >&2
    exit 2
    ;;
esac
