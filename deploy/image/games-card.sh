#!/bin/bash
# pocket-ui SD image: a games card from ArkOS, dArkOS or a stock R36S in the second slot is used exactly as it is.
# Runs as rocknix-automount.service's ExecStart (drop-in rocknix-automount-games-card.conf), at every start and whenever
# udev restarts the unit for a card put in (99-automount.rules). docs/SD-IMAGE.md, docs/OTHER-LAYOUTS.md.
#
# ROCKNIX's /usr/bin/automount mounts the first big FAT/exFAT/NTFS/ext4 partition of a card, writes a probe file on it,
# creates <card>/roms with ~100 empty system folders and shows only <card>/roms as /storage/roms. ArkOS cards keep their
# system folders at the partition's root (gba/, snes/, psx/, bios/ ... on EASYROMS), so that would hide every game and
# litter the card. This script looks first, read-only:
# - an ArkOS card (no entry "roms" at its root in any case, and at least two known system folders there): mounted on
#   /storage/games-external as automount would, but nothing is written on it (no probe file, no fsck, no folders);
#   /storage/roms is the internal folder as without a card; the card's BIOS files are copied into it where missing;
#   /run/pocket-ui/games-card-root names the card's root for pocket-ui.
# - anything else (a ROCKNIX card with roms/, an Anbernic stock card with Roms/, a blank card, no card, the probe failing):
#   ROCKNIX's automount runs, unchanged.
# A failure on the ArkOS path ends with the internal folder only: automount never gets an ArkOS card. Never fails the unit.
# Tests: deploy/image/test-games-card.sh. The PU_ names are for those tests only.
ARGS=("$@")
MOUNTS=${PU_MOUNTS:-/proc/mounts}
PARTITIONS=${PU_PARTITIONS:-/proc/partitions}
SYSBLOCK=${PU_SYSBLOCK:-/sys/class/block}
STORAGE=${PU_STORAGE:-/storage}
EXT=${PU_EXTERNAL:-/storage/games-external}
ROMS=${PU_ROMS:-/storage/roms}
INT=${PU_INTERNAL:-/storage/games-internal}
RUN=${PU_RUN:-/run/pocket-ui}
LOG=${PU_LOG:-/var/log/pocket-ui-games-card.log}
AUTOMOUNT=${PU_AUTOMOUNT:-/usr/bin/automount}
PROFILE=${PU_PROFILE:-/etc/profile}
BB=${PU_BUSYBOX:-/usr/bin/busybox}
DIRS_CONF=${PU_DIRS_CONF:-/usr/config/system-dirs.conf}
PROBE=$RUN/probe
MARKER=$RUN/games-card-root
# A BIOS file bigger than this stays on the card only (a mistake, not a BIOS).
BIOS_MAX=${PU_BIOS_MAX:-$((512 * 1024 * 1024))}
# The folders an ArkOS/dArkOS/R36S stock card has at its root (two are enough to tell it from a blank or another card).
KNOWN=" gba gb gbc nes snes psx n64 megadrive genesis mastersystem gamegear pcengine nds psp dreamcast arcade mame fbneo \
neogeo atari2600 ngp ngpc wonderswan wonderswancolor sfc famicom segacd sega32x atarilynx "

log() { { echo "$*" >>"$LOG"; } 2>/dev/null; }

# ROCKNIX's own way, unchanged.
automount() {
  log "-> ROCKNIX's automount ($1)"
  rm -f "$MARKER"
  exec "$AUTOMOUNT" "${ARGS[@]}"
  log "automount could not start"
  exit 0
}

# The name of the entry at the root of $1 that is $2 in any case (exFAT and FAT are case-insensitive), or nothing.
entry() {
  local p n
  for p in "$1"/*; do
    n=${p##*/}
    [ "${n,,}" = "$2" ] && { echo "$n"; return 0; }
  done
  return 1
}

# An ArkOS card: no roms entry at the root, and at least two known system folders there.
arkos_card() {
  local p n count=0
  entry "$1" roms >/dev/null && return 1
  for p in "$1"/*; do
    n=${p##*/}
    case $KNOWN in *" ${n,,} "*) [ -d "$p" ] && count=$((count + 1)) ;; esac
  done
  [ "$count" -ge 2 ]
}

# The whole device of a partition: mmcblk1p3 -> mmcblk1, nvme0n1p1 -> nvme0n1, sda1 -> sda.
root_of() {
  local n=${1##*/}
  case $n in
    mmcblk*p[0-9]* | nvme*p[0-9]*) echo "${n%p[0-9]*}" ;;
    sd*) echo "${n%%[0-9]*}" ;;
    *) echo "$n" ;;
  esac
}

[ "$(stat -c %s "$LOG" 2>/dev/null || echo 0)" -gt 262144 ] && mv -f "$LOG" "$LOG.old" 2>/dev/null
log "== $(date 2>/dev/null)"
mkdir -p "$RUN" 2>/dev/null
rm -f "$MARKER"

# get_setting and set_setting, as automount's own `. /etc/profile` gives them.
[ -r "$PROFILE" ] && . "$PROFILE" >/dev/null 2>&1
type get_setting >/dev/null 2>&1 || get_setting() { :; }
type set_setting >/dev/null 2>&1 || set_setting() { :; }
shopt -s nullglob dotglob execfail

[ "$(get_setting system.automount)" = 0 ] && automount "automount switched off in ROCKNIX's settings"
[ -n "$(get_setting system.gamesdevice)" ] && automount "a games device is set in ROCKNIX's settings"
[ -e "$STORAGE/.please_resize_me" ] && automount "first start, STORAGE not grown yet"
grep -q " $STORAGE " "$MOUNTS" 2>/dev/null || automount "STORAGE is not mounted"

# What automount loads before it looks (the probe needs the file systems too).
for m in exfat vfat ntfs ntfs3; do modprobe "$m" >/dev/null 2>&1; done

# Clean up first, as automount does: a card mounted by the last run (udev restarts the unit for a card put in) is free again.
umount -f "$EXT" >/dev/null 2>&1
umount -f "$ROMS" >/dev/null 2>&1

# Not the card the system runs from.
own=" "
for mp in "$STORAGE" /flash; do
  d=$(grep " $mp " "$MOUNTS" 2>/dev/null | head -n 1 | cut -d" " -f1)
  [ -n "$d" ] && own="$own$(root_of "$d") "
done

# The partitions automount would look at, in its order (all mmcblk, then sd, then nvme, each sort -r), FAT/exFAT/NTFS only.
table=$(blkid 2>/dev/null)
found= ftype=
for pat in 'mmcblk[0-9]' 'sd[a-z]' 'nvme[0-9]n[0-9]'; do
  [ -n "$found" ] && break
  while read -r part type; do
    [ -n "$part" ] || continue
    case $type in *fat* | ntfs) ;; *) continue ;; esac
    root=$(root_of "$part")
    case $own in *" $root "*) continue ;; esac
    size=$(awk -v r="$root" '$4 == r {print $3}' "$PARTITIONS" 2>/dev/null)
    [ "${size:-0}" -gt 8388608 ] 2>/dev/null || { log "$part: device under 8 GiB"; continue; }
    [ -L "$SYSBLOCK/${root}boot0" ] && { log "$part: an Android boot device"; continue; }
    grep -q "^$part " "$MOUNTS" 2>/dev/null && { log "$part: already mounted"; continue; }
    margs=()
    [ "$type" = ntfs ] && margs=(-t ntfs3)
    mkdir -p "$PROBE" 2>/dev/null
    if "$BB" mount -o ro "${margs[@]}" "$part" "$PROBE" >/dev/null 2>&1; then
      if arkos_card "$PROBE"; then
        found=$part ftype=$type
        log "$part ($type): an ArkOS card (system folders at its root)"
      else
        log "$part ($type): not an ArkOS card"
      fi
      "$BB" umount "$PROBE" >/dev/null 2>&1 || umount -l "$PROBE" >/dev/null 2>&1
    else
      log "$part ($type): could not be looked at"
    fi
    [ -n "$found" ] && break
  done < <(printf '%s\n' "$table" | grep "^/dev/$pat" | sed -n 's/^\([^:]*\):.* TYPE="\([^"]*\)".*/\1 \2/p' | sort -r)
done
[ -n "$found" ] || automount "no ArkOS card"
if grep -q " $PROBE " "$MOUNTS" 2>/dev/null; then
  log "the probe could not be unmounted: games from STORAGE only"
  found=
fi

card=0
if [ -n "$found" ]; then
  margs=()
  [ "$ftype" = ntfs ] && margs=(-t ntfs3)
  mkdir -p "$EXT" 2>/dev/null
  if "$BB" mount "${margs[@]}" "$found" "$EXT" >/dev/null 2>&1; then
    card=1
    log "$found on $EXT, as it is"
  else
    log "$found could not be mounted on $EXT: games from STORAGE only, the card untouched"
  fi
fi

# The internal folder as /storage/roms, as automount's start_ms internal.
mkdir -p "$INT/roms" 2>/dev/null
mount --bind "$INT/roms" "$ROMS" >/dev/null 2>&1 || log "could not bind $INT/roms on $ROMS"
# ROCKNIX's system folders, only where /storage/roms is STORAGE's (the last mount on it), never on the card.
last=$(grep " $ROMS " "$MOUNTS" 2>/dev/null | tail -n 1 | cut -d" " -f1)
if [ -e "$DIRS_CONF" ] && [ -n "$last" ] && [ "$last" != "$found" ]; then
  systemd-tmpfiles --create "$DIRS_CONF" >/dev/null 2>&1
fi
[ -z "$(get_setting system.merged.device)" ] && set_setting system.merged.device internal

if [ $card = 1 ]; then
  # BIOS files: copied where missing, never over one that is there, nothing written on the card.
  bios=$(entry "$EXT" bios)
  if [ -n "$bios" ] && [ -d "$EXT/$bios" ]; then
    to=$INT/roms/bios
    while IFS= read -r -d '' f; do
      rel=${f#"$EXT/$bios/"}
      [ -e "$to/$rel" ] && continue
      bytes=$(stat -c %s "$f" 2>/dev/null || echo 0)
      [ "$bytes" -le "$BIOS_MAX" ] || { log "bios: $rel left on the card ($bytes bytes)"; continue; }
      if mkdir -p "$(dirname "$to/$rel")" 2>/dev/null && cp "$f" "$to/$rel.pocket-part" 2>/dev/null &&
        mv "$to/$rel.pocket-part" "$to/$rel" 2>/dev/null; then
        log "bios: copied $rel"
      else
        rm -f "$to/$rel.pocket-part"
        log "bios: could not copy $rel"
      fi
    done < <(find "$EXT/$bios/" -type f -print0 2>/dev/null)
  fi
  { echo "$EXT" >"$MARKER"; } 2>/dev/null || log "could not write $MARKER"
fi
exit 0
