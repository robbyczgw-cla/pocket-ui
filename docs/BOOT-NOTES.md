# Retroid Pocket Mini V2: ROCKNIX boot notes (as of 2026-10-21)

> What shows on the screen from power-on to the shelf on pocket-ui's own boot path (the quiet start, the boot screen
> pocket-splash, the fade into the shelf) is in [BOOT-SPLASH.md](BOOT-SPLASH.md).
>
> **Status: background research.** This describes ROCKNIX's official route (OpenABL in `abl_a`/`abl_b`), which
> pocket-ui does **not** use, because it rewrites the bootloader. pocket-ui starts through Retroid's u-boot
> and GRUB from the SD card instead; see [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md). This page is kept for
> the recovery facts and the list of known issues.

Sources: rocknix.org/devices/retroid/retroid-pocket-mini/, ROCKNIX/distribution (main),
OpenHandheldCollective/OpenABL README (v1.1).

## Can the stock Android bootloader boot the SD card?
**No.** Retroid ships Qualcomm's stock ABL, which only boots Android boot images from internal UFS.
On SM8250, ROCKNIX uses `BOOTLOADER="qcom-abl"`, so it relies on **OpenABL**, which must be written to the
internal `abl_a` and `abl_b` partitions. OpenABL is what adds SD/USB boot and the Linux/Android
boot-mode switch.

→ Booting ROCKNIX from SD **requires an internal bootloader modification** (ABL partitions only).
Android's `boot`, `system`, `vendor`, `userdata`, apps and saves are not touched. OpenABL boots Android
"the same way as with Qualcomm's ABL, including verified boot and recovery".

## Official install path (performed from Android, as root)
1. Copy the `openabl` folder from the SD (the docs call it `rocknix_abl`) to Android internal storage.
2. Settings → Handheld Settings → Advanced → Run Script as Root:
   - `backup_abl.sh`: `dd` of `abl_a` / `abl_b` → `/sdcard/openabl/abl_{a,b}.img`
   - `flash_abl.sh`: `dd` of `abl_signed-SM8250.elf` → `abl_a` and `abl_b`
3. Reboot holding **Vol−** → OpenABL menu → Device model "Retroid Pocket Mini V2" → Boot mode Linux → Start.

## Recovery
- `restore_backup_abl.sh` writes the two backups back (from Android, as root).
- OpenABL keeps **fastboot** in its menu (Vol− at power-on). With the backups on a PC:
  `fastboot flash abl_a abl_a.img` and `fastboot flash abl_b abl_b.img`.
- OpenABL menu → "Emergency download mode" (Qualcomm EDL) is the last resort and needs Retroid's firehose/
  factory package.
- **Copy `abl_a.img`/`abl_b.img` off the device to the Mac right after the backup.** Keep them on
  Android internal storage and on the Mac.

## Known V2 issues
- Retroid's Android OTA for the Mini V2 ships a faulty bootloader. The boot-selection screen disappears,
  and the device can boot with the wrong device model (inverted screen). The documented fix is
  `fastboot flash loader u-boot-sm8250-retroidpocket-rpminiv2.img` (this is the older u-boot/GRUB path; check
  that it still applies under OpenABL before using it).
- An **Android OTA can overwrite ABL**, which drops back to stock and leaves no SD boot. Re-flash
  OpenABL afterwards.
- **ROCKNIX updates re-flash ABL automatically.** `bootloader/update.sh` → `/usr/bin/updateabl` compares
  `abl_a`/`abl_b` with the bundled OpenABL and `dd`s it over both slots if they differ. Expect this
  on every ROCKNIX update that bumps OpenABL.
- `installtointernal` exists on the image and must **not** be run (it would replace Android).
