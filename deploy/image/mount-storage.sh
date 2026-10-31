# pocket-ui SD image, every start. On the FAT partition as /flash/mount-storage.sh (scripts/make-image.sh).
#
# ROCKNIX's initramfs sources this file instead of mounting /storage itself (init, mount_storage(): "if [ -f
# /flash/mount-storage.sh ]"), every boot, with $disk from the kernel command line (disk=LABEL=STORAGE), before
# check_update. It mounts /storage exactly as ROCKNIX would, then:
#
# 1. First start only: once STORAGE has been grown, it moves the files from /storage/.pocket-seed into place.
#    At the first boot ROCKNIX grows STORAGE to the size of the card (/usr/lib/rocknix/fs-resize, started because
#    /storage/.please_resize_me exists), but refuses and leaves it at 32 MB if /storage/.config or /storage/.cache
#    already exists ("Resizing is not permitted"). The essway drop-in that starts pocket-ui has to be in
#    /storage/.config/system.d, so the image carries it in /storage/.pocket-seed until the second boot.
#
# 2. Every start: guards against system updates pocket-ui did not check. ROCKNIX's check_update installs any *.tar,
#    *.img, *.img.gz (or a loose KERNEL + SYSTEM pair) in /storage/.update: from EmulationStation's updater, from the
#    Samba share "update", or copied there by hand. On this card such an update would replace the kernel GRUB does not
#    boot and flash the bootloader Android needs (docs/SD-IMAGE.md). pocket-ui stages its own checked updates with an
#    approval next to them (src/update.rs stage_os: .pocket-ui-approved, "<sha256>  <file name>"). Anything else is
#    moved to /storage/.update-refused (never deleted, never over an earlier one) and the reason is logged there;
#    ROCKNIX then boots as usual. If a file cannot be moved at all, init's check_update is skipped for this start.
#    When in doubt (no approval, no sha256sum, a mismatch) the file is refused: the start goes on unchanged.
#
# Runs in the initramfs: BusyBox ash, no `set -e`, only the applets ROCKNIX's initramfs has (no date, no test: ash's
# builtins); mount_part and progress come from ROCKNIX's init.
# PU_STORAGE is for the tests only. Tests: deploy/image/test-mount-storage.sh (part of the gate)

pu_storage=${PU_STORAGE:-/storage}

pu_seed() {
  [ -e "$pu_storage/.please_resize_me" ] && return 0
  [ -d "$pu_storage/.pocket-seed" ] || return 0
  progress "pocket-ui: putting the first-start files in place"
  cp -a "$pu_storage/.pocket-seed/." "$pu_storage/" && rm -rf "$pu_storage/.pocket-seed"
  sync
}

# The first of NAME, NAME.1, NAME.2 ... that does not exist yet: a refused file never replaces an earlier one.
pu_free() {
  pu_x=$1
  pu_i=1
  while [ -e "$pu_x" ] || [ -L "$pu_x" ]; do pu_x=$1.$pu_i; pu_i=$((pu_i + 1)); done
  echo "$pu_x"
}

# Moves $1 (a path in .update) to .update-refused and writes why ($2) into its log. If that fails, the file is renamed
# in place (NAME.refused, never over an earlier one) so check_update's patterns no longer match it. If even that fails
# (a read-only or damaged STORAGE), ROCKNIX's check_update is switched off for this start, below.
pu_refuse() {
  pu_r=$pu_storage/.update-refused
  pu_n=$(basename "$1")
  mkdir -p "$pu_r" 2>/dev/null
  pu_to=$(pu_free "$pu_r/$pu_n")
  mv "$1" "$pu_to" 2>/dev/null || { pu_to=$(pu_free "$1.refused"); mv "$1" "$pu_to" 2>/dev/null; }
  if [ -e "$1" ]; then
    pu_stuck=1
    { echo "could not move $pu_n away: $2" >>"$pu_r/refused.log"; } 2>/dev/null
    return 0
  fi
  { echo "$pu_n -> $(basename "$pu_to"): $2" >>"$pu_r/refused.log"; } 2>/dev/null
  progress "pocket-ui: system update $pu_n not installed ($2)"
}

pu_guard() {
  pu_stuck=0
  pu_u=$pu_storage/.update
  [ -d "$pu_u" ] || return 0
  pu_ok=$pu_u/.pocket-ui-approved
  for pu_f in "$pu_u"/*.tar "$pu_u"/*.img "$pu_u"/*.img.gz; do
    [ -f "$pu_f" ] || continue
    case $pu_f in
      *.tar) ;;
      *) pu_refuse "$pu_f" "pocket-ui installs no disk images"; continue ;;
    esac
    if [ ! -f "$pu_ok" ]; then
      pu_refuse "$pu_f" "not staged by pocket-ui (an update from EmulationStation, the network share or a copy)"
      continue
    fi
    pu_want=
    pu_n=$(basename "$pu_f")
    while read -r pu_sum pu_name || [ -n "$pu_sum" ]; do
      [ "$pu_name" = "$pu_n" ] && { pu_want=$pu_sum; break; }
    done <"$pu_ok"
    if [ -z "$pu_want" ]; then
      pu_refuse "$pu_f" "not the update pocket-ui staged"
      continue
    fi
    progress "pocket-ui: checking the system update"
    pu_have=$(sha256sum "$pu_f" 2>/dev/null | cut -d' ' -f1)
    [ -n "$pu_have" ] && [ "$pu_have" = "$pu_want" ] || pu_refuse "$pu_f" "its checksum is not the one pocket-ui checked"
  done
  # A loose KERNEL + SYSTEM pair is never staged by pocket-ui (its updates are tars).
  for pu_f in KERNEL SYSTEM KERNEL.md5 SYSTEM.md5; do
    [ -f "$pu_u/$pu_f" ] && pu_refuse "$pu_u/$pu_f" "pocket-ui installs no loose KERNEL or SYSTEM"
  done
  # A foreign update could not be moved: ROCKNIX's check_update would install it (or fail on the read-only STORAGE
  # and reboot). This file is sourced inside init's shell and init calls its boot steps by name after this one, so
  # this definition replaces init's check_update for this start only: the start goes on normally, the file stays.
  if [ "$pu_stuck" = 1 ]; then
    progress "pocket-ui: a system update pocket-ui did not check could not be moved aside; update check skipped"
    check_update() { :; }
  fi
  # The approval is spent once nothing it approved is left (ROCKNIX's cleanup removes it after an install).
  [ -f "$pu_ok" ] && ! ls "$pu_u"/*.tar >/dev/null 2>&1 && rm -f "$pu_ok"
  sync
  return 0
}

mount_part "$disk" "/storage" "rw,noatime"
pu_seed
pu_guard
true
