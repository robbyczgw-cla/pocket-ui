This is the RK3326 section of [SD-IMAGE.md](SD-IMAGE.md), the rest of the image's documentation.

# RK3326: Game Console R36S (beta card)

Status 2026-11-20: a beta from 0.6.0, `pocket-ui-<version>-r36s` (and the device-test twin `pocket-ui-<version>-r36s-devtest`
for our own device tests, never published). Same builder and check as the other families: `scripts/make-image-family.sh
--family RK3326`, `scripts/check-image-family.sh`. Pins in `scripts/image/pins-rk3326.sh`.

**Base.** ROCKNIX **20261101** (`RK3326_TAG`, the same tag and commit as the RK3566 family), image **"a"**
(`ROCKNIX-RK3326.aarch64-20261101-a.img.gz`): the legacy Hardkernel-style U-Boot, which ROCKNIX's config.xml names for the
genuine R36S. Image "b" (mainline U-Boot) is for the EE clones and is not used. The table is **MBR** (`PARTITION_TABLE="msdos"`)
with the H700's offsets and size, so `grow_image` and the checks take the MBR path (only p2's size field changes); the first
16 MiB (`RK3326_LOADER_MD5`) are otherwise unchanged. ROCKNIX's STORAGE is the usual 32 MiB (7027 free blocks), grown to 128 MiB.

**Boot path and the one change.** U-Boot runs `boot.scr` (ROCKNIX's `a_boot.ini`, `RK3326_BOOTSCR_MD5`, unchanged): it picks a
tree by an ADC value and has **no R36S branch** (a genuine R36S would land on the R33S tree, no sticks), then `sysboot` reads
`extlinux/extlinux.conf`: `LINUX /KERNEL`, `FDTDIR /`, a commented `# FDT /your-custom-devicetree.dtb`, `FDTOVERLAYS
/overlays/mipi-panel.dtbo`, `APPEND boot=${partition_boot} disk=${partition_storage} …`. The card turns that comment into
`  FDT /rk3326-gameconsole-r36s.dtb` (an FDT line wins over FDTDIR and the script's `fdtfile`), which is the line ROCKNIX's own
`extlinux/extlinux.conf.r36s` carries (ROCKNIX's file also drops the `##` hint line; ours keeps it, so exactly one line changes).
`build.sh assemble` refuses unless extlinux.conf is ROCKNIX's (`RK3326_EXTLINUX_MD5`), holds exactly one `RK3326_FDT_COMMENT`
and no FDT line, and exactly that line changed; the check puts it back and compares the md5 again, and compares the FDT line
with `extlinux.conf.r36s`. The tree (`rk3326-gameconsole-r36s.dtb` in the FAT root, model "Game Console R36S", pad
`r36s_Gamepad`) is checked by md5. U-Boot's extlinux parser breaks on a space after the .dtb name; the check matches the
whole line.

**Screens.** The R36S's own tree drives the original 2023 panel. Newer R36S need `overlays/mipi-panel.dtbo`; a missing file
there is fine (ROCKNIX's extlinux README), so the card ships **no** overlay. `r36s-panels/` on the FAT partition holds seven of
ROCKNIX's prebuilt overlays (github.com/stolen/rnix release `panel_overlays`, 2024-07-04; sha256 each in `RK3326_PANELS`,
downloaded once into the cache): `r36s-panel1`…`4`, `r36s_clone`, `rg351mp-v2`, `rg351v-v2`, plus a plain README
(`deploy/image/r36s-panels-README.txt`: copy one to `overlays/mipi-panel.dtbo`, delete it to go back). The release's `cb408` and
`x35s` target the RK3566's DSI (`/dsi@fe060000/panel@0`) and fail to apply to the R36S tree (fdtoverlay `FDT_ERR_NOTFOUND`), so
they stay off. The check applies every shipped overlay to the R36S tree with `fdtoverlay`. Origin: a ROCKNIX developer's
fork of ROCKNIX's repository; no licence of their own is stated for these files (not researched further).

**Picking the screen from a stock tree.** Two ways, both with ROCKNIX's own `importpanel.py` (package generic-dsi, from the
pinned SYSTEM, sha256 `RK3326_IMPORTPANEL_SHA256`), which turns the stock tree's `panel@0` (init sequence and timings) into an
overlay for ROCKNIX's generic-dsi driver and nothing else:
- On a computer: `scripts/image/r36s-overlay-from-stock.sh --system <SYSTEM> [--base rk3326-gameconsole-r36s.dtb] STOCK.dtb
  OUT.dtbo`. It takes the script out of the SYSTEM with unsquashfs and runs it with `python3 -I` and the pinned pyFDT wheel
  (PyPI `fdt` 0.3.3, Apache-2.0; the SYSTEM has it only as Python 3.14 bytecode), downloaded once to `~/.cache/pocket-ui/pyfdt/`.
  `--base` applies the result with fdtoverlay. `--tool rocknix_dtbo` runs ROCKNIX's other script instead (below).
- On the handheld: `deploy/image/r36s-stock-panel.sh`, seeded to `/storage/.config/autostart/pocket-ui-r36s-panel` on both
  cards. At every start: if `stock/*.dtb` exists on the FAT partition and `overlays/mipi-panel.dtbo` does not, it runs
  `importpanel.py -O dtbo`, writes the overlay, renames the tree `.used` and restarts once; a tree it cannot read becomes
  `.failed` (no restart, so no loop). It logs to `stock/panel-log.txt`. Deleting the overlay therefore goes back to the original
  screen for good. Tests: `deploy/image/test-r36s-stock-panel.sh` (run by `scripts/image/test-h700-pins.sh`).
- Without a computer (a card flashed from a phone, 19.11.2026): with no tree in `stock/` and none used or failed before, the
  same script looks at the other cards' small FAT partitions (`blkid` vfat, under 1 GiB, not the card it runs from, not already
  mounted), mounts each read-only and takes the one tree the stock card's `boot.ini` names and has (copied into `stock/`, then
  the steps above). Two or more named trees there: no guess, one line in `panel-log.txt` and `stock/tf2-looked` (delete it to
  look again). So the stock card in TF2 at the first start gives the screen. Not tried on an R36S yet.
Not ROCKNIX's EE-clone quirk (`002-generate_dtbo`, `rocknix_dtbo.py`): that one also rewrites pad, sticks and headphones for the
EE clone tree and reinstalls the boot loader. On genuine R36S stock trees it switches the pad to the "My Mini" multi-ADC layout
(it sees `adc-keys` disabled), which is wrong for the R36S tree's single-ADC pad.

Validation (2026-11-15, 43 kernel trees of the dArkOSRE-R36 repository, `files/BOOT/dtb/*/*/rk3326-r36s-linux.dtb`, git blob
hashes checked): `importpanel` made an overlay from 40; the three `clone/R36S-V20 2025-05-18` trees fail in ROCKNIX's script
(a display mode without a phandle). All 40 overlays (and those of `rocknix_dtbo`) apply to the R36S tree. Init sequences
identical to a prebuilt overlay: G80C/G80CA/G80D Panel 8, R36S-V12 Variant 1 Panel 8 and Variant 2 Panel 1, Y02, RG36S-Y07 →
`r36s_clone`; HL-R36H-V20 and R36S-V12 Panel 4 → `r36s-panel4`; R35S-V11 and R36S-V12 Panel 0 → `rg351v-v2` (the 2023 panel);
R36S-V30 2025-11-18 → `rg351mp-v2`. The timings often differ from the prebuilt file. R36S-V21/V22, R36XX-V21, HL-R36H-V21 share
99 % of `r36s-panel4`'s init; the Y3506 ("Soy Sauce") and GR36 trees match none. So for an unknown unit the stock tree is the
reliable way; the prebuilt list is the fallback. The full table is in the build's report (images-r36s-work/REPORT.md).

**SYSTEM without** (`RK3326_SYSTEM_REMOVE`, read from the unpacked 20261101 SYSTEM): the SKick tables, loose and in Amiberry's
`boot-data.zip`, and the DOOM shareware data, as on the H700. DraStic stays. No ABL. Repacked md5 `RK3326_POCKET_SYSTEM_MD5`.

**Device-test card** (`r36s-devtest`, pins fifth field `devtest`, `is_devtest`). The same card plus, in `.pocket-seed`:
`.cache/usbgadget/usbgadget.conf` = `USB_MODE=cdc` (ROCKNIX's `081-usbgadget` runs `usbgadget --start`, which sources that file
and starts the NCM gadget: the handheld is 169.254.170.2, its udhcpd hands the computer .1), `.ssh/authorized_keys` (700/600,
from `make-image-family.sh --authorized-keys FILE`: public keys only, refused for any other card, never in the repository),
`.config/pocket-ui/dev-ssh` (pocket-ui keeps sshd on, src/remote.rs) and `autostart/pocket-ui-devtest`
(`deploy/image/r36s-devtest.sh`: `ssh.enabled=1`, sshd now, the gadget again if it is off). The R36S has one OTG port:
while it is a gadget a USB Wi-Fi adapter cannot work, and ROCKNIX's usbgadget refuses to switch while a device is plugged in.
The normal card has none of this (the check fails on any of it). Reach it: `ssh root@169.254.170.2` from the computer whose key
is on the card.

**No Wi-Fi.** The genuine R36S has no Wi-Fi chip: no OTA updates, no phone upload, no file sharing without a USB adapter. The
notes point to a second card in TF2 (ROCKNIX uses it for games) instead.

**Firmware.** SYSTEM has 295 files; `deploy/image/firmware-licences-rk3326/README.txt` lists each with md5. 285 are the same
files (same bytes) as in the RK3566 list and take its terms; 10 are not traced (seven Realtek Bluetooth files, as on the
RK3566, and three ESP8089 Wi-Fi files of ROCKNIX's esp8089-firmware package). The R36S tree asks for no firmware.

**Source.** `scripts/image/rocknix-source.sh` knows no RK3326 family, so there is no source bundle; the offer points to
ROCKNIX's repository at the tag, as for the RK3566.

Still open for the beta: starts on more R36S units and screens, the Mali driver's terms, the 10 untraced firmware files.
