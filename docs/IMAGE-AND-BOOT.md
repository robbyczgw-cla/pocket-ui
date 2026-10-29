# Permanent boot and our own image (pocket-os): Retroid Pocket Mini V2

Status 2026-10-21. [V] = verified (file, URL, or measurement), [I] = inferred, needs a device check.

## Summary

- **Boot (A):** flashing our 27 MB boot image into `loader` fails on size: `loader` is 10 MiB according to the
  factory firmware package (Mini V1; check V2). Recommendation: **option 1b**. `loader` holds only Retroid's small
  u-boot (632 KiB), and ROCKNIX starts through EFI/GRUB from the SD card. The stock ABL stays untouched, Android is
  the default, and Vol Up starts ROCKNIX. Read first, and test from memory with `fastboot boot`. **Plan B:**
  OpenABL v1.1 with Boot mode Android and Vol Up = Linux (external).
- **Image (B):** first repack SYSTEM (1 to 2 days), then a fork with a GitHub Actions build. A local build on a Mac
  is not practical (the builder image is amd64 only, 200 GB free are needed). Keep the unit name `essway.service`,
  rename DISTRONAME (protects against upstream updates) and remove non-commercial components and ROCKNIX
  branding.

> **Status:** part A became [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md) (option 1b, built and partly tested); what
> the screen shows on that path (quiet kernel command line, the boot screen pocket-splash) is in
> [BOOT-SPLASH.md](BOOT-SPLASH.md).
> Part B, an own *build* of the system, has **not** been started. Instead, since 2026-10-22 there is an SD card
> image that adds pocket-ui to the unchanged official ROCKNIX 20261001 image: [SD-IMAGE.md](SD-IMAGE.md). Dates and measurements below are
> from 2026-10-21.

## 0. Corrections to BOOT-NOTES.md

1. The stock Retroid ABL **can** boot an alternative OS: SM8250 devices have a `loader` partition, booted with Power+Vol Up or by setting the fastboot default boot to "Loader" [V: github.com/RetroidPocket/u-boot README, "Flashing u-boot"].
2. The 20261001 SD card ships the older **ROCKNIX ABL** (`rocknix_abl/`, ELF 258,048 B, sha256 `041778d6…`), not OpenABL [V: extracted from the image, also `usr/share/bootloader/rocknix_abl` in SYSTEM]. OpenABL v1.1 (ELF 532,480 B, sha256 `b0069146…`) is a different binary. ROCKNIX main (`1f2e51e`, today) already bundles OpenABL 1.1 (`projects/ROCKNIX/packages/tools/openabl/package.mk`), so the next ROCKNIX update swaps your ABL via `updateabl` (below).

## A. Permanent boot options

### Facts about `loader`
| Fact | Evidence |
|---|---|
| The partition is A/B: `loader_a`, `loader_b`, **10,485,760 B each**. For comparison, `abl_*` is 1 MiB and `boot_*` is 96 MiB. | [V] Mini **V1** factory QFIL package (Android10_RPMini_V1.0.0.310): file list and sizes read from the 7z header, github.com/TheGammaSqueeze/Retroid_Pocket_Stock_Firmware. V2 is [I] identical. |
| `fastboot flash loader` writes only the **active** slot (device reported slot b). | [I] standard fastboot behaviour. |
| Our test image is 27,299,840 B (Image 59.7 MB raw, gzip 27 MB, built-in initramfs). It **does not fit** into 10 MiB. | [V] `downloads/fastboot-test/`. |
| Retroid's u-boot for the V2 is 647,168 B (`rp-v1.0.1`, `u-boot-sm8250-retroidpocket-rpminiv2.img`). Its env is upstream-Qualcomm style (`bootcmd=bootefi bootmgr`). | [V] GitHub release; env file read in branch `retroidpocket/sm8250`. The final Retroid env may differ [I]. |
| Default content of `loader` on a V2: most likely this u-boot. Reasons: ROCKNIX's old install needed no flashing, and ROCKNIX tells V2 owners to *re-flash* `loader` after an OTA. | [I] unverified; read it first (Phase 0). |
| An Android OTA can rewrite `loader`: the OTA ships a faulty u-boot and the GRUB screen disappears. | [V] rocknix.org/devices/retroid/retroid-pocket-mini/ (the fix is `fastboot flash loader …`). With A/B, an OTA also switches the slot, so the image may sit in the stale slot [I]. |
| Default boot stays Android. Vol Up at power-on enters the loader once. "Boot mode: Loader" in the fastboot menu makes it permanent. | [V] Retroid README; the Reddit threads agree (not fetched, snippets only). |
| The ROCKNIX **pre-ABL** path was: u-boot → GRUB on the SD → kernel + DTB. The SD KERNEL is a PE/EFI-stub arm64 Image (`MZ`/`PE` found after gunzip). | [V] rocknix.org page ("select … from the GRUB screen"); [V] stub check. `projects/ROCKNIX/packages/tools/grub` still exists, but SM8250 now uses `BOOTLOADER="qcom-abl"` (`devices/SM8250/options`). |

### Comparison
| | 1. Full boot image in `loader` | **1b. Retroid u-boot in `loader`, EFI/GRUB on SD** | 2. OpenABL/ROCKNIX ABL in `abl_a/b` |
|---|---|---|---|
| Feasible | **No** (27 MB > 10 MiB) | Yes, with an SD EFI layout we still have to build | Yes (the documented ROCKNIX way) |
| Writes | – | `loader` only, and possibly nothing | `abl_a` and `abl_b` |
| Worst case | – | Bad `loader` does not stop Android. Vol Down gives stock fastboot, then reflash the backup. | Bad ABL means a hard brick: XBL never falls back [V: OpenABL README]. EDL needs a Retroid firehose; the V1 one exists, V2 and 2026 validity unverified [I]. |
| OTA | Overwrites `loader` | Overwrites `loader` (harmless, re-flash) | Overwrites ABL, no SD boot until re-flashed |
| Android vs ROCKNIX | – | **Power on = Android. Vol Up = ROCKNIX.** | OpenABL: set *Boot mode* Android and *Volume up at boot* Linux (external) for the same UX [V: README table, `Core/Settings.c`]. **Default is Linux** (`Settings.c:21`), and without an SD card it opens the menu instead of Android. ROCKNIX ABL 1.2 default [I], unreadable (compressed). |
| ROCKNIX updates | Kernel in flash goes stale | Everything lives on the SD | Native (KERNEL on SD) |
| Stock ABL | Kept | Kept | Replaced (Retroid's loader menu disappears) |

### Recommendation: 1b, with 2 as plan B
Android stays untouched, and the only flash target (if any) is the small `loader`. UFS is invisible to Linux in our DTB, so ROCKNIX scripts such as `updateabl` find no `abl_*` partitions and exit [I: `blkid -t PARTLABEL=abl_a` in `updateabl`].

**Plan (each step reversible):**
0. **Read-only inventory.** On the Mac: `fastboot getvar all`, `partition-size:loader_a/b`, `current-slot`. In Android (Run Script as Root): `dd if=/dev/block/by-name/loader_a of=/sdcard/loader_a.img` (same for `_b`), then `sha256sum` and `strings | grep -i u-boot`. Copy to the Mac. Also back up `abl_a/b` with the existing `scripts/android/safe_backup_abl.sh`.
1. **RAM test, zero writes.** Build the SD with `EFI/BOOT/BOOTAA64.EFI` (GRUB from the ROCKNIX `grub` package or an older SM8250 image, with `grub.cfg` pointing at the raw Image from KERNEL, the Mini V2 DTB and our cmdline). Then run `fastboot boot u-boot-sm8250-retroidpocket-rpminiv2.img` (the Retroid README documents this).
2. **Only if step 0 shows the loader is not this u-boot**, or it is the faulty OTA version: `fastboot flash loader_a` and `loader_b` with the verified Retroid image.
3. **Daily use:** Vol Up = ROCKNIX, nothing = Android. Never set "Boot mode: Loader" as default (the power-button hold then only reboots).
4. **Fall back to plan 2** (OpenABL v1.1, `safe_*` scripts, verified sha, then Android mode + Vol Up Linux) if the u-boot chain cannot boot our kernel. The ELF is two days old, so it needs a dry run of all menu paths while the SD is out.

## B. pocket-os from the ROCKNIX build system

### Building SM8250
- `make docker-SM8250` [V: `Makefile`]. It runs `make SM8250`, which builds `ARCH=arm` (32-bit) and `ARCH=aarch64`. The builder image is `ghcr.io/rocknix/rocknix-build:latest` (`Dockerfile`: ubuntu:jammy).
- **The image is amd64 only** [V: I read the GHCR manifest]. On Apple Silicon this means Rosetta/QEMU emulation, which is slow, and rocknix.org tests only Ubuntu 22.04 hosts. The `Dockerfile` has an aarch64-host branch, so a native arm64 Linux VM may work [I].
- rocknix.org/contribute/build [V]: **200 GB free** for one device, a first build of about **ten hours**, rebuilds in minutes. This Mac has 10 cores, 32 GB RAM and 142 GB free, so a full build does not fit. RAM is not documented; LTO is on (`LTO_SUPPORT="yes"`), so plan for 16 GB or more [I].
- **Recommended:** fork `ROCKNIX/distribution`, then run `build-nightly.yml` via `workflow_dispatch` with `SM8250=true` (staged jobs in `build-device.yml`, ubuntu-24.04). The builder image comes from `build-docker-image.yml`. The ccache download and the ES API secrets are optional [I]. A rented x86 box (16 cores, 300 GB) is the alternative.
- Cheap first step: **repack SYSTEM** without compiling. It is a zstd SquashFS (1.49 GB packed, 4.7 GB unpacked) that opens with `7z`. Of that, `usr/lib/libretro` is 1.32 GB and `usr/lib/wine` 546 MB. Unpack, overlay our files, re-squash with zstd, regenerate `SYSTEM.md5`.

### Minimal change set (fork)
1. **Binaries and service as an overlay:** `projects/ROCKNIX/devices/SM8250/filesystem/usr/bin/pocket-ui` plus `…/usr/lib/systemd/system/essway.service`. `scripts/image:178-195` copies the device overlay after the packages and auto-enables every `*.service` there [V].
2. **Keep the unit name `essway.service`.** `runemu.sh:117` uses `StopPropagatedFrom=essway.service`, `quirks/platforms/SM8250/090-ui_service` and `sway/autostart/111-sway-init` reference it, and `rocknix-fake-suspend` and `run` use `UI_SERVICE` [V]. Only `ExecStart` changes.
3. **Cleaner packaging:** `projects/ROCKNIX/packages/ui/pocketui/package.mk` with `PKG_TOOLCHAIN="manual"`, following `network/rclone/package.mk` (prebuilt tarball, `PKG_URL` plus `PKG_SHA256`) or `rocknix/package.mk` (`PKG_URL=""`, files from `${PKG_DIR}/sources`). Build with the same glibc target, e.g. `cargo zigbuild …gnu.<glibc>`.
4. **Drop packages:** `projects/ROCKNIX/packages/virtual/image/package.mk:18` `PKG_UI="emulationstation es-themes textviewer lowerdeck"` becomes `pocketui`. Trim `PKG_MULTIMEDIA` (vlc, mpv, gmu), `PKG_SYNC`, `PKG_GRAPHICS` (imagemagick, an ES scraper dependency [I]) and the `distributions/ROCKNIX/options` flags (`SAMBA_SERVER`, `ZEROTIER_SUPPORT`, `WIREGUARD_SUPPORT`, …). ROCKNIX has no Kodi. `BASE_ONLY` drops ES *and* all emulators, so it is too coarse. Cut emulators in `virtual/emulators/package.mk`. **Keep it**: `es_systems.cfg` is generated there (lines 1699–1707), not by the ES package [V].
5. **Known ES couplings:** `profile.d/100-gamecontroller-functions` reads `es_input.cfg`, and `runemu.sh` reads `/storage/.emulationstation/es_settings.cfg` and `set_kill "emulationstation"`. Check these paths on the device [I].

### Update path under our control
- Rename `DISTRONAME` (`distributions/ROCKNIX/options`). The initramfs accepts only update files whose name contains `@DISTRONAME@` (`busybox/scripts/init`, `check_update`) [V]. That blocks upstream ROCKNIX tars.
- `rocknix-update` polls `https://update.rocknix.org` (line 10). Delete it or point it at our own endpoint. pocket-ui never calls it.
- Edit `devices/SM8250/bootloader/update.sh`: remove the `rocknix_abl`/`openabl` copy and the `updateabl` call, and drop `openabl` from `ADDITIONAL_PACKAGES`. The init runs the **new** SYSTEM's `update.sh` on every update (`update_bootloader`) [V].
- Push updates: `scp <tar> root@host:/storage/.update && reboot` (rocknix.org/contribute/modify) [V].
- Change `ROOT_PASSWORD="rocknix"` and decide on SSH. (For the SD card image, which keeps ROCKNIX's build: SSH,
  Samba and Syncthing are off by default and pocket-ui sets a per-device password when they are turned on; see
  [SECURITY-HARDENING.md](SECURITY-HARDENING.md#rollout-order).)

### Licensing (not legal advice)
- **GPL:** kernel, busybox, RetroArch, PPSSPP, bash and others. Offer the corresponding source: our fork plus `make src-pkg` (tar of `sources/`) [V]. Keep copyright headers.
- **Branding:** `LICENSE.md` puts ROCKNIX branding and images under **CC BY-NC-SA 4.0** and states the distro "includes components licensed for non-commercial use only" [V]. Rename `DISTRONAME`, `OSNAME`, `HOME_URL`, `WIKI_URL`, `BUG_REPORT_URL`, `GIT_*`. Replace `distributions/ROCKNIX/logos/rocknix-logo.png` and the `rocknix-splash` package. `DISTRO_BOOTLABEL="ROCKNIX"` is only the label used in `boot=LABEL=…`; change it in all places together.
- **Non-commercial or proprietary packages** [V: `PKG_LICENSE`]: snes9x*, genesis-plus-gx, fbneo, mame2000/2003/2010, picodrive, fmsx, opera, `drastic-sa`, `bigpemu-sa`. `duckstation-sa` is **CC-BY-NC-ND-4.0**, so patched builds must not be redistributed. The ES theme `art-book-next` has a "CUSTOM" licence.
- Private use is fine. For anything given to other people or sold with hardware, **build without the NC and proprietary packages and without ROCKNIX branding**. Check the firmware blobs in `FIRMWARE="extra-firmware"` / `linux-firmware` for redistribution terms; I could not inspect them [I].
- pocket-ui is clean-room (no NextUI code), so its licence is free. It only starts `runemu.sh` (aggregation), and the SDL3 dependency is zlib.

### Effort and first milestone
| Step | Effort |
|---|---|
| Phase 0 inventory, backups | 0.5 day |
| u-boot to EFI/GRUB proof of boot | 2–4 days |
| SYSTEM repack with pocket-ui as UI | 1–2 days |
| Fork with first green CI build (rename, overlay, service) | 3–5 days elapsed, mostly CI wait |
| Slimming, update path, branding, licence cleanup | 1–2 weeks |

Overall about 3–4 weeks part-time for a distributable pocket-os.

**Milestone M-IMG-1:** an SD with repacked SYSTEM and EFI layout boots **Vol Up → u-boot → GRUB → pocket-ui** on the device, with no new flash write (`fastboot boot` of the u-boot image only). Power-on without keys gives Android, and the Android partitions are unchanged. Measure and record the boot time.
