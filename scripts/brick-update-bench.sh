#!/bin/bash
# What a Brick app update writes to a sync-mounted FAT32 card (docs/UPDATE-CHANNEL.md): installs OLD_TAR on an empty image, then NEW_TAR
# over it with the updater's own code, and prints the bytes and writes the loop device saw for the second step.
#   scripts/brick-update-bench.sh TEST_BIN OLD_TAR OLD_VERSION NEW_TAR NEW_VERSION
# TEST_BIN is the lib test binary (cargo test --features brick --lib --no-run) containing update::...::bench_install. Needs sudo for
# losetup/mount; touches nothing but its own image and mount point under /tmp.
set -eu
BIN=$1 OLD=$2 OLDV=$3 NEW=$4 NEWV=$5
W=$(mktemp -d /tmp/brick-bench.XXXXXX); IMG=$W/card.img; M=$W/mnt; mkdir "$M"
LOOP=
cleanup() { mountpoint -q "$M" && sudo umount "$M"; [ -z "$LOOP" ] || sudo losetup -d "$LOOP"; rm -rf "$W"; }
trap cleanup EXIT
truncate -s 600M "$IMG"; mkfs.vfat -F 32 -s 64 -S 512 "$IMG" >/dev/null 2>&1
LOOP=$(sudo losetup -f --show "$IMG")
sudo mount -o sync,flush,uid="$(id -u)",gid="$(id -g)",shortname=mixed "$LOOP" "$M"
mkdir "$M/pocket-ui"
stat() { awk '{print $5, $7}' "/sys/block/$(basename "$LOOP")/stat"; }
run() { # tarball version label
  echo "$2" >"$W/version"
  read -r io0 sec0 < <(stat); t0=$(date +%s.%N)
  BENCH_CARD="$M/pocket-ui" BENCH_TAR="$1" BENCH_VERSION_FILE="$W/version" \
    "$BIN" --exact update::brick_card_tests::delta_tests::bench_install --ignored --nocapture >"$W/out" 2>&1 || { cat "$W/out"; exit 1; }
  read -r io1 sec1 < <(stat); t1=$(date +%s.%N)
  printf '%s: %.1f MB written, %d write I/Os, %.1f s (unthrottled)\n' "$3" "$(echo "($sec1 - $sec0) * 512 / 1000000" | bc -l)" $((io1 - io0)) "$(echo "$t1 - $t0" | bc -l)"
}
run "$OLD" "$OLDV" "install $OLDV on an empty card"
run "$NEW" "$NEWV" "update $OLDV -> $NEWV"
echo "app is $(cat "$M/pocket-ui/app/RELEASE"), previous is $(cat "$M/pocket-ui/app.previous/RELEASE" 2>/dev/null || echo none); moved: $(grep -c '^move:' "$M/pocket-ui/app.previous/.asset-moves" 2>/dev/null || echo 0) entries"
