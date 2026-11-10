#!/bin/sh
# pocket-ui SD image, first start only: splits the card into STORAGE and a GAMES drive a Mac or Windows PC can open.
#
# ROCKNIX's fs-resize would grow STORAGE over the whole card, and the system can't be changed for one script (SYSTEM is
# pinned), so deploy/image/mount-storage.sh binds this file over /usr/lib/rocknix/fs-resize for the first boot. ROCKNIX's
# init starts fs-resize.target as long as /storage/.please_resize_me exists; that marker goes last here, so a power cut
# anywhere means the next start runs this again and picks up from what's on the card:
#
#   p1 ROCKNIX (FAT, boot)   p2 STORAGE (ext4) STORAGE_MB   p3 GAMES (exFAT, partition type 7) the rest
#
# - Every step is safe to repeat: the table is what it should be, resize2fs has nothing to do, a good GAMES stays.
# - A table with two partitions and a p3 that isn't ours are told apart by .pocket-games-state, written before the
#   table changes. A p3 we didn't make is never touched; nor is any drive called GAMES that was there before, so
#   flashing a card again with the same layout keeps its games (the old bytes are still there behind the new table).
# - mkfs only runs on a p3 with nothing on it, or one whose formatting this script began and which doesn't check out.
# - Anything else that doesn't work, a card under MIN_CARD_MB, or more than MAX_TRIES tries: ROCKNIX's own fs-resize
#   runs, as it would have without this file. That removes the marker first, grows p2 as far as it can and starts the
#   card with its games in STORAGE: today's layout. Worst case is a smaller STORAGE, never a card that doesn't start.
# /flash stays read-only all the way (ROCKNIX's script writes its log there and changes the FAT's id; neither is needed).
#
# Tests: deploy/image/test-mount-storage.sh (part of the gate). The PU_ names are for those tests only.
S=${PU_STORAGE:-/storage}
STOCK=${PU_STOCK:-/usr/lib/rocknix/fs-resize}
MOUNTS=${PU_MOUNTS:-/proc/mounts}
BLOCK=${PU_BLOCK:-/sys/class/block}
DIRS=${PU_DIRS:-/usr/config/system-dirs.conf}
LOG=${PU_LOG:-/run/pocket-games.log}
WORK=${PU_WORK:-/run/pocket-games}
STORAGE_MB=6144
MIN_CARD_MB=12288
MAX_TRIES=3
MARK=$S/.please_resize_me
STATE=$S/.pocket-games-state
TRIES=$S/.pocket-games-tries

say() { echo "$*"; echo "$*" >>"$LOG" 2>/dev/null; }
run() { echo "+ $*" >>"$LOG" 2>/dev/null; "$@" >>"$LOG" 2>&1; }
restart() { sync; reboot -f; exit 0; }
storage_up() { grep -q " $S " "$MOUNTS" || run mount -o rw,noatime "$PART" "$S"; }

# ROCKNIX's script, with STORAGE mounted and the marker there, as it expects them. Its own file comes back from below
# this one (umount -l: this file is open, it's being read as it runs).
stock() {
  say "pocket-ui: $1. Using the whole card for STORAGE."
  [ -n "${PART:-}" ] && storage_up
  rm -f "$STATE" "$TRIES"
  sync
  umount -l "$STOCK" 2>/dev/null
  exec "$STOCK"
}

# p3 goes again when it couldn't be formatted, then the rest as ROCKNIX would do it.
give_up_games() {
  run parted -s -f "$DISK" rm 3
  stock "$1"
}

# One value of "parted -m unit s print": partition $1, field $2 (2 start, 3 end).
table_field() { parted -s -m "$DISK" unit s print 2>/dev/null | grep "^$1:" | cut -d: -f"$2" | tr -d s; }
fs_field() { blkid -p -o value -s "$1" "$2" 2>/dev/null; }

say "pocket-ui: setting up the card. Please leave the handheld on, this takes a minute."
PART=$(grep " $S " "$MOUNTS" | cut -d" " -f1 | grep '[0-9]$')
[ -n "$PART" ] || stock "STORAGE wasn't found"
case $PART in
  /dev/mmcblk* | /dev/nvme* | /dev/loop*) DISK=${PART%p[0-9]*} ;;
  *) DISK=${PART%[0-9]*} ;;
esac
[ "${PART##*[!0-9]}" = 2 ] || stock "STORAGE isn't the second partition"
# Started before: ROCKNIX's script says so and leaves everything alone.
[ -d "$S/.config" ] || [ -d "$S/.cache" ] && stock "this card was already set up"

tried=$(cat "$TRIES" 2>/dev/null | wc -c)
[ "${tried:-0}" -lt $MAX_TRIES ] || stock "the card couldn't be split in $MAX_TRIES tries"
printf x >>"$TRIES"
sync

card=$(cat "$BLOCK/${DISK##*/}/size" 2>/dev/null)
[ "${card:-0}" -ge $((MIN_CARD_MB * 2048)) ] || stock "this card is too small to split"
start2=$(table_field 2 2)
end2=$(table_field 2 3)
start3=$(table_field 3 2)
[ -n "$start2" ] && [ -n "$end2" ] || stock "the partition table can't be read"
[ -z "$(table_field 4 2)" ] || stock "this card has more partitions than expected"
# Whole 4 MiB for GAMES' start, whole MiB for its end, as SD cards like it. The table is MBR (2 TiB at most).
want3=$(((start2 + STORAGE_MB * 2048 + 8191) / 8192 * 8192))
last3=$((card / 2048 * 2048 - 1))
[ "$end2" -lt "$want3" ] || stock "STORAGE is already larger than planned"
if [ -n "$start3" ]; then
  [ "$start3" = "$want3" ] && [ -f "$STATE" ] || stock "partition 3 isn't one of ours"
  fresh=0
else
  echo table >"$STATE"
  sync
  fresh=1
fi

umount "$S" || stock "STORAGE couldn't be released"
if [ "$end2" -lt $((want3 - 1)) ]; then
  say "pocket-ui: making room for the GAMES drive"
  run parted -s -f -a none "$DISK" unit s resizepart 2 $((want3 - 1)) || stock "STORAGE's partition couldn't be changed"
fi
if [ $fresh = 1 ]; then
  # "ntfs" is how parted writes partition type 7, which Windows wants for exFAT.
  run parted -s -f -a none "$DISK" unit s mkpart primary ntfs "$want3" "$last3" || stock "the GAMES partition couldn't be made"
fi
run partprobe "$DISK"
P3=${PART%2}3
n=0
while { [ ! -f "$BLOCK/${P3##*/}/size" ] || [ "$(cat "$BLOCK/${PART##*/}/size")" != $((want3 - start2)) ]; } && [ $n -lt 10 ]; do
  sleep 1
  n=$((n + 1))
done
# The kernel still has the old table: it reads the new one at the next start.
[ -f "$BLOCK/${P3##*/}/size" ] && [ "$(cat "$BLOCK/${PART##*/}/size")" = $((want3 - start2)) ] || restart

say "pocket-ui: growing STORAGE"
run e2fsck -f -p "$PART"
[ $? -lt 4 ] || run e2fsck -f -y "$PART"
run resize2fs "$PART"
run tune2fs "$PART" -U random
storage_up || { run e2fsck -f -y "$PART"; storage_up; } || restart

kind=$(fs_field TYPE "$P3")
label=$(fs_field LABEL "$P3")
state=$(cat "$STATE" 2>/dev/null)
format=0
if [ "$kind" = exfat ] && [ "$label" = GAMES ]; then
  if [ "$state" = formatting ]; then
    run fsck.exfat -n "$P3" || format=1
  else
    say "pocket-ui: GAMES drive found, keeping its games"
    run fsck.exfat -p "$P3"
  fi
elif [ $fresh = 1 ] || [ "$state" = formatting ] || [ -z "$kind" ]; then
  format=1
fi
if [ $format = 1 ]; then
  say "pocket-ui: making the GAMES drive"
  echo formatting >"$STATE"
  sync
  { run mkfs.exfat -L GAMES "$P3" && run fsck.exfat -n "$P3"; } || give_up_games "the GAMES drive couldn't be made"
fi

# The folders ROCKNIX and pocket-ui read, ready when the card first goes into a computer. ROCKNIX's automount adds any
# that are missing at every start.
if [ "$(fs_field TYPE "$P3")" = exfat ] && mkdir -p "$WORK" && run mount -t exfat "$P3" "$WORK"; then
  mkdir -p "$WORK/roms"
  while read -r kind path _; do
    case $kind:$path in d:/storage/roms/?*) mkdir -p "$WORK/roms/${path#/storage/roms/}" ;; esac
  done <"$DIRS"
  printf 'Put your games in the "roms" folder here, each in the folder of its system: roms/gba, roms/snes and so on.\r\nThen eject this drive and put the card back in the handheld.\r\n' >"$WORK/READ ME - games go in roms.txt"
  sync
  run umount "$WORK"
elif [ $format = 1 ]; then
  give_up_games "the GAMES drive couldn't be opened"
fi

cp "$LOG" "$S/.pocket-games-first-start.log" 2>/dev/null
# Commit the completed card before retiring the grow request. Keep ownership state
# until that removal is durable: a cut while MARK remains must still recognise p3.
sync || restart
rm -f "$MARK" || restart
sync || restart
rm -f "$STATE" "$TRIES"
sync
say "pocket-ui: card ready, restarting"
umount "$S" 2>/dev/null
restart
