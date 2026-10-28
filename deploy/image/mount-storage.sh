# pocket-ui SD image, first start. On the FAT partition as /flash/mount-storage.sh (scripts/make-image.sh).
#
# ROCKNIX's initramfs sources this file instead of mounting /storage itself (init, mount_storage(): "if [ -f
# /flash/mount-storage.sh ]"), every boot, with $disk from the kernel command line (disk=LABEL=STORAGE). It mounts
# /storage exactly as ROCKNIX would, and once STORAGE has been grown it moves the files from /storage/.pocket-seed
# into place and deletes itself. From then on ROCKNIX mounts /storage itself again.
#
# Why the detour: at the first boot ROCKNIX grows STORAGE to the size of the card (/usr/lib/rocknix/fs-resize,
# started because /storage/.please_resize_me exists), but refuses and leaves it at 32 MB if /storage/.config or
# /storage/.cache already exists ("Resizing is not permitted"). The essway drop-in that starts pocket-ui has to be
# in /storage/.config/system.d, so the image carries it in /storage/.pocket-seed until the second boot.
#
# Runs in the initramfs: BusyBox ash, no `set -e`, mount_part and progress come from ROCKNIX's init.
# Tests: deploy/image/test-hooks.sh

mount_part "$disk" "/storage" "rw,noatime"

if [ ! -e /storage/.please_resize_me ]; then
  if [ -d /storage/.pocket-seed ]; then
    progress "pocket-ui: putting the first-start files in place"
    cp -a /storage/.pocket-seed/. /storage/ && rm -rf /storage/.pocket-seed
    sync
  fi
  # Done (or nothing to do): ROCKNIX mounts /storage itself from the next boot on.
  if [ ! -d /storage/.pocket-seed ]; then
    mount -o remount,rw /flash && rm -f /flash/mount-storage.sh && sync
    mount -o remount,ro /flash
  fi
fi
