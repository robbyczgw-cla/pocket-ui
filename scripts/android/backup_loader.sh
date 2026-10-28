#!/system/bin/sh
# Phase 4 of docs/INSTALL-DUAL-BOOT.md: back up loader_a/loader_b (and abl_a/abl_b) before the one flash.
# Run as root from Android (Handheld Settings > Advanced > Run Script as Root). Only reads partitions.
# Retroid's "Run Script as Root" sends every line to a system service on its own (seen in logcat,
# PServiceBridgeV2): no variables, loops, functions or `exec` carry over, so each line stands alone with
# literal paths, like ROCKNIX's backup_abl.sh. A line that fails leaves the next one unaffected.
# Afterwards the Mac pulls /sdcard/pocket-backup with adb and compares partitions.sha256 with images.sha256.
mkdir -p /sdcard/pocket-backup
dd if=/dev/block/by-name/loader_a of=/sdcard/pocket-backup/loader_a.img bs=1M
dd if=/dev/block/by-name/loader_b of=/sdcard/pocket-backup/loader_b.img bs=1M
dd if=/dev/block/by-name/abl_a of=/sdcard/pocket-backup/abl_a.img bs=1M
dd if=/dev/block/by-name/abl_b of=/sdcard/pocket-backup/abl_b.img bs=1M
sha256sum /dev/block/by-name/loader_a /dev/block/by-name/loader_b /dev/block/by-name/abl_a /dev/block/by-name/abl_b > /sdcard/pocket-backup/partitions.sha256
sha256sum /sdcard/pocket-backup/loader_a.img /sdcard/pocket-backup/loader_b.img /sdcard/pocket-backup/abl_a.img /sdcard/pocket-backup/abl_b.img > /sdcard/pocket-backup/images.sha256
date > /sdcard/pocket-backup/DONE
