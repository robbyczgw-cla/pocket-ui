#!/bin/sh
# Virtual Mini V2 (or Pocket 5, Flip 2, Mini V1: up --device): an aarch64 container with the real ROCKNIX userland, a headless sway at the device's size,
# llvmpipe, PipeWire (null sink) and a uinput pad. Same interface as dev.sh, no device needed.
#   vdev.sh up [--device D] | down   start / remove the container (fast after the first image build); D: miniv2
#                                    (default), rp5, flip2, mini1: that device's screen, touch panel and device tree
#   vdev.sh lid close|open           the Flip 2's lid (a container started with --device flip2)
#   vdev.sh deploy [--no-build]      zigbuild, copy into the container, (re)start pocket-ui
#   vdev.sh send up down a           drive the UI through its command FIFO
#   vdev.sh pad press A 150          in-game input through the virtual pad (see docs/VIRTUAL-DEVICE.md)
#   vdev.sh shot NAME                screenshot to screenshots/NAME.png
#   vdev.sh log [-f] [NAME]          pocket-ui log (default), or boot|sway|pipewire|pad|core
#   vdev.sh sh [CMD...]              shell (or one command) inside
#   vdev.sh run-core CORE ROM [--bench N]   run pocket-core directly; CORE like gambatte, ROM a path
#   vdev.sh stop                     stop pocket-ui (the container stays up)
#   vdev.sh ls                       all virtual devices
#   vdev.sh image [--rebuild-base]   build the images (up does it when missing)
# Several at once: VDEV_NAME=<name> vdev.sh ...   (default name: dev)
# Other knobs: VDEV_ROMS=<host dir>, VDEV_CPUS=<n>, VDEV_ONBOARDING=1, VDEV_LIVE=1, ROCKNIX_IMG=<.img>, VDEV_DOCKER_ARGS="<docker run args>"
set -eu
ROOT=$(cd "$(dirname "$0")/.." && pwd)
NAME=${VDEV_NAME:-dev}
C=vdev-$NAME
CACHE=${VDEV_CACHE:-$HOME/.cache/pocket-vdev}
TARGET=aarch64-unknown-linux-gnu
BIN=$ROOT/target/$TARGET/release
# the launcher script first, or it would restart pocket-ui; [r] keeps pkill from matching its own shell
# BusyBox `pkill -x` missed pocket-ui (agents found four UIs reading the same FIFO), so: pidof, then KILL
# whatever is still there a moment later.
KILL='pkill -f "[r]un-pocket-ui.sh"; kill $(pidof pocket-ui pocket-core pocket-logd) 2>/dev/null; sleep 0.3; kill -9 $(pidof pocket-ui pocket-core pocket-logd) 2>/dev/null; true'

die() { echo "vdev: $*" >&2; exit 1; }
say() { echo "vdev: $*" >&2; }

# --- engine -------------------------------------------------------------------------------------------
engine() {
  docker info >/dev/null 2>&1 && return 0
  command -v docker >/dev/null || die "docker is not installed"
  if command -v colima >/dev/null; then
    say "docker is not running, starting colima (first start takes about a minute)"
    colima start >&2 || die "colima start failed"
    docker info >/dev/null 2>&1 || die "docker still not reachable"
  else
    die "docker is not running (start Docker Desktop or colima)"
  fi
}

running() { [ "$(docker inspect -f '{{.State.Running}}' "$C" 2>/dev/null)" = true ]; }
need_running() { engine; running || die "$C is not running: vdev.sh up"; }
dx() { docker exec "$C" "$@"; }

# --- images -------------------------------------------------------------------------------------------
find_img() {
  if [ -n "${ROCKNIX_IMG:-}" ]; then echo "$ROCKNIX_IMG"; return; fi
  main=$(cd "$ROOT" && cd "$(git rev-parse --git-common-dir 2>/dev/null)/.." 2>/dev/null && pwd || echo "$ROOT")
  for d in "$ROOT/downloads" "$main/downloads" "$main/../downloads"; do
    # newest first: the file names carry the date
    img=$(ls -1 "$d"/rocknix-*/ROCKNIX-SM8250.aarch64-*.img 2>/dev/null | sort | tail -1 || true)
    [ -n "$img" ] && { echo "$img"; return; }
  done
  return 1
}

overlay_hash() { cat "$ROOT/docker/Dockerfile" "$ROOT/docker/mesa-closure.sh" $(find "$ROOT/docker/vdev" -type f | sort) | shasum | cut -c1-10; }

base_image() {
  # the base is named after the ROCKNIX version it was made from; the newest one wins
  docker images --format '{{.Repository}}:{{.Tag}}' pocket-vdev-base 2>/dev/null | sort | tail -1
}

build_base() {
  img=$(find_img) || die "no ROCKNIX image found: set ROCKNIX_IMG=<path to ROCKNIX-SM8250...img> (see docs/VIRTUAL-DEVICE.md)"
  ver=$(basename "$img" | sed -n 's/.*aarch64-\([0-9]*\)\.img/\1/p')
  [ -n "$ver" ] || ver=unknown
  say "unpacking $img (once, about 2 minutes; nothing is copied out of the container)"
  docker build -q -f "$ROOT/docker/Dockerfile.extract" -t pocket-vdev-extract "$ROOT/docker" >/dev/null
  docker run --rm --platform linux/arm64 -v "$img:/in/rocknix.img:ro" \
    -v "$ROOT/docker/extract-rocknix.sh:/extract.sh:ro" pocket-vdev-extract sh /extract.sh \
    | docker import --platform linux/arm64 - "pocket-vdev-base:$ver" >/dev/null
  say "base image pocket-vdev-base:$ver ready"
}

build_overlay() {
  base=$(base_image)
  [ -n "$base" ] || die "no base image"
  docker build -q --platform linux/arm64 --build-arg "BASE=$base" -t "pocket-vdev:$(overlay_hash)" \
    -f "$ROOT/docker/Dockerfile" "$ROOT/docker" >/dev/null
}

ensure_image() {
  engine
  [ -n "$(base_image)" ] || build_base
  docker image inspect "pocket-vdev:$(overlay_hash)" >/dev/null 2>&1 || { say "building the device image"; build_overlay; }
}

# --- instance ----------------------------------------------------------------------------------------
cmd_up() {
  device=${VDEV_DEVICE:-miniv2}
  while [ $# -gt 0 ]; do
    case $1 in
      --device) device=${2:-}; shift 2 || die "--device needs a name: miniv2, rp5, flip2, mini1, rp6, nova, odin2mini, odin2, odin2portal, rg28xx, rg35xx-sp" ;;
      --device=*) device=${1#--device=}; shift ;;
      *) die "unknown option for up: $1" ;;
    esac
  done
  case $device in miniv2 | rp5 | flip2 | mini1 | rp6 | nova | odin2mini | odin2 | odin2portal | rg28xx | rg35xx-sp) ;; *) die "unknown device '$device': miniv2, rp5, flip2, mini1, rp6, nova, odin2mini, odin2, odin2portal, rg28xx or rg35xx-sp" ;; esac
  ensure_image
  if running; then
    have=$(docker inspect -f '{{index .Config.Labels "pocket-vdev.device"}}' "$C" 2>/dev/null)
    [ "${have:-miniv2}" = "$device" ] || say "$C is already running as ${have:-miniv2}, not $device: vdev.sh down first"
    say "$C is already running"
    return 0
  fi
  docker rm -f "$C" >/dev/null 2>&1 || true
  roms=${VDEV_ROMS:-$CACHE/roms}
  set -- --init --privileged --name "$C" --hostname "$C" --label pocket-vdev=1 --label "pocket-vdev.device=$device" \
    -e VDEV_ONBOARDING="${VDEV_ONBOARDING:-0}" -e VDEV_DEVICE="$device"
  [ -d "$roms" ] && set -- "$@" -v "$roms:/mnt/roms:ro"
  [ -n "${VDEV_CPUS:-}" ] && set -- "$@" --cpus "$VDEV_CPUS"
  [ "${VDEV_LIVE:-0}" = 1 ] && set -- "$@" -v "$ROOT/docker/vdev:/vdev:ro"
  # Extra `docker run` arguments, split on spaces: the Wi-Fi bench starts its device with `--network none`
  # (only the simulated wlan0, docs/WIFI-BENCH.md).
  # shellcheck disable=SC2086
  [ -n "${VDEV_DOCKER_ARGS:-}" ] && set -- "$@" $VDEV_DOCKER_ARGS
  start=$(date +%s)
  docker run -d "$@" "pocket-vdev:$(overlay_hash)" >/dev/null
  i=0
  until dx sh -c 'test -e /run/vdev/ready' 2>/dev/null; do
    i=$((i + 1))
    [ $i -gt 150 ] && { dx cat /var/log/vdev-boot.log >&2 || true; die "$C did not come up"; }
    sleep 0.1
  done
  say "$C up in $(( $(date +%s) - start )) s"
  dx cat /var/log/vdev-boot.log | sed 's/^/  /' >&2
}

cmd_down() { engine; docker rm -f "$C" >/dev/null 2>&1 && say "$C removed" || say "$C was not there"; }

cmd_deploy() {
  if [ "${1:-}" != --no-build ]; then
    (cd "$ROOT" && . "$ROOT/scripts/release-rustflags.sh" && cargo zigbuild --release --target $TARGET.2.38 --bin pocket-ui --bin pocket-core)
  fi
  ensure_image
  running || cmd_up
  # stop the running one first (the launcher script, or it would restart it), like dev.sh stops the unit
  dx sh -c "$KILL; mkdir -p /storage/pocket-ui/releases/dev; true"
  docker cp "$BIN/pocket-ui" "$C:/storage/pocket-ui/releases/dev/pocket-ui"
  docker cp "$BIN/pocket-core" "$C:/storage/pocket-ui/releases/dev/pocket-core"
  docker cp "$ROOT/deploy/portmaster-setup.sh" "$C:/storage/pocket-ui/releases/dev/portmaster-setup.sh"
  docker cp "$ROOT/deploy/run-pocket-ui.sh" "$C:/storage/pocket-ui/run-pocket-ui.sh"
  # same layout as dev.sh deploy: releases/dev is current, the binaries are links into it
  dx sh -c 'cd /storage/pocket-ui && chmod +x run-pocket-ui.sh releases/dev/*; rm -f update-pending;
    for f in pocket-ui pocket-core portmaster-setup.sh; do [ -L $f ] || rm -f $f; ln -sfn current/$f $f; done;
    if [ "$(readlink current)" != releases/dev ]; then [ -L current ] && ln -sfn "$(readlink current)" previous; ln -sfn releases/dev current; fi'
  docker exec -d "$C" /storage/pocket-ui/run-pocket-ui.sh
  i=0
  until dx grep -q 'first frame' /var/log/pocket-ui.log 2>/dev/null; do
    i=$((i + 1))
    [ $i -gt 100 ] && { dx cat /var/log/pocket-ui.log >&2; die "pocket-ui did not draw a first frame"; }
    sleep 0.1
  done
  dx cat /var/log/pocket-ui.log
}

cmd_send() {
  need_running
  [ $# -gt 0 ] || die "usage: vdev.sh send up down a ..."
  # The lid is the pad's (a switch, not a button of the UI's FIFO): `send lid close` is `lid close`.
  if [ "$1" = lid ]; then shift; cmd_lid "$@"; return; fi
  dx vsend "$@"
}

cmd_lid() {
  case "${1:-}" in close | open) ;; *) die "usage: vdev.sh lid close|open" ;; esac
  cmd_pad "lid $1"
}

cmd_pad() {
  need_running
  [ $# -gt 0 ] || die "usage: vdev.sh pad 'press A 150' | pad press A 150"
  # one argument = one vpad line; several plain words = one line
  if [ $# -eq 1 ]; then line=$1; else line="$*"; fi
  dx sh -c 'test -p /tmp/vpad.cmd || { echo "no virtual pad (needs /dev/uinput, see the docs)" >&2; exit 1; }; echo "$1" > /tmp/vpad.cmd' sh "$line"
}

cmd_shot() {
  need_running
  mkdir -p "$ROOT/screenshots"
  out=$ROOT/screenshots/${1:-shot}.png
  dx grim - >"$out"
  echo "$out"
}

cmd_log() {
  need_running
  follow=cat
  [ "${1:-}" = -f ] && { follow="tail -f"; shift; }
  case "${1:-pocket-ui}" in
    pocket-ui) f=/var/log/pocket-ui.log ;;
    boot) f=/var/log/vdev-boot.log ;;
    sway) f=/var/log/sway.log ;;
    pipewire) f=/var/log/pipewire.log ;;
    pulse) f=/var/log/pipewire-pulse.log ;;
    pad) f=/var/log/vdev-pad.log ;;
    *) die "unknown log: pocket-ui (also has pocket-core's output), boot, sway, pipewire, pulse, pad" ;;
  esac
  dx $follow "$f"
}

cmd_sh() {
  need_running
  if [ $# -gt 0 ]; then dx sh -c "$*"; else docker exec -it "$C" sh -c 'export PS1="vdev:\w # "; exec bash --norc -i 2>/dev/null || exec sh'; fi
}

cmd_stop() { need_running; dx sh -c "$KILL; true"; }

cmd_ls() {
  engine
  docker ps -a --filter label=pocket-vdev=1 --format 'table {{.Names}}\t{{.Label "pocket-vdev.device"}}\t{{.Status}}\t{{.CreatedAt}}'
}

cmd_run_core() {
  need_running
  [ $# -ge 2 ] || die "usage: vdev.sh run-core CORE ROM [--bench N]   (CORE: gambatte, mgba, nestopia, snes9x or a path)"
  core=$1 rom=$2
  shift 2
  case "$core" in */*) ;; *) core=/usr/lib/libretro/${core%_libretro.so}_libretro.so ;; esac
  if [ -f "$rom" ]; then
    # a file on this Mac: put it into the system folder pocket-core takes its saves from
    ext=$(echo "${rom##*.}" | tr 'A-Z' 'a-z')
    case "$ext" in
      gb) sys=gb ;; gbc) sys=gbc ;; gba) sys=gba ;; nes) sys=nes ;; sfc | smc) sys=snes ;;
      *) sys=${VDEV_SYSTEM:-} ;;
    esac
    [ -n "$sys" ] || die "which system is .$ext? set VDEV_SYSTEM=<system>"
    dx mkdir -p "/storage/roms/$sys"
    docker cp "$rom" "$C:/storage/roms/$sys/$(basename "$rom")"
    rom=/storage/roms/$sys/$(basename "$rom")
  fi
  dx sh -c 'test -x /storage/pocket-ui/pocket-core' || die "pocket-core is not deployed: vdev.sh deploy"
  tty=-i
  [ -t 0 ] && tty=-it
  # with --bench the CPU time is reported too (vtime), the same measure as the device numbers in docs/HOST.md
  wrap=
  for a in "$@"; do [ "$a" = --bench ] && wrap=vtime; done
  docker exec $tty "$C" $wrap /storage/pocket-ui/pocket-core "$core" "$rom" "$@"
}

cmd_image() {
  engine
  [ "${1:-}" = --rebuild-base ] && build_base
  [ -n "$(base_image)" ] || build_base
  build_overlay
  if [ "${1:-}" = --prune ]; then
    # old overlay images (one per change of docker/); those still used by a container stay
    for img in $(docker images --format '{{.Repository}}:{{.Tag}}' pocket-vdev); do
      [ "$img" = "pocket-vdev:$(overlay_hash)" ] || docker rmi "$img" >/dev/null 2>&1 || true
    done
    docker images --format '{{.Repository}}:{{.Tag}}  {{.Size}}' | grep '^pocket-vdev' >&2 || true
  fi
  say "image pocket-vdev:$(overlay_hash) ready"
}

case "${1:-}" in
  up) shift; cmd_up "$@" ;;
  down) cmd_down ;;
  deploy) shift; cmd_deploy "$@" ;;
  send) shift; cmd_send "$@" ;;
  pad) shift; cmd_pad "$@" ;;
  lid) shift; need_running; cmd_lid "$@" ;;
  shot) shift; cmd_shot "$@" ;;
  log) shift; cmd_log "$@" ;;
  sh) shift; cmd_sh "$@" ;;
  stop) cmd_stop ;;
  ls) cmd_ls ;;
  run-core) shift; cmd_run_core "$@" ;;
  image) shift; cmd_image "$@" ;;
  *)
    echo "usage: vdev.sh up [--device miniv2|rp5|flip2|mini1|rp6|nova|odin2mini|odin2|odin2portal|rg28xx|rg35xx-sp]|down|deploy|send CMD...|pad ...|lid close|open|shot NAME|log|sh|run-core CORE ROM|stop|ls|image" >&2
    echo "       VDEV_NAME=<name> picks the instance (default: dev), see the header of this file" >&2
    exit 2
    ;;
esac
