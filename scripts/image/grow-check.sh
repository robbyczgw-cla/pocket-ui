#!/bin/sh
# Would this STORAGE survive the first boot? Runs ROCKNIX's fs-resize steps (e2fsck -f -p, resize2fs, tune2fs -U random) on
# sparse copies at card sizes, then the check init runs at the next boot (fsck -p; anything but 0 and 1 is "Filesystem
# corruption"). fs-resize never looks at what resize2fs returns and prints "done" anyway, so a failure here is a card
# that breaks at its second boot. Only reads $1; the copies stay next to it.
#   scripts/image/grow-check.sh <STORAGE partition file> [GB ...]
# Sizes are whole-card sizes as sold; both tools come from scripts/image/Dockerfile.
set -u
P=$1
shift
[ $# -gt 0 ] || set -- 8 16 32 64 128 256 512 1024 2048
C=$P.grow
rc=0
for gb in "$@"; do
  cp --sparse=always "$P" "$C" || exit 1
  # Partition 2 starts where pins.sh says and runs to the end of the card.
  truncate -s $((gb * 1000000000 - P2_START * 512)) "$C" || exit 1
  e2fsck -f -p "$C" >"$C.log" 2>&1
  a=$?
  resize2fs "$C" >>"$C.log" 2>&1
  b=$?
  tune2fs "$C" -U random >>"$C.log" 2>&1
  c=$?
  e2fsck -f -p "$C" >>"$C.log" 2>&1
  d=$?
  if [ "$a" -gt 1 ] || [ "$b" != 0 ] || [ "$c" != 0 ] || [ "$d" -gt 1 ]; then
    echo "     $gb GB card: e2fsck $a, resize2fs $b, tune2fs $c, next boot's fsck $d: $(grep -m1 -E 'Illegal|while trying|No space' "$C.log")"
    rc=1
  fi
done
rm -f "$C" "$C.log"
exit $rc
