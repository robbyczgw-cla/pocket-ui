# Test ROCKNIX without writing anything to the device (`fastboot boot`)

> **Status:** the first test, from 2026-10-21. It still works as a way to start the system from a computer, and
> `scripts/make-test-bootimg.py` is still used to build the device tree for the SD boot files. The permanent route is
> [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md).

The device boots ROCKNIX from **RAM only**. Nothing is written to its internal storage.
After a reboot or a long press of Power, the Mini V2 is exactly as before.

## What was built (on a computer)
`rocknix-miniv2-test.img` (a build from 2026-10-21) is made by
`scripts/make-test-bootimg.py` from the verified ROCKNIX 20261001 `KERNEL`:
- **Only the Mini V2 device tree.** The original contains 9, and all Retroid models share
  board-id `0x1001f`. The stock bootloader would pick "Flip2 Visionox" first.
- **Internal UFS disabled** in that device tree (`ufshc@1d84000`, `phy@1d87000` → `disabled`).
  Linux cannot see Android's storage at all. The SD controller stays enabled.
- Cmdline: `nodtbo` added, so the stock ABL does not apply Android overlays (as in Retroid's own u-boot
  guide). `quiet` removed, so boot messages are visible.

## Steps on the device
1. Charge to at least 50 %.
2. Power off completely. Insert the ROCKNIX microSD.
3. Connect the Mini V2 to the computer with a **USB-C data cable**.
4. Hold **Volume Down**, then press **Power**. Keep holding Volume Down until the
   fastboot/bootloader screen appears.
5. On the computer, run:
   - `fastboot devices` (is it connected?)
   - `fastboot getvar unlocked`, `getvar current-slot`, `getvar product` (read-only)
   - `fastboot boot rocknix-miniv2-test.img` (RAM only, **no** `flash`)
6. Expected result: kernel messages on screen, then the ROCKNIX first boot. It grows the
   `STORAGE` partition **on the SD card** and **reboots by itself**. That reboot lands in **normal
   Android**, because nothing was changed. For the real ROCKNIX start, repeat steps 4–5.

## If something goes wrong
- **Black screen or hang:** hold Power for about 15 s until the device turns off. Power on normally and
  Android starts.
- **`fastboot boot` is refused** (bootloader locked): stop. **Do not unlock.** Unlocking
  wipes all Android data. The ABL path is the alternative.
- **Wrong rotation or colours:** harmless for the test. Note what you see and reboot.

## Results
- **2026-10-21, run 1:** the stock ABL accepted `fastboot boot` (`product: kona`, `unlocked: yes`, slot b).
  The ROCKNIX initramfs ran. `fs-resize.log` shows resizepart + resize2fs on `/dev/mmcblk0p2`
  (SD only), and STORAGE grew from 33.6 MB to 60.4 GB. It then rebooted into Android as expected.
  The RTC was unset, so log time is 2024-06-25. Android added its media folders to the SD FAT partition
  (harmless).
- **2026-10-21, run 2:** full boot to EmulationStation. Kernel 7.2.0, boot 24 s (2.3 s kernel + 21.7 s userspace).
  No `sd*` devices exist, so internal UFS is invisible as intended. `mem_sleep: [s2idle]` only. 
  Harmless log noise: "Initramfs unpacking failed" (5-byte dummy ramdisk; initramfs is built in), early "DSI PLL lock failed" (recovers),
  missing iris VPU firmware (no hardware video decode), DP audio errors (no DP connected).
