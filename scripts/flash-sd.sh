#!/bin/bash
# Flash a ROCKNIX .img to a removable SD card on macOS, with guard rails.
# Usage: flash-sd.sh <diskN> <image.img> <expected-size-bytes>
set -euo pipefail

DISK="$1"; IMG="$2"; EXPECTED_BYTES="$3"
[[ "$DISK" =~ ^disk[0-9]+$ ]] || { echo "disk id must look like disk9"; exit 1; }
[ -f "$IMG" ] || { echo "image not found: $IMG"; exit 1; }

info="$(diskutil info -plist "/dev/$DISK")"
get() { plutil -extract "$1" raw - <<<"$info" 2>/dev/null || true; }

internal="$(get Internal)"; removable="$(get RemovableMediaOrExternalDevice)"
size="$(get TotalSize)"; whole="$(get WholeDisk)"
boot_disk="$(diskutil info -plist / | plutil -extract ParentWholeDisk raw -)"

echo "target=/dev/$DISK internal=$internal removable=$removable whole=$whole size=$size"
[ "$whole" = "true" ]            || { echo "ABORT: not a whole disk"; exit 1; }
[ "$internal" = "false" ]        || { echo "ABORT: disk is internal"; exit 1; }
[ "$removable" = "true" ]        || { echo "ABORT: disk is not removable/external"; exit 1; }
[ "$size" = "$EXPECTED_BYTES" ]  || { echo "ABORT: size $size != expected $EXPECTED_BYTES"; exit 1; }
[ "$DISK" != "$boot_disk" ]      || { echo "ABORT: target is the boot disk"; exit 1; }
[ -n "${DRY_RUN:-}" ] && { echo "DRY_RUN: all checks passed"; exit 0; }

diskutil unmountDisk "/dev/$DISK"
echo "Writing $IMG -> /dev/r$DISK"
sudo dd if="$IMG" of="/dev/r$DISK" bs=4m status=progress
sync
echo "Write finished."
