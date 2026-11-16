# Pinned inputs of the RK3566 image family (Anbernic RG DS Plus; the RG353, RG503, RGB30 and RK2023 card), sourced by scripts/image/pins.sh when this file exists.
# Public build inputs for the Experimental RK3566 card; the RG DS Plus remains a device-test target.

# ---------------------------------------------------------------------------------------------------------------
# RK3566 family (Rockchip; generic handheld card and the Anbernic RG DS Plus device-test target): scripts/make-image-family.sh.
# Experimental, without a real-device boot test: ROCKNIX 20261101, not 20261001 (the Plus's audio and input fixes are in
# 20261101; the other families stay on 20261001). ROCKNIX's U-Boot (before the first partition, on a GPT card with the
# same offsets as the Mini V2's) reads extlinux/extlinux.conf: LINUX /KERNEL, FDT /device_trees/<tree>. The official
# "Specific" image names the Powkiddy X55's tree there, so the card gets exactly one changed line: the model's tree.
# Open before any release: charging (ROCKNIX PR #3539, the tree asks the charger for 10 A) and the Mali driver's terms.
# ---------------------------------------------------------------------------------------------------------------
RK3566_TAG=20261101
RK3566_COMMIT=cefc14339a4ae18c02bb69888f27835e50664b60
# ROCKNIX's LICENSE.md plus its licenses/ folder at 20261101.
RK3566_LICENSE_FILES=75
RK3566_IMG=ROCKNIX-RK3566.aarch64-20261101-Specific.img
RK3566_URL=https://github.com/ROCKNIX/distribution/releases/download/20261101/ROCKNIX-RK3566.aarch64-20261101-Specific.img.gz
RK3566_GZ_SHA256=0e0b73a0455b3ef7116af661ce785687fbec4f2f6bb1b99f785a2182f6976931
RK3566_IMG_SHA256=6fa096cff0e58d379baf352b293202544ec1145f6844ac926b508bef4ae88408
RK3566_KERNEL_MD5=bc2da0ff3c74936032ef7c14b1491e2d
RK3566_SYSTEM_MD5=a55bf4cf58cdbf23a61716f4daef1c92
# The first 16 MiB of the card (GPT, ROCKNIX's idbloader and U-Boot), unchanged; and ROCKNIX's extlinux.conf, which
# names the X55's tree (RK3566_DEFAULT_FDT) and is ours with that one line put back.
RK3566_LOADER_MD5=1c567e95c9e0d47dc7385fd180e60150
RK3566_EXTLINUX_MD5=9e6f0f89f39fa42cde4683d13b34c165
RK3566_DEFAULT_FDT=/device_trees/rk3566-powkiddy-x55.dtb
# ROCKNIX's "Generic" image of the same release (rocknix.org's RG353, RG503, RGB30 and RK2023 pages point to "RK3566").
# Same KERNEL, SYSTEM, device_trees/ and overlays/ as the Specific image (read 2026-11-15); it differs in the first 16 MiB (mainline
# U-Boot anbernic-rgxx3-rk3566, which tells the RG353M/P/V/VS/PS, RG503, RGB30, RK2023, RGB10MAX3 and RG ARC-D/S apart by an ADC
# value and sets fdtfile itself; doc/board/anbernic/rgxx3.rst in U-Boot) and in extlinux.conf (FDTDIR /device_trees, no FDT line).
# A "generic" model (fifth field) is the Specific card with those two parts of the Generic image, as a DDR3 model on the H700.
RK3566_GENERIC_IMG=ROCKNIX-RK3566.aarch64-20261101-Generic.img
RK3566_GENERIC_URL=https://github.com/ROCKNIX/distribution/releases/download/20261101/ROCKNIX-RK3566.aarch64-20261101-Generic.img.gz
RK3566_GENERIC_GZ_SHA256=b573c9656d85bca5fa1aba038aac6cfabd92345d28a55cd74725cc98c618c775
RK3566_GENERIC_IMG_SHA256=f1ef6f67d3e07f7983c52f984e18cd6d3ac49ba98b66c748db4cf3c0b21df1df
RK3566_GENERIC_LOADER_MD5=fce6810f6daf6165383e84bbb73bfe44
RK3566_GENERIC_EXTLINUX_MD5=8fd8e83bb4c9d9e15caa61be6d2f033c
# The trees the Generic card's U-Boot starts on the handhelds pocket-ui has a profile for (src/device_model.rs RK3566_BOARDS;
# the RG353M boots the RG353P's): tree, md5 and `model` (spaces as _), checked on the finished card.
RK3566_GENERIC_TREES="
rk3566-anbernic-rg353p ca3db29061115e0c26861e49950b0b5d Anbernic_RG353P
rk3566-anbernic-rg353ps 1cf469b43ce6872ca1170b788b31f6f4 Anbernic_RG353PS
rk3566-anbernic-rg353v 987349a6415edb25afa762950f1e3b88 Anbernic_RG353V
rk3566-anbernic-rg353vs 8485d56a7a072e7527e8c74e9814321b Anbernic_RG353VS
rk3566-anbernic-rg503 155338c492b27372fca0efd52b74e3f1 Anbernic_RG503
rk3566-powkiddy-rgb20sx 2256ad08de536f22167b19a2ae979767 Powkiddy_RGB20SX
rk3566-powkiddy-rgb30 abe0857e8fbcb791252b173e7c261eeb Powkiddy_RGB30
rk3566-powkiddy-rk2023 f367f7c8d28be145f94506f56be0a1d4 Powkiddy_RK2023
"
# Models: as H700_MODELS; the tree is the one extlinux.conf names (not copied to dtb.img). Not the plain RG DS's tree.
# rk3566: one card for every handheld the Generic U-Boot detects (no tree to name: "-" "-"), as ROCKNIX ships one image for
# them. The RG353M/P/V (and RG ARC-D) start it only once Android is wiped from their eMMC (ROCKNIX's RG353 page): the card
# itself never writes there (ROCKCHIP-NATIVE.md, section 1).
RK3566_MODELS="
rg-ds-plus rk3568-anbernic-rg-ds-plus a83ea2c367aacfa51cf26290b0d38e01 Anbernic_RG_DS_Plus
rk3566 - - RK3566_handhelds generic
"
# SYSTEM on an RK3566 card: as for the H700 (read with check-release-tree.py on the unpacked 20261101 SYSTEM, 2026-11-05: the
# same SKick tables and DOOM data; the rest of the finds are libretro cores and benchmark models). DraStic stays as there.
RK3566_SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/share/idtech/doom.tar.gz
"
RK3566_SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked RK3566 SYSTEM; two strip runs (2026-11-05, one after the other on the build machine) gave this md5.
RK3566_POCKET_SYSTEM_MD5=1d6a69adf8e8ef7821947776d03a357c
RK3566_GAMES_DRIVE_MODELS=""


# The firmware licence texts this family shares with deploy/image/firmware-licences/.
RK3566_FW_SHARED="LICENCE.rtlwifi_firmware.txt wireless-regdb-LICENSE"
