# Tools for building and checking the SD card image without loop mounts: mtools for the FAT partition,
# debugfs/e2fsck for STORAGE, sfdisk for the partition table, fdtput for the device tree.
# scripts/make-image.sh and scripts/check-image.sh build it as pocket-image-tools:<hash of this file>.
FROM debian:bookworm
RUN apt-get update -qq \
 && apt-get install -y -qq --no-install-recommends \
      fdisk mtools e2fsprogs xz-utils device-tree-compiler python3 file ca-certificates >/dev/null \
 && rm -rf /var/lib/apt/lists/*
