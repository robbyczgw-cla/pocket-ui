Firmware inside SYSTEM (usr/lib/kernel-overlays/base/lib/firmware) and its terms: H700 images
=============================================================================================

ROCKNIX 20261001 puts these 15 files into SYSTEM for the Allwinner H700 devices. All are unchanged.
The Realtek Wi-Fi and Bluetooth files and the regulatory database are taken from upstream (md5 below, compared
on 2026-10-30); the panel files are ROCKNIX's own.

linux-firmware 20260309 (https://cdn.kernel.org/pub/linux/kernel/firmware/linux-firmware-20260309.tar.gz,
sha256 28fb1b57fa9531edbd001a2462068267f10622e613abd40f952344b3f1fc5a16, ROCKNIX's kernel-firmware package)

  md5                              file                              terms (WHENCE)
  10f34596581e022af3658731b4af853f rtl_bt/rtl8821cs_fw.bin           LICENCE.rtlwifi_firmware.txt
  783db7913e59ab81adb335684d5f83fb rtl_bt/rtl8821cs_config.bin       LICENCE.rtlwifi_firmware.txt
                                                                     (linux-firmware links it to rtl8761b_config.bin;
                                                                     the same bytes as upstream's file)
  2638a40042a27e98df2f7929c8eee760 rtw88/rtw8821c_fw.bin             LICENCE.rtlwifi_firmware.txt

wireless-regdb 2026.05.30 (https://www.kernel.org/pub/software/network/wireless-regdb/wireless-regdb-2026.05.30.tar.xz,
sha256 8a27bfc081bafed8c24dd70fab0d96f098e5a0bfcd08d3da672595f225ab8993)

  7b37a33bbe0f18f342fcd5a9f80bc3fe regulatory.db                     wireless-regdb-LICENSE (ISC-style)
  1ab34236cbd7434b889a28f5bc3ce530 regulatory.db.p7s                 wireless-regdb-LICENSE

Display panel files (ROCKNIX's own: projects/ROCKNIX/packages/linux-firmware/kernel-firmware/extra-firmware/panels/
in the distribution repository at the tag, so they are in rocknix-20261001-distribution.tar.xz). Each is 387 to 557
bytes: the start-up command list of one Anbernic screen, written with the "panel firmware generator" that goes with
the kernel's generic MIPI panel driver (ROCKNIX patch 0110, by Hironori Kikuchi). The repository states no separate
licence for them; ROCKNIX's licence for files it wrote is GNU GPL version 2 (pocket-ui-info/rocknix-licenses/). We
pass them on as ROCKNIX ships them and could not find where the command values were first read from (the screens'
makers publish none).

  e048d184d8b9ce3a98d5e70f4409060e panels/anbernic,rg28xx-panel.panel
  6165500f107ce78cc6f3dd1dc87d176a panels/anbernic,rg34xx-panel.panel
  d7ddf8c7a90ba5df677706f4ee7834e1 panels/anbernic,rg34xx-sp-panel.panel
  bf8addd4bf8dbe587db0986bb6075919 panels/anbernic,rg34xx-sp-v2-panel.panel
  65d943a6a306614b89aa9e185ede82b4 panels/anbernic,rg35xx-plus-panel.panel
  adbec56f6968ec9b29266778efe6e960 panels/anbernic,rg35xx-plus-rev6-panel.panel
  3215b08ccf278d0a90ffe84b30ba9bc6 panels/anbernic,rg35xx-sp-v2-panel.panel
  7b1d5601f55435294f52f758888928fb panels/anbernic,rg40xx-panel.panel
  c9dd2d67b0d6adea08d49a92cf2ec320 panels/anbernic,rg40xx-v2-panel.panel
  f73b4ab6f4656992f8aa772e980f8301 panels/anbernic,rgcubexx-panel.panel

The Realtek files are "Redistributable" in WHENCE; LICENCE.rtlwifi_firmware.txt (this folder) allows passing
them on in binary form with the licence text included.

Not firmware but binary: usr/share/bootloader/H700_DDR3_u-boot-sunxi-with-spl.bin, H700_DDR4_u-boot-sunxi-with-spl.bin
and bl31.bin are built by ROCKNIX from U-Boot (GNU GPL version 2 or later), Arm Trusted Firmware (BSD 3-clause) and
ROCKNIX's h700-suspend-stub; their source is in the source archives next to the image (packages u-boot-DDR3,
u-boot-DDR4, atf, h700-suspend-stub in packages.txt). The DDR4 file is also what sits on the card at 8 KiB.
