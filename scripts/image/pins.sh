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

# SYSTEM on our card: ROCKNIX's, repacked without the files below (scripts/image/build.sh strip-system,
# docs/SD-IMAGE.md "What changes on the card"). Everything else in it is unchanged, file for file.
#   - SKick's Kickstart relocation tables for Amiberry: SKick's licence allows passing them on only inside its
#     unchanged archive (aminet.net util/boot/skick346.lha, SKick.doc section 1), not as loose files.
#   - firmware for the AYN Thor and the Mangmi Pocket Max (ROCKNIX's extra-firmware, no licence stated): the
#     Mini V2's device tree never asks for it.
#   - ROCKNIX's ABL (usr/share/bootloader/rocknix_abl/: a signed bootloader binary and the scripts that flash it;
#     github.com/ROCKNIX/abl states no licence and has no source): this boot path never flashes an ABL, and the
#     same folder is already left off the FAT partition.
#   - the DOOM shareware IWAD (usr/share/idtech/doom.tar.gz, doom.wad v1.9, md5 f0cefca4...), which ROCKNIX's
#     idtech-lr package downloads for its "Scan id Tech Games" tool: a game, and the shareware terms ask for the
#     complete, unchanged shareware package. The card promises no games; without it the tool adds no Doom entry.
SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/lib/kernel-overlays/base/lib/firmware/qcom/sm8250/ayn
usr/lib/kernel-overlays/base/lib/firmware/qcom/sm8250/mangmi
usr/share/bootloader/rocknix_abl
usr/share/idtech/doom.tar.gz
"
# Members taken out of a zip inside SYSTEM ("zip-path member-glob"): the same SKick tables ride along in Amiberry's
# WHDLoad boot archive (Devs/Kickstarts). The rest of that archive stays as it is (zip -d keeps the other entries).
SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked SYSTEM (mksquashfs from scripts/image/Dockerfile, zstd level 19, 1 MiB blocks as ROCKNIX's).
POCKET_SYSTEM_MD5=91cfd1d6d696cd907eb7136bce956d36
# ROCKNIX's LICENSE.md plus its licenses/ folder at the tag (pocket-ui-info/rocknix-licenses/ on the card).
ROCKNIX_LICENSE_FILES=76
# usr/config/nanoboyadvance/bios/gba_bios.bin in SYSTEM: the open-source replacement BIOS from
# https://github.com/Nebuleon/ReGBA/raw/master/bios/gba_bios.bin (Normmatt, VBA-M team; GPL-2.0-or-later), which
# ROCKNIX's nanoboyadvance-sa package downloads (PKG_OPEN_SOURCE_BIOS). Not Nintendo's BIOS.
REGBA_BIOS_MD5=1876f71b0d8c65eef3454547896ffb11

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
