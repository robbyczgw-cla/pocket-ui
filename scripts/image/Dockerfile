# Tools for building and checking the SD card image without loop mounts: mtools for the FAT partition,
# debugfs/e2fsck for STORAGE, sfdisk for the partition table, fdtput for the device tree, squashfs-tools to
# repack SYSTEM without the files in pins.sh SYSTEM_REMOVE, zip/unzip for SYSTEM_ZIP_REMOVE, fsck.vfat.
# scripts/make-image.sh and scripts/check-image.sh build it as pocket-image-tools:<hash of this file>.
FROM debian:bookworm
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends \
      fdisk mtools e2fsprogs xz-utils device-tree-compiler python3 file ca-certificates squashfs-tools zip unzip dosfstools >/dev/null \
 && rm -rf /var/lib/apt/lists/*
