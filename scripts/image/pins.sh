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

# SM8250 EFI cards. Trees are selected by their exact model in the pinned KERNEL,
# rather than a filename on FAT (this base carries its trees appended to KERNEL).
# Columns: image key | exact model | original MD5 | UFS-disabled MD5.
SM8250_MODELS="
rp-mini-v2|Retroid Pocket Mini V2|4c49e68a9625eda8ea6a37283ca52740|1ff6d2a9f7221a3aa3c7c683e1bd5080
rp5|Retroid Pocket 5|f7502e1a1453aecee136bd3bf504860e|7655f05d1502a7aa705f783e12a13d78
flip2|Retroid Pocket Flip2|97154a65cf345121ed65f92cff39152b|13321ee941e568d0cd5cc1c68d044bf5
"
# Partition type of p1 on SM8250 cards. The Pocket 5 and Flip 2 mark it an EFI system partition: Retroid's newer
# "Boot mode: UEFI" is reported to start only from one (r/retroid, Pocket 6, 2026-08). Retroid's u-boot, OpenABL and
# ROCKNIX's ABL start from either type. The Mini V2 keeps ROCKNIX's basic-data type until a Mini V2 card is checked.
SM8250_ESP_TYPE=C12A7328-F81F-11D2-BA4B-00A0C93EC93B
SM8250_DATA_TYPE=EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
sm8250_p1_type() {
  case ${1:-} in rp5 | flip2) echo "$SM8250_ESP_TYPE" ;; *) echo "$SM8250_DATA_TYPE" ;; esac
}

sm8250_pins() {
  SM8250_MODEL=${1:-rp-mini-v2}
  SM8250_MODEL_NAME=$(echo "$SM8250_MODELS" | awk -F '|' -v m="$SM8250_MODEL" '$1 == m {print $2}')
  [ -n "$SM8250_MODEL_NAME" ] || { echo "unsupported SM8250 model: $SM8250_MODEL" >&2; return 1; }
  SM8250_ORIG_MD5=$(echo "$SM8250_MODELS" | awk -F '|' -v m="$SM8250_MODEL" '$1 == m {print $3}')
  DTB_MD5=$(echo "$SM8250_MODELS" | awk -F '|' -v m="$SM8250_MODEL" '$1 == m {print $4}')
  SM8250_DTB_FILE=$SM8250_MODEL-noufs.dtb
  [ "$SM8250_MODEL" != rp-mini-v2 ] || SM8250_DTB_FILE=miniv2-noufs.dtb
}

# Partition table of the ROCKNIX image, kept as it is (sectors of 512 bytes).
P1_START=32768
P1_SIZE=4194304
P2_START=4227072
P2_SIZE=65536
# ROCKNIX's STORAGE as it comes on every base image we use (SM8250, SM8550, H700 DDR4 and DDR3; read with dumpe2fs on 2026-11-08):
# ext4, 8192 blocks of 4 KiB with a 4 MiB journal (the smallest ext4 allows at this block size), 7027 of them free. build.sh checks
# it before it writes; scripts/image/storage-budget.sh adds up what goes in. The room we keep free on top of the programs is
# P2_MIN_HEADROOM_BLOCKS: about two releases of growth (0.3.0 to 0.3.1 took 67 blocks).
P2_FREE_BLOCKS=7027
P2_MIN_HEADROOM_BLOCKS=128

# ---------------------------------------------------------------------------------------------------------------
# H700 family (Allwinner; Anbernic RG35XX Plus, RG35XX SP, RG28XX): scripts/make-image-h700.sh. Same ROCKNIX tag and
# commit as above. ROCKNIX's own U-Boot boots it (extlinux.conf: LINUX /KERNEL, FDT /dtb.img), so there is no GRUB, no
# kernel unpacking and no device tree edit: the image is the official one plus our files, one image per model, and the
# models differ in exactly one file, the FAT root's dtb.img (a copy of device_trees/<model>.dtb).
# Checked by hand on 2026-10-30, see docs/SD-IMAGE.md "H700".
# ---------------------------------------------------------------------------------------------------------------
# The DDR4 image. The RG35XX Plus reads vdd-dram = 1.1 V = DDR4 (checked on the author's unit). A real RG35XX SP and a
# real RG28XX both booted this image with the DDR4 U-Boot (2026-11-01; the RG28XX also on 2026-10-31). A wrong RAM type
# does not boot (the U-Boot at 8 KiB of the card cannot start the RAM), the card is not harmed.
H700_IMG=ROCKNIX-H700.aarch64-20261001-DDR4.img
H700_URL=https://github.com/ROCKNIX/distribution/releases/download/20261001/ROCKNIX-H700.aarch64-20261001-DDR4.img.gz
H700_GZ_SHA256=cd070faf05754b77a3f8687e4add5e00fc7d261518d8fc95dea188f2ef9eb9d4
H700_IMG_SHA256=960be0de75e58e6322fc1d6282e07b49774bc98c0655263fe27f4223520ac8b8
H700_KERNEL_MD5=637d88be2fc50ceae70bfeb5ae3b98a1
H700_SYSTEM_MD5=4af2ee93d11d320ee6834a11601b82fa
# Everything before the first partition (16 MiB: the partition table, U-Boot 8 KiB in) of the DDR4 image.
H700_LOADER_MD5=5a1aced17aae61b5ec3afc8f2e796207
# The DDR3 image (models with "ddr3" as a fifth field in H700_MODELS). Its KERNEL and SYSTEM are byte for byte the DDR4 image's
# (same pins above), so the DDR3 card is the DDR4 card with the two things that differ taken from the DDR3 image: the first
# 16 MiB (U-Boot with the DDR3 RAM set-up) and extlinux/extlinux.conf (one more line, FDTOVERLAYS for the 1.2 V RAM regulator).
# Checked 2026-11-05 against ROCKNIX-H700.aarch64-20261001-DDR3.img.gz; the 0.2.0 download built the same way by hand.
H700_DDR3_IMG=ROCKNIX-H700.aarch64-20261001-DDR3.img
H700_DDR3_URL=https://github.com/ROCKNIX/distribution/releases/download/20261001/ROCKNIX-H700.aarch64-20261001-DDR3.img.gz
H700_DDR3_GZ_SHA256=222b1f8172ab5b4fef6b4b23c0482a474789cf83542a6d2e5f45dfbbf2ae1359
H700_DDR3_IMG_SHA256=6089cd9a5aacf7b36a6c438ca73be76be90193060c1254b030448435b5f8f5b8
H700_DDR3_LOADER_MD5=ca112618836167777a47d2fde901e258
H700_DDR3_EXTLINUX_MD5=56f3b5ff7f6fd26d8a3a915fa68a0570
# Models: "<image name> <dtb in device_trees/ without .dtb> <md5 of that dtb> <model string in the tree>".
# The panel variants (rg35xx-plus-r6, rg35xx-sp-v2: image names, the trees are ...-rev6-panel and ...-v2-panel) are newer screen revisions with their own tree, and
# their model string is the plain model's. The other models are untested expectations (same image, same U-Boot, their own
# tree); the md5s are those of ROCKNIX 20261001's device_trees/. The remaining panel trees (rg35xx-2024-rev6-panel,
# rg35xx-h-rev6-panel, rg40xx-h-v2-panel, rg40xx-v-v2-panel, rg34xx-sp-v2-panel) are not built yet. DDR4 for all: ROCKNIX's
# config.xml lists every tree under both DDR4 and DDR3 and names no model; its only DDR3 file is an overlay named for the
# RG35XX 2024. A line with "ddr3" as its fifth field is the same model on the DDR3 image (rg40xx-h-ddr3: the 0.2.0 download for
# RG40XX H units that stay black on the plain card); the other models are DDR4 only so far.
H700_MODELS="
rg35xx-plus sun50i-h700-anbernic-rg35xx-plus 7772960532233e284871ede57ba50426 Anbernic_RG35XX_Plus
rg35xx-sp sun50i-h700-anbernic-rg35xx-sp 954098e16c57c69095a5db14822e4720 Anbernic_RG35XX_SP
rg28xx sun50i-h700-anbernic-rg28xx a3618905df1a397b3a4192437ee90c77 Anbernic_RG28XX
rg35xx-h sun50i-h700-anbernic-rg35xx-h a0b9663ed1a14b36d9b65cd95f3b94ae Anbernic_RG35XX_H
rg35xx-pro sun50i-h700-anbernic-rg35xx-pro 0d801401c9cae0f96c0f8bebf128a6d8 Anbernic_RG35XX_Pro
rg35xx-2024 sun50i-h700-anbernic-rg35xx-2024 f088ca547b7a62f437ff4b3e21c88aae Anbernic_RG35XX_2024
rg40xx-h sun50i-h700-anbernic-rg40xx-h 7a4be234d2fa4e041844c18d94381296 Anbernic_RG40XX_H
rg40xx-v sun50i-h700-anbernic-rg40xx-v 2f226e8cc0831a72c12b2ce5def18b6e Anbernic_RG40XX_V
rg34xx sun50i-h700-anbernic-rg34xx 1f456bd28c466ce2c835cde613ad9289 Anbernic_RG34XX
rg34xx-sp sun50i-h700-anbernic-rg34xx-sp 082d459946583421a098792520f97c87 Anbernic_RG34XX-SP
rg-sp sun50i-h700-anbernic-rg-sp e7e56ee3dc6675ab5d967ce6766d4756 Anbernic_RG-SP
rgcubexx sun50i-h700-anbernic-rgcubexx 7487cada10ee1c8516d9391386d25522 Anbernic_RG_CubeXX
rg35xx-plus-r6 sun50i-h700-anbernic-rg35xx-plus-rev6-panel c2d994a36a7482f59068d83e64c80bfb Anbernic_RG35XX_Plus
rg35xx-sp-v2 sun50i-h700-anbernic-rg35xx-sp-v2-panel aa22183a17906b56545d0f0127118198 Anbernic_RG35XX_SP
rg40xx-h-ddr3 sun50i-h700-anbernic-rg40xx-h 7a4be234d2fa4e041844c18d94381296 Anbernic_RG40XX_H ddr3
"
# SYSTEM on an H700 card: ROCKNIX's, repacked without the files below (same reasons as the SM8250 list above; the
# AYN/Mangmi firmware and rocknix_abl do not exist in the H700 SYSTEM). Everything else is unchanged.
# usr/config/drastic (the DraStic Nintendo DS emulator by Exophase, closed source, ROCKNIX's drastic-sa) is NOT removed:
# it stays unchanged, as in ROCKNIX (decision 2026-10-31). The only basis is Exophase's 2017 forum statement (LICENSE.pdf
# in that folder, ROCKNIX's licenses/DRASTIC.pdf): tolerance, not a formal licence. It goes on request. See docs/SD-IMAGE.md.
H700_SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/share/idtech/doom.tar.gz
"
H700_SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked H700 SYSTEM (same mksquashfs settings); "unset" for one build prints it. Two independent strip
# runs (2026-10-31, side by side; the same md5 as on 2026-10-30) gave this md5; a build that does not reproduce it is refused.
H700_POCKET_SYSTEM_MD5=81d174ea53cca597f909ca109547b149

# ---------------------------------------------------------------------------------------------------------------
# SM8550 family (Snapdragon 8 Gen 2; AYN Odin 2 family, Retroid Pocket 6 and Nova): scripts/make-image-family.sh. Same
# ROCKNIX tag and commit. ROCKNIX's ABL, flashed into the handheld once from Android, starts KERNEL (an Android boot image
# holding every model's device tree). We never flash it, and it is not on our card. The Odin cards have nothing else, so
# they differ only in their name and notes; the Retroid cards also start without that ABL (SM8550_EFI_MODELS below).
# ---------------------------------------------------------------------------------------------------------------
SM8550_IMG=ROCKNIX-SM8550.aarch64-20261001.img
SM8550_URL=https://github.com/ROCKNIX/distribution/releases/download/20261001/ROCKNIX-SM8550.aarch64-20261001.img.gz
SM8550_GZ_SHA256=2314561e75bc8dbbc91a7dd5aac719d49ed98d07428d8d0967797a79c0b3b16f
SM8550_IMG_SHA256=33d4119f7d4822fdddb8ef0cfb41295bae62aaed323e663516dbabffa579a0fe
SM8550_KERNEL_MD5=50d85d0461b8ce820f494c50e317112c
SM8550_SYSTEM_MD5=2bcc1d80a64b8c1739d2aec9e24b54f9
# Models: as H700_MODELS; "-" and "-" for the tree and its md5, because KERNEL carries the trees (the check finds the
# model string in it). Only the Odin 2 Mini has a unit to test.
SM8550_MODELS="
odin2-mini - - AYN_Odin_2_Mini
odin2 - - AYN_Odin_2
odin2-portal - - AYN_Odin_2_Portal
rp6 - - Retroid_Pocket_6
rp6-top-dpad - - Retroid_Pocket_6_TOP-DPAD
nova - - Retroid_Pocket_Nova
"
# The Retroid cards also start without ROCKNIX's ABL, the way the SM8250 cards do: Retroid's own "UEFI" in the Volume Down
# menu (reported for a Pocket 6 on r/retroid, 2026-08: GRUB loads the tree, p1 is an EFI system partition, no flashing) or
# any other UEFI firmware starts GRUB from the card; OpenABL takes loader/entries/pocket-ui.conf. Both start the arm64 Image
# unpacked from KERNEL with the model's tree, internal UFS off (deploy/sd-efi-sm8550/). KERNEL stays as it is, and ROCKNIX's
# ABL still starts it: its SM8550 build looks only for \KERNEL, \kernel.img and \boot\reglinux.update (read from
# abl_signed-SM8550.elf, 2026-11-12). One card per model, and none of this on the Odin cards: OpenABL reads loader/entries
# and EFI/BOOT before KERNEL, so a shared card would start an Odin with a Retroid tree, and nothing before the kernel can
# tell a Pocket 6 from its TOP-DPAD layout (the only difference in the tree: the stick axes, inverted).
# Columns: image key | exact model in KERNEL | original MD5 | UFS-disabled MD5 (SM8550_UFS_NODES set to disabled).
SM8550_EFI_MODELS="
rp6|Retroid Pocket 6|29bf6151db49a1043aeee937c9a4685c|afba667a53ef4c3115bc5effa689dd24
rp6-top-dpad|Retroid Pocket 6 TOP-DPAD|db9a865e5a0c7ae1c7b9d1cd732493a0|d84d7f3b8abbd985acd78804380a3668
nova|Retroid Pocket Nova|95bdad48aa6bb6fd7b9e57d086ecd936|21b8fd78dbfcd6ed3065fd080ec77adf
"
SM8550_UFS_NODES="/soc@0/ufshc@1d84000 /soc@0/phy@1d80000"
# md5 of the arm64 Image unpacked from SM8550 KERNEL (the gzip member before the appended trees).
SM8550_IMAGE_MD5=02d4725f0142622e8f1b40079a815c59
# sm8550_efi_pins MODEL: the model's EFI pins (EFI_MODEL_NAME, EFI_ORIG_MD5, EFI_DTB_MD5, EFI_DTB_FILE); fails for an Odin.
sm8550_efi_pins() {
  pu_line=$(echo "$SM8550_EFI_MODELS" | awk -F '|' -v m="${1:-}" 'NF && $1 == m')
  [ -n "$pu_line" ] || return 1
  EFI_MODEL_NAME=$(echo "$pu_line" | cut -d'|' -f2)
  EFI_ORIG_MD5=$(echo "$pu_line" | cut -d'|' -f3)
  EFI_DTB_MD5=$(echo "$pu_line" | cut -d'|' -f4)
  EFI_DTB_FILE=$1-noufs.dtb
  unset pu_line
}
# is_efi_card FAMILY MODEL: the card carries GRUB, the BLS entry and an EFI system partition.
is_efi_card() {
  [ "${1:-}" = SM8550 ] && echo "$SM8550_EFI_MODELS" | awk -F '|' -v m="${2:-}" 'NF && $1 == m { f = 1 } END { exit !f }'
}
# SYSTEM on an SM8550 card, as for the SM8250 list above. The firmware of devices we don't list goes (AYANEO Pocket
# family, AYN Thor; ROCKNIX's extra-firmware, which states no licence). The rest of extra-firmware STAYS because the listed
# devices need it (sound and battery run on the ADSP): an open licence question, deploy/image/firmware-licences-sm8550/README.txt.
# DraStic stays unchanged as on the H700. usr/share/bootloader/rocknix_abl goes: without it ROCKNIX's update.sh cannot flash.
SM8550_SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/lib/kernel-overlays/base/lib/firmware/qcom/sm8550/ayaneo
usr/lib/kernel-overlays/base/lib/firmware/qcom/sm8550/ayn/thor
usr/share/bootloader/rocknix_abl
usr/share/idtech/doom.tar.gz
"
SM8550_SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked SM8550 SYSTEM; two strip runs side by side (2026-10-31) gave this md5.
SM8550_POCKET_SYSTEM_MD5=ca26139c1ec4416b42bb3ea80a014e99

# SM4450 Classic Rev Q beta. Official 20261101 tree already disables internal eMMC;
# requires the existing ROCKNIX ABL. Other Classic revisions/six-button models unsupported.
SM4450_TAG=20261101
SM4450_COMMIT=cefc14339a4ae18c02bb69888f27835e50664b60
SM4450_LICENSE_FILES=75
SM4450_IMG=ROCKNIX-SM4450.aarch64-20261101.img
SM4450_URL=https://github.com/ROCKNIX/distribution/releases/download/20261101/ROCKNIX-SM4450.aarch64-20261101.img.gz
SM4450_GZ_SHA256=cb8924c5fc82679a32949f632c9cda3264863f61c450cf5ae985bec379add93d
SM4450_IMG_SHA256=5bfb01b43a84c539d9f77db3c037083a792e4a17a7f37ef1418c78316ba52dbe
SM4450_KERNEL_MD5=f0db233941e22d36e3c63d0bea1bb469
SM4450_SYSTEM_MD5=e062fd2c8b1d889dac71fc86fe4b1bbe
SM4450_BLS_MD5=21bb31a6c66fb28a665614247bd26a2f
SM4450_MODELS="
rp-classic sm4450-retroid-pocket-classic-revq 62352309b1e1261984e91e551a6ab6f9 Retroid_Pocket_Classic_Rev_Q
"
SM4450_SYSTEM_REMOVE="$H700_SYSTEM_REMOVE
usr/share/bootloader/openabl
usr/lib/kernel-overlays/base/lib/firmware/qcom/sm4450/RG55G1
"
SM4450_SYSTEM_ZIP_REMOVE="$SYSTEM_ZIP_REMOVE"
# First strip prints its MD5; must be pinned before handing out the image.
SM4450_POCKET_SYSTEM_MD5=7ad75d773235362171161a45bac4640a
SM4450_FW_SHARED="LICENCE.rtlwifi_firmware.txt LICENSE.QualcommAtheros_ath10k LICENSE.qcom qca-NOTICE.txt qcom-NOTICE.txt wireless-regdb-LICENSE"
SM4450_GAMES_DRIVE_MODELS=""

# The RK3566 family (Anbernic RG DS Plus) has its pins in scripts/image/pins-rk3566.sh, a file the public export leaves
# out (scripts/public-exclude.txt): the RG DS Plus image is a private test image. Looked for under $ROOT, /repo (the
# build container) and the current folder; without it family_pins RK3566 gives empty values.
for pu_rk in "${ROOT:-/nonexistent}/scripts/image/pins-rk3566.sh" /repo/scripts/image/pins-rk3566.sh scripts/image/pins-rk3566.sh; do
  if [ -f "$pu_rk" ]; then . "$pu_rk"; break; fi
done
unset pu_rk

# The firmware licence texts a family shares with deploy/image/firmware-licences/.
H700_FW_SHARED="LICENCE.rtlwifi_firmware.txt wireless-regdb-LICENSE"
SM8550_FW_SHARED="LICENCE.rtlwifi_firmware.txt LICENSE.QualcommAtheros_ath10k LICENSE.qcom qca-NOTICE.txt qcom-NOTICE.txt wireless-regdb-LICENSE"

# A family's pins under one set of names, for the scripts all such families share.
# Cards that make a GAMES drive at their first start (docs/SD-IMAGE.md, "The GAMES drive"), by model. ROCKNIX's layout, with
# STORAGE over the whole card, is the standard on every image; two partitions for a player to think about are only worth
# it on a handheld with no Wi-Fi chip, which has no file sharing (RG28XX and RG35XX 2024: ROCKNIX's tree has no Wi-Fi node for the 2024).
H700_GAMES_DRIVE_MODELS="rg28xx rg35xx-2024"
SM8550_GAMES_DRIVE_MODELS=""
# is_ddr3 FAMILY MODEL: the model's line in <FAMILY>_MODELS has "ddr3" as its fifth field (the DDR3 base image, H700 only).
is_ddr3() {
  eval "pu_models=\${${1}_MODELS:-}"
  echo "$pu_models" | awk -v m="$2" '$1 == m && $5 == "ddr3" { f = 1 } END { exit !f }'
}
# has_games_drive FAMILY MODEL
has_games_drive() {
  [ -n "$2" ] || return 1 # no model (the Mini V2 build): no GAMES drive; an empty list matched the empty name before
  eval "pu_list=\${${1}_GAMES_DRIVE_MODELS:-}"
  case " $pu_list " in *" $2 "*) return 0 ;; esac
  return 1
}

# The name a card gives its handheld on the network (system.hostname, Tailscale, Bluetooth): "pocket-" and the image's
# model name, so two handhelds differ (ROCKNIX's default is the family, the same on all of them). The Mini V2 build
# has no MODEL. Lowercase ASCII, letters, digits and dashes, at most 24 characters (scripts/image/test-h700-pins.sh).
# image_hostname MODEL
image_hostname() {
  case ${1:-} in rp-mini-v2 | "") echo pocket-mini-v2 ;; *) echo "pocket-$1" ;; esac
}

family_pins() {
  for v in IMG URL GZ_SHA256 IMG_SHA256 KERNEL_MD5 SYSTEM_MD5 MODELS SYSTEM_REMOVE SYSTEM_ZIP_REMOVE POCKET_SYSTEM_MD5 FW_SHARED; do
    eval "FAM_$v=\${${1}_$v:-}"
  done
  # A family on another ROCKNIX release (RK3566) names its own tag, commit and licence count; the others use the ones above.
  eval "ROCKNIX_TAG=\${${1}_TAG:-$ROCKNIX_BASE_TAG} ROCKNIX_COMMIT=\${${1}_COMMIT:-$ROCKNIX_BASE_COMMIT}"
  eval "ROCKNIX_LICENSE_FILES=\${${1}_LICENSE_FILES:-$ROCKNIX_BASE_LICENSE_FILES}"
}
ROCKNIX_BASE_TAG=$ROCKNIX_TAG
ROCKNIX_BASE_COMMIT=$ROCKNIX_COMMIT
ROCKNIX_BASE_LICENSE_FILES=$ROCKNIX_LICENSE_FILES
