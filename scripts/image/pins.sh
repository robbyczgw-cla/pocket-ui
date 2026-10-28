# Pinned inputs of the SD card image (scripts/make-image.sh, scripts/check-image.sh). Sourced, not run.
# Every value was checked by hand on 2026-10-22, see docs/SD-IMAGE.md.

# ROCKNIX release the image is based on, and the exact source tree it was built from (GitHub tag -> commit).
ROCKNIX_TAG=20261001
ROCKNIX_COMMIT=c445081a59518f37d9776e5412dd7b14910696f7
ROCKNIX_IMG=ROCKNIX-SM8250.aarch64-20261001.img
ROCKNIX_URL=https://github.com/ROCKNIX/distribution/releases/download/20261001/ROCKNIX-SM8250.aarch64-20261001.img.gz
# sha256 of the .img.gz, as published next to it (ROCKNIX-SM8250.aarch64-20261001.img.gz.sha256).
ROCKNIX_GZ_SHA256=86f79670d7f9385b65c144c283374c6f6d122915cc59e4bec5bf6a1f0b64ffb8
ROCKNIX_IMG_SHA256=1bb73b7d3bc94923e2db39a0f98951698753da26e0717a9c7175909c7aaae97c

# Files inside it. KERNEL_MD5 is also what /flash/KERNEL.md5 says and what the author's device runs.
KERNEL_MD5=e313a71b97a3112981bd7597a4016847
SYSTEM_MD5=6b3248528b2f19d11009a85eb90abf91

# What our boot files must be. Both are byte for byte the files on the author's device (/flash/boot/pocket,
# read 2026-10-22), which booted through Boot mode: Loader.
#   Image: the arm64 Image unpacked from KERNEL (scripts/make-sd-efi.sh)
#   dtb:   the "Retroid Pocket Mini V2" tree from KERNEL, ufshc@1d84000 and phy@1d87000 set to disabled with fdtput
IMAGE_MD5=1cc770836cf13e915b8b30f9c311d716
DTB_MD5=1ff6d2a9f7221a3aa3c7c683e1bd5080
# GRUB on the device. Rebuilt from Debian's current grub-efi-arm64-bin it may differ; the check only warns then.
GRUB_EFI_MD5_DEVICE=3e6c69bc4fc03408913aa9f6375b0796

# Partition table of the ROCKNIX image, kept as it is (sectors of 512 bytes).
P1_START=32768
P1_SIZE=4194304
P2_START=4227072
P2_SIZE=65536
