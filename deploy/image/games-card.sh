#!/bin/bash
# pocket-ui SD image: a games card from ArkOS, dArkOS or a stock R36S in the second slot is used exactly as it is.
# Runs as rocknix-automount.service's ExecStart (drop-in rocknix-automount-games-card.conf), at every start and whenever
# udev restarts the unit for a card put in (99-automount.rules). docs/SD-IMAGE.md, docs/OTHER-LAYOUTS.md.
#
# ROCKNIX's /usr/bin/automount mounts the first big FAT/exFAT/NTFS/ext4 partition of a card, writes a probe file on it,
# creates <card>/roms with ~100 empty system folders and shows only <card>/roms as /storage/roms. ArkOS cards keep their
# system folders at the partition's root (gba/, snes/, psx/, bios/ ... on EASYROMS), so that would hide every game and
# litter the card. This script looks first, read-only:
# - an ArkOS card (at least two known system folders at the root, with roms/ absent or containing only empty folders): mounted on
#   /storage/games-external. ext4 cards stay read-only, with journal replay and symlink traversal disabled.
#   The script writes no probe file or folders there and does not run fsck;
#   /storage/roms is the internal folder as without a card; the card's BIOS files are copied into it where missing;
#   /run/pocket-ui/games-card-root names the card's root for pocket-ui.
# - anything else (a populated ROCKNIX roms/, an Anbernic stock card with Roms/, a blank card, no card, the probe failing):
#   ROCKNIX's automount runs. On Qualcomm SD images both mount paths admit only SD cards and USB storage, even if
#   the kernel exposes internal UFS or eMMC.
# A failure on the ArkOS path ends with the internal folder only: automount never gets an ArkOS card. Never fails the unit.
# Tests: deploy/image/test-games-card.sh. The PU_ names are for those tests only.
ARGS=("$@")
MOUNTS=${PU_MOUNTS:-/proc/mounts}
PARTITIONS=${PU_PARTITIONS:-/proc/partitions}
SYSBLOCK=${PU_SYSBLOCK:-/sys/class/block}
OS_RELEASE=${PU_OS_RELEASE:-/etc/os-release}
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

automount() {
  local script
  log "-> ROCKNIX's automount ($1)"
  rm -f "$MARKER"
  if [ "$only_sd_usb" = 1 ]; then
    # Keep ROCKNIX's games and merged-storage behavior. Guard its common mount entry point and clear an unsafe
    # explicitly configured device so the normal scan can still find a card. If the upstream entry points change,
    # use the SD card's STORAGE folder instead of running an unguarded delegate.
    if script=$(awk '
      /^GAMES_DEVICE=\$\(get_setting system[.]gamesdevice\)$/ {
        print
        print "if [ -n \"$GAMES_DEVICE\" ] && ! pocket_external_games_device \"$GAMES_DEVICE\"; then GAMES_DEVICE=; fi"
        setting++
        next
      }
      /^function mount_games\(\) \{$/ {
        print
        print "  pocket_external_games_device \"$1\" || return 0"
        mount++
        next
      }
      { print }
      END { if (setting != 1 || mount != 1) exit 1 }
    ' "$AUTOMOUNT"); then
      export SYSBLOCK
      export -f pocket_external_games_device root_of
      exec /bin/bash -c "$script" "$AUTOMOUNT" "${ARGS[@]}"
    fi
    log "ROCKNIX automount could not be guarded: games from STORAGE only"
    umount -f "$EXT" >/dev/null 2>&1
    umount -f "$ROMS" >/dev/null 2>&1
    games_from_storage
    exit 0
  fi
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

# Require two system folders at the root. ROCKNIX may already have created empty folders under roms/.
arkos_card() {
  local p n roms contents count=0
  for p in "$1"/*; do
    n=${p##*/}
    case $KNOWN in *" ${n,,} "*) [ -d "$p" ] && count=$((count + 1)) ;; esac
  done
  [ "$count" -ge 2 ] || return 1
  roms=$(entry "$1" roms) || return 0
  [ -d "$1/$roms" ] && [ ! -L "$1/$roms" ] || return 1
  contents=$(find "$1/$roms" ! -type d -print -quit 2>/dev/null) || return 1
  [ -z "$contents" ]
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

pocket_external_games_device() {
  local device root node
  device=$(readlink -f -- "$1") || return 1
  case $device in /dev/*) ;; *) return 1 ;; esac
  root=$(root_of "$device")
  case $root in
    mmcblk*)
      [ "$(cat "$SYSBLOCK/$root/device/type" 2>/dev/null)" = SD ]
      ;;
    *)
      # UFS and USB mass storage both appear as SCSI disks. Only the USB disk has a USB bus ancestor.
      node=$(readlink -f -- "$SYSBLOCK/$root") || return 1
      [ -d "$node" ] || return 1
      while [ -n "$node" ]; do
        case $(readlink "$node/subsystem" 2>/dev/null) in */bus/usb) return 0 ;; esac
        node=${node%/*}
      done
      return 1
      ;;
  esac
}

games_from_storage() {
  local selected=${1:-} last
  mkdir -p "$INT/roms" 2>/dev/null
  mount --bind "$INT/roms" "$ROMS" >/dev/null 2>&1 || log "could not bind $INT/roms on $ROMS"
  # Make the system folders on STORAGE, where the last mount on /storage/roms put them.
  last=$(grep " $ROMS " "$MOUNTS" 2>/dev/null | tail -n 1 | cut -d" " -f1)
  if [ -e "$DIRS_CONF" ] && [ -n "$last" ] && [ "$last" != "$selected" ]; then
    systemd-tmpfiles --create "$DIRS_CONF" >/dev/null 2>&1
  fi
  [ -z "$(get_setting system.merged.device)" ] && set_setting system.merged.device internal
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

only_sd_usb=0
if grep -Eq '^HW_DEVICE="?SM(8250|8550|4450)"?$' "$OS_RELEASE" 2>/dev/null; then only_sd_usb=1; fi

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

# The partitions automount would look at, in its order (all mmcblk, then sd, then nvme, each sort -r).
table=$(blkid 2>/dev/null)
found=
mount_args=()
for pat in 'mmcblk[0-9]' 'sd[a-z]' 'nvme[0-9]n[0-9]'; do
  [ -n "$found" ] && break
  while read -r part type; do
    [ -n "$part" ] || continue
    case $type in *fat* | ntfs | ext4) ;; *) continue ;; esac
    if [ "$only_sd_usb" = 1 ] && ! pocket_external_games_device "$part"; then continue; fi
    root=$(root_of "$part")
    case $own in *" $root "*) continue ;; esac
    size=$(awk -v r="$root" '$4 == r {print $3}' "$PARTITIONS" 2>/dev/null)
    [ "${size:-0}" -gt 8388608 ] 2>/dev/null || { log "$part: device under 8 GiB"; continue; }
    [ -L "$SYSBLOCK/${root}boot0" ] && { log "$part: an Android boot device"; continue; }
    grep -q "^$part " "$MOUNTS" 2>/dev/null && { log "$part: already mounted"; continue; }
    mount_args=()
    case $type in
      ntfs) mount_args=(-t ntfs3) ;;
      # A read-only ext4 mount can replay the journal unless noload is set. Disable symlink traversal on the card.
      ext4) mount_args=(-t ext4 -o ro,noload,nosymfollow,nodev,nosuid,noexec) ;;
    esac
    mkdir -p "$PROBE" 2>/dev/null
    if "$BB" mount -o ro "${mount_args[@]}" "$part" "$PROBE" >/dev/null 2>&1; then
      if arkos_card "$PROBE"; then
        found=$part
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
  # The scan stops at the matching partition and leaves its mount options in mount_args.
  mkdir -p "$EXT" 2>/dev/null
  if "$BB" mount "${mount_args[@]}" "$found" "$EXT" >/dev/null 2>&1; then
    card=1
    log "$found on $EXT, as it is"
  else
    log "$found could not be mounted on $EXT: games from STORAGE only, the card untouched"
  fi
fi

games_from_storage "$found"

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
