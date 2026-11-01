#!/bin/sh
# pocket-ui SD image: runs right after ROCKNIX's automount (rocknix-automount-games.conf) at every start. Only for the
# GAMES drive on the card the system runs from; a games card in the second slot stays exactly as automount left it.
# Mounts GAMES again with what automount doesn't give it:
# - noatime and fixed modes. It can't be done with a remount: exFAT ignores options there. File sharing, pocket-ui and the
#   emulators all write as root, so the drive stays root's.
# - a check first when the drive wasn't closed properly (a power cut, the card pulled from a computer): exFAT has no
#   journal to replay, so this does what ext4 does for STORAGE by itself. Skipped on a clean drive: no cost at a normal start.
# Never fails the start: when the new mount doesn't work, the old way comes back, then the games in STORAGE.
# Tests: deploy/image/test-mount-storage.sh. The PU_ names are for those tests only.
MOUNTS=${PU_MOUNTS:-/proc/mounts}
EXT=${PU_EXTERNAL:-/storage/games-external}
ROMS=${PU_ROMS:-/storage/roms}
INT=${PU_INTERNAL:-/storage/games-internal}
line=$(grep " $EXT " "$MOUNTS") || exit 0
set -- $line
dev=$1
[ "$3" = exfat ] || exit 0
[ "$(blkid -o value -s LABEL "$dev" 2>/dev/null)" = GAMES ] || exit 0
storage=$(grep " ${PU_STORAGE:-/storage} " "$MOUNTS" | cut -d" " -f1)
[ "${dev%p[0-9]*}" = "${storage%p[0-9]*}" ] || exit 0
umount "$ROMS" 2>/dev/null
if umount "$EXT"; then
  # exFAT's VolumeFlags, 106 bytes into the boot sector: bit 1 is "not closed properly".
  flags=$(dd if="$dev" bs=1 skip=106 count=1 2>/dev/null | od -An -tu1 | tr -d " ")
  case $flags in '' | *[!0-9]*) flags=0 ;; esac
  [ $((flags & 2)) = 0 ] || timeout 120 fsck.exfat -p "$dev" >/dev/null 2>&1
  mount -t exfat -o noatime,fmask=0022,dmask=0022 "$dev" "$EXT" || mount -t exfat "$dev" "$EXT"
fi
if grep -q " $EXT " "$MOUNTS" && [ -d "$EXT/roms" ]; then
  mount --bind "$EXT/roms" "$ROMS"
else
  mount --bind "$INT/roms" "$ROMS"
fi
exit 0
