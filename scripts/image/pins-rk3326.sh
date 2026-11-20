# Pinned inputs of the RK3326 image family (Game Console R36S), sourced by scripts/image/pins.sh when this file exists.
# The R36S card is a beta (0.6.0): first started on a real R36S on 20.11.2026.

# ---------------------------------------------------------------------------------------------------------------
# RK3326 family (Rockchip; for now only the genuine Game Console R36S, 640x480, 1 GB, no eMMC, no Wi-Fi chip):
# scripts/make-image-family.sh --family RK3326, docs/SD-IMAGE-RK3326.md. A beta card.
# ROCKNIX 20261101, image "a" (the legacy Hardkernel-style U-Boot; ROCKNIX's config.xml puts the genuine R36S there, "b" is for
# the EE clones). MBR (devices/RK3326/options PARTITION_TABLE="msdos") with the H700's offsets and size. U-Boot runs boot.scr,
# which picks a tree by an ADC value and has no R36S branch (a genuine R36S would get the R33S tree: no sticks), then reads
# extlinux/extlinux.conf: LINUX /KERNEL, FDTDIR /, FDTOVERLAYS /overlays/mipi-panel.dtbo. The card's one change there: the
# commented "# FDT" line becomes "FDT /rk3326-gameconsole-r36s.dtb" (what ROCKNIX's own extlinux.conf.r36s says).
# ---------------------------------------------------------------------------------------------------------------
RK3326_TAG=20261101
RK3326_COMMIT=cefc14339a4ae18c02bb69888f27835e50664b60
# ROCKNIX's LICENSE.md plus its licenses/ folder at 20261101 (the same tag as the RK3566 family).
RK3326_LICENSE_FILES=75
RK3326_IMG=ROCKNIX-RK3326.aarch64-20261101-a.img
RK3326_URL=https://github.com/ROCKNIX/distribution/releases/download/20261101/ROCKNIX-RK3326.aarch64-20261101-a.img.gz
RK3326_GZ_SHA256=4e96c2574e7856f9c35963cd8aba1c1ba40a87694b8676f42b8bb2e5efaa02ce
RK3326_IMG_SHA256=a8fdfadfbf5259c9600d4a00c0e9f1f128957cfe201b70c480fc900b4e09a6bd
RK3326_KERNEL_MD5=42f9162bf470a50815c01d11cdd4958e
RK3326_SYSTEM_MD5=193ef203ae7de08e1d3a09c8fc5d49dd
# The first 16 MiB of the card (MBR, ROCKNIX's legacy U-Boot), unchanged but for p2's size; boot.scr (ROCKNIX's a_boot.ini) unchanged;
# ROCKNIX's extlinux.conf with its commented FDT line (RK3326_FDT_COMMENT) made into RK3326_FDT_LINE, nothing else.
RK3326_LOADER_MD5=a1f3a0bc412cff733cd49c378635fd69
RK3326_BOOTSCR_MD5=e1bc1c92e3221fe96e243a78f107c41c
RK3326_EXTLINUX_MD5=21f4da2c9ba6926ed8ccbe48563ea15d
RK3326_FDT_COMMENT="  # FDT /your-custom-devicetree.dtb"
# Models: as H700_MODELS (the tree is in the FAT root, as ROCKNIX's FDTDIR /). A fifth field "devtest" is the same card plus the
# USB network preset, SSH on and the build machine's public key (make-image-family.sh --authorized-keys): agents reach the handheld
# over a USB-C cable. Never handed out.
RK3326_MODELS="
r36s rk3326-gameconsole-r36s 484c724f212ea80762bf25697376eee6 Game_Console_R36S
r36s-devtest rk3326-gameconsole-r36s 484c724f212ea80762bf25697376eee6 Game_Console_R36S devtest
"
# SYSTEM on an RK3326 card: as for the H700 (the same SKick tables, loose and in Amiberry's boot-data.zip, and the DOOM shareware data;
# read from the unpacked 20261101 SYSTEM, 2026-11-15). DraStic stays as there. No ABL.
RK3326_SYSTEM_REMOVE="
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33180.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick33192.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick34005.A500.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40063.A600.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A1200.RTB
usr/config/amiberry/whdboot/save-data/Kickstarts/kick40068.A4000.RTB
usr/share/idtech/doom.tar.gz
"
RK3326_SYSTEM_ZIP_REMOVE="
usr/config/amiberry/whdboot/boot-data.zip Devs/Kickstarts/*.RTB
"
# md5 of the repacked RK3326 SYSTEM (mksquashfs as for the other families); two strip runs on the build host (2026-11-15) gave it.
RK3326_POCKET_SYSTEM_MD5=0a40490df6dd083b64056f7b37a1c86e
RK3326_GAMES_DRIVE_MODELS=""
RK3326_FW_SHARED="LICENCE.rtlwifi_firmware.txt wireless-regdb-LICENSE"

# The screens of 2024 and later R36S need overlays/mipi-panel.dtbo (ROCKNIX's R35S/R36S page). The card carries the prebuilt ones
# ROCKNIX's page points to (github.com/stolen/rnix, release "panel_overlays", 2024-07-04, by a ROCKNIX developer; the repository is a
# fork of ROCKNIX's and states no licence of its own for these files) in r36s-panels/ on the FAT partition, none in overlays/.
# "<name> <sha256>": r36s-panels/<name>.dtbo on the card, downloaded from RK3326_PANELS_URL/mipi-panel.dtbo.<name>. The release has two
# more, cb408 and x35s: they target the RK3566's DSI (/dsi@fe060000/panel@0) and do not apply to the R36S tree (fdtoverlay
# FDT_ERR_NOTFOUND, 2026-11-15), so they stay off the card.
RK3326_PANELS_URL=https://github.com/stolen/rnix/releases/download/panel_overlays
RK3326_PANELS="
r36s-panel1 c2005751c98cd505c686d65d18308ad1abe105a9bd86c6beef87b35a22938054
r36s-panel2 8d1dddc717f7b3aceeadc063209bf4f235092a37268e9549b7a92a5d848aa767
r36s-panel3 364921615dffcb772e33c6cfbe5af0d7079dbc9a22ca71a3fab0ab96e03b31a2
r36s-panel4 846ee139c96f0c121116234c63025b97cf3f4f947147e6761185ab965f4b77f3
r36s_clone cb5f6fd860c7fbb82e2bc6b07c7e1db197388dbe9df74bfde5c28c3b3be9e743
rg351mp-v2 c155d96eaa2b8efeae0dfd8bab2dd33b72a8c0b4e69d765d6d48c9dc22aa520a
rg351v-v2 71e390a6c3ef682c391918d8b6c4e1f9009f62ac0de210b654960d33d7353a4a
"
# ROCKNIX's panel scripts in that SYSTEM (package generic-dsi; scripts/image/r36s-overlay-from-stock.sh runs them on a computer,
# deploy/image/r36s-stock-panel.sh on the handheld) and the pyFDT wheel they need on a computer (PyPI fdt 0.3.3, Apache-2.0).
RK3326_IMPORTPANEL_SHA256=f3265e790fd572500c2ded6a33b00f3b89ce3e084976c040167e357c28395c76
RK3326_ROCKNIX_DTBO_SHA256=322498090645015417b7088018bdf8b6f937afa46b86cde35892fdf62631c63e
RK3326_PYFDT_URL=https://files.pythonhosted.org/packages/c7/ff/fcbe672acd848caddb5877225446fdf0fc1bc0c6c437343e2bde8f7b0aa9/fdt-0.3.3-py3-none-any.whl
RK3326_PYFDT_SHA256=6b2fae2e8dfa38e9b0f9666aa001dd25be74e893d293a8d60001438f732e9e47
