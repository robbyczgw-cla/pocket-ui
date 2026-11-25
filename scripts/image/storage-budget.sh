#!/bin/sh
# Do the programs still fit into the SD image's STORAGE, with room to grow? STORAGE is a 128 MiB ext4 in the image (ROCKNIX's 32 MiB,
# grown by scripts/image/build.sh; pins.sh P2_SIZE) that stays that size until the first start (then the card's whole space), and
# build.sh writes pocket-ui into it. This adds up what goes in, in 4 KiB blocks, against the blocks that are free in the grown
# STORAGE (pins.sh P2_FREE_BLOCKS). No Docker, no image: seconds. With ROCKNIX's 32 MiB, 0.3.1 ran into the limit (27 blocks short,
# 2026-11-08) and 0.4.2 had 307 blocks spare; release-check.sh runs this on the release binaries.
#
#   sh scripts/image/storage-budget.sh BIN_DIR       BIN_DIR: the AArch64 pocket-ui, pocket-core, pocket-splash (+ cores/ if any)
#
# Exit 0: fits (under the 2 MiB growth wish it prints a NOTE: raise STORAGE generously, don't compress). 1: does not fit.
# 2: usage, programs missing. Historical image receipts retain pins.sh's original finished-image minimum.
# The licence texts are not counted: STORAGE links to the copy on the FAT partition (pocket-ui-info/licences), see build.sh.
set -u
cd "$(dirname "$0")/../.." || exit 2
. scripts/image/pins.sh
# The release programs keep this many blocks (2 MiB) free on top of what goes in, against the 128 MiB STORAGE. Do not
# retrospectively invalidate the checked 0.3.1 images/receipts by changing their pinned minimum (docs/BINARY-SIZE.md).
min_headroom=512
[ "$P2_MIN_HEADROOM_BLOCKS" -le "$min_headroom" ] || min_headroom=$P2_MIN_HEADROOM_BLOCKS
BIN=${1:-}
[ -n "$BIN" ] && [ -d "$BIN" ] || { sed -n '5,8p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
for f in pocket-ui pocket-core pocket-splash; do
  [ -s "$BIN/$f" ] || { echo "storage-budget: $BIN/$f is missing (it needs the AArch64 programs, not the host build)" >&2; exit 2; }
done
blocks() { echo $((($(stat -c %s "$1") + 4095) / 4096)); }
need=0
line() { printf '  %6d blocks  %s\n' "$2" "$1"; need=$((need + $2)); }
echo "STORAGE budget ($P2_FREE_BLOCKS of $((P2_SIZE / 8)) blocks are free in the image's STORAGE):"
for f in pocket-ui pocket-core pocket-splash; do line "$f ($(stat -c %s "$BIN/$f") bytes)" "$(blocks "$BIN/$f")"; done
if [ -d "$BIN/cores" ]; then
  n=0; for f in $(find "$BIN/cores" -type f); do n=$((n + $(blocks "$f"))); done
  line "cores/" "$n"
fi
# The Chinese font files build.sh writes next to the programs (scripts/shipped-fonts.sh).
n=0
for f in $(sed -n 's/^FONTS="\(.*\)"$/\1/p' scripts/shipped-fonts.sh); do n=$((n + $(blocks "assets/fonts/$f"))); done
line "fonts/" "$n"
# What build.sh also writes (launcher, helper, systemd drop-ins, defaults, the ArkOS card's mount; the GAMES drive's two files counted on every model).
n=0
for f in deploy/portmaster-setup.sh deploy/run-pocket-ui.sh deploy/essway-pocket-ui.conf deploy/pocket-splash.service deploy/image/image-defaults.sh \
         deploy/image/games-mount.sh deploy/image/rocknix-automount-games.conf deploy/image/games-card.sh deploy/image/rocknix-automount-games-card.conf; do n=$((n + $(blocks "$f"))); done
line "launcher, helper, drop-ins, defaults" "$n"
# One block per directory: pocket-ui, releases, the version, its fonts, .pocket-seed and its five folders, the automount drop-ins.
line "directories" 12
left=$((P2_FREE_BLOCKS - need))
echo "  ------"
printf '  %6d blocks  needed, %d free, %d left = %d KiB (at least %d blocks = %d KiB kept for growth)\n' \
  "$need" "$P2_FREE_BLOCKS" "$left" $((left * 4)) "$min_headroom" $((min_headroom * 4))
if [ "$left" -lt 0 ]; then
  echo "STORAGE BUDGET: does NOT fit, $((-left)) blocks short. The image build would stop with 'Could not allocate block'." >&2; exit 1
elif [ "$left" -lt "$min_headroom" ]; then
  # A limit we own (14.11.2026): only "does not fit" stops a release; a small reserve is a note to grow STORAGE in pins.sh.
  echo "STORAGE BUDGET: NOTE fits, but only $left blocks are left (wish: $min_headroom): raise STORAGE generously (pins.sh), don't compress." >&2
  exit 0
fi
echo "STORAGE BUDGET: ok"
