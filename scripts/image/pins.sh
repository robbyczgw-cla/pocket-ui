# Pinned inputs of the SD card image (scripts/make-image.sh, scripts/check-image.sh). Sourced, not run.
# Every value was checked by hand on 2026-10-22, see docs/SD-IMAGE.md.

# The name of the system on our cards: About shows "<name> <version> · built on ROCKNIX <tag> (<device>)", read from
# pocket-ui-info/pocket-ui-os.txt on the FAT partition. The only place it is set (the ROCKNIX team was asked about it).
POCKET_OS_NAME="pocket-ui OS"

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

# ---------------------------------------------------------------------------------------------------------------
# H700 family (Allwinner; Anbernic RG35XX Plus, RG35XX SP, RG28XX): scripts/make-image-h700.sh. Same ROCKNIX tag and
# commit as above. ROCKNIX's own U-Boot boots it (extlinux.conf: LINUX /KERNEL, FDT /dtb.img), so there is no GRUB, no
# kernel unpacking and no device tree edit: the image is the official one plus our files, one image per model, and the
# models differ in exactly one file, the FAT root's dtb.img (a copy of device_trees/<model>.dtb).
# Checked by hand on 2026-10-30, see docs/SD-IMAGE.md "H700".
# ---------------------------------------------------------------------------------------------------------------
# The DDR4 image. The RG35XX Plus reads vdd-dram = 1.1 V = DDR4 (checked on the author's unit). The RG35XX SP and the
# RG28XX are expected to be DDR4 too (ROCKNIX's pages say LPDDR4 for both) but that is NOT confirmed on the units: a
# wrong RAM type does not boot (the U-Boot at 8 KiB of the card cannot start the RAM), the card is not harmed.
H700_IMG=ROCKNIX-H700.aarch64-20261001-DDR4.img
H700_URL=https://github.com/ROCKNIX/distribution/releases/download/20261001/ROCKNIX-H700.aarch64-20261001-DDR4.img.gz
H700_GZ_SHA256=cd070faf05754b77a3f8687e4add5e00fc7d261518d8fc95dea188f2ef9eb9d4
H700_IMG_SHA256=960be0de75e58e6322fc1d6282e07b49774bc98c0655263fe27f4223520ac8b8
H700_KERNEL_MD5=637d88be2fc50ceae70bfeb5ae3b98a1
H700_SYSTEM_MD5=4af2ee93d11d320ee6834a11601b82fa
# Models: "<image name> <dtb in device_trees/ without .dtb> <md5 of that dtb> <model string in the tree>".
# The panel variants (rg35xx-plus-rev6-panel, rg35xx-sp-v2-panel) are newer screen revisions with their own tree:
# not built until a unit shows a broken picture with these.
H700_MODELS="
rg35xx-plus sun50i-h700-anbernic-rg35xx-plus 7772960532233e284871ede57ba50426 Anbernic_RG35XX_Plus
rg35xx-sp sun50i-h700-anbernic-rg35xx-sp 954098e16c57c69095a5db14822e4720 Anbernic_RG35XX_SP
rg28xx sun50i-h700-anbernic-rg28xx a3618905df1a397b3a4192437ee90c77 Anbernic_RG28XX
"
# SYSTEM on an H700 card: ROCKNIX's, repacked without the files below (same reasons as the SM8250 list above; the
# AYN/Mangmi firmware and rocknix_abl do not exist in the H700 SYSTEM). Everything else is unchanged.
#   - usr/config/drastic: the DraStic Nintendo DS emulator (closed-source binary by Exophase, ROCKNIX's drastic-sa) with
#     its cheat and game databases and a BSD-licensed replacement BIOS. The only permission ROCKNIX points to is a 2017
#     forum thread (LICENSE.pdf in that folder) in which Exophase says he asked people NOT to distribute the Raspberry Pi
#     beta and that free use there is "up to my discretion": no licence to pass it on. Gone with it: the two
#     drastic_bios_*.bin (the open replacement BIOS), so the card holds no BIOS files at all.
H700_SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/share/idtech/doom.tar.gz
usr/config/drastic
"
H700_SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked H700 SYSTEM (same mksquashfs settings); "unset" for one build prints it. Two independent strip
# runs (2026-10-31, side by side) gave this md5; a build that does not reproduce it is refused.
H700_POCKET_SYSTEM_MD5=d03073318997fedcd5f3593672680254
