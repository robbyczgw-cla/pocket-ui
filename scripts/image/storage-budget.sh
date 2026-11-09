#!/bin/sh
# Do the programs still fit into the SD image's STORAGE, with room to grow? ROCKNIX's STORAGE is a 32 MiB ext4 that stays that
# size until the first start (then the card's whole space), and scripts/image/build.sh writes pocket-ui into it. This adds up
# what goes in, in 4 KiB blocks, against the blocks ROCKNIX leaves free (pins.sh P2_FREE_BLOCKS). No Docker, no image: seconds.
# 0.3.1 ran into the limit (27 blocks short, 2026-11-08); release-check.sh runs this on the release binaries.
#
#   sh scripts/image/storage-budget.sh BIN_DIR       BIN_DIR: the AArch64 pocket-ui, pocket-core, pocket-splash (+ cores/ if any)
#
# Exit 0: fits, with at least P2_MIN_HEADROOM_BLOCKS free. 1: does not fit or too little room. 2: usage, programs missing.
# The licence texts are not counted: STORAGE links to the copy on the FAT partition (pocket-ui-info/licences), see build.sh.
set -u
cd "$(dirname "$0")/../.." || exit 2
. scripts/image/pins.sh
BIN=${1:-}
[ -n "$BIN" ] && [ -d "$BIN" ] || { sed -n '5,8p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
for f in pocket-ui pocket-core pocket-splash; do
  [ -s "$BIN/$f" ] || { echo "storage-budget: $BIN/$f is missing (it needs the AArch64 programs, not the host build)" >&2; exit 2; }
done
blocks() { echo $((($(stat -c %s "$1") + 4095) / 4096)); }
need=0
line() { printf '  %6d blocks  %s\n' "$2" "$1"; need=$((need + $2)); }
echo "STORAGE budget (ROCKNIX leaves $P2_FREE_BLOCKS of 8192 blocks free):"
for f in pocket-ui pocket-core pocket-splash; do line "$f ($(stat -c %s "$BIN/$f") bytes)" "$(blocks "$BIN/$f")"; done
if [ -d "$BIN/cores" ]; then
  n=0; for f in $(find "$BIN/cores" -type f); do n=$((n + $(blocks "$f"))); done
  line "cores/" "$n"
fi
# What build.sh also writes (launcher, helper, systemd drop-ins, defaults; the GAMES drive's two files counted on every model).
n=0
for f in deploy/portmaster-setup.sh deploy/run-pocket-ui.sh deploy/essway-pocket-ui.conf deploy/pocket-splash.service deploy/image/image-defaults.sh \
         deploy/image/games-mount.sh deploy/image/rocknix-automount-games.conf; do n=$((n + $(blocks "$f"))); done
line "launcher, helper, drop-ins, defaults" "$n"
# One block per directory: pocket-ui, releases, the version, .pocket-seed and its five folders, the GAMES drop-in.
line "directories" 11
left=$((P2_FREE_BLOCKS - need))
echo "  ------"
printf '  %6d blocks  needed, %d free, %d left = %d KiB (at least %d blocks = %d KiB kept for growth)\n' \
  "$need" "$P2_FREE_BLOCKS" "$left" $((left * 4)) "$P2_MIN_HEADROOM_BLOCKS" $((P2_MIN_HEADROOM_BLOCKS * 4))
if [ "$left" -lt 0 ]; then
  echo "STORAGE BUDGET: does NOT fit, $((-left)) blocks short. The image build would stop with 'Could not allocate block'." >&2; exit 1
elif [ "$left" -lt "$P2_MIN_HEADROOM_BLOCKS" ]; then
  echo "STORAGE BUDGET: fits, but only $left blocks are left (need $P2_MIN_HEADROOM_BLOCKS): the next release will not." >&2; exit 1
fi
echo "STORAGE BUDGET: ok"
