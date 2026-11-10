# The SD card image

Status: 2026-10-28. The image builds (on an arm64 Mac and, since 2026-10-26, on x86_64 Linux) and passes every
offline check below. It was **started once on a fresh card on a Mini V2 on 2026-10-27** (image from `5e9533d`): it
booted and the first-start setup ran, and that run found the Wi-Fi first-start bug (a wrong password looped; fixed in
`9fd8929` and checked in the Wi-Fi bench, not yet on a fresh card). A complete first start as a new player (flash,
resize, Boot mode, setup, a first game without help) has not been repeated with the fixed build: the steps for that
test are at the end. Players install it as in [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md).

[V] marks something checked (a file, a source, a measurement). [I] marks something inferred.

```sh
scripts/make-image.sh                 # build/image/pocket-ui-<version>-rp-mini-v2.img.xz, .txt, SHA256SUMS
scripts/check-image.sh build/image/pocket-ui-<version>-rp-mini-v2.img.xz
sh deploy/image/test-mount-storage.sh # tests of the boot hook: seed and update guard (part of the gate)
sh deploy/image/test-hooks.sh         # tests of post-flash.sh (Docker, the virtual device's base image)
```

`make-image.sh` takes `--version` (default: Cargo.toml), `--bin-dir` (prebuilt binaries instead of `cargo zigbuild`),
`--out`, `--keep-work` and `--no-check`. It needs Docker. It writes only below `build/image/` (gitignored) and the
download cache, and never touches a disk. A build takes about 6 minutes; most of it is compiling and `xz`. It runs on
an arm64 Mac and on an x86_64 Linux machine alike: every container runs on the machine's own architecture (GRUB too,
step 3), and the files it leaves behind belong to the user who ran it.

For a card that already runs ROCKNIX there is a second artefact, the install package
(`scripts/make-install-zip.sh` → `build/image/pocket-ui-<version>-rocknix-install.zip`, tests
`sh deploy/package/test-package.sh`), described in [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md#install-on-a-rocknix-card-you-already-use).

H700 releases also pass `--gba-core BUILD_DIR` to `scripts/make-image-family.sh --family H700`, using the pinned
ARM64 gpSP output from `scripts/build-gpsp.sh`. `make-image.sh` accepts the same option. The verified library,
COPYING, corresponding source and recipe go into `pocket-ui/releases/<version>/cores/`; the image without this
option has the same layout as before. Check the resulting image with the explicit packaged-core allowlist
described in [UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#releasing-step-by-step), keeping source-tree checks core-free.

**For the Mini V2 only.** The image boots through GRUB with the Mini V2's device tree; the experimental Pocket 5,
Flip 2 and Mini V1 need ROCKNIX's own image and the install package ([DEVICES.md](DEVICES.md#installing)). ROCKNIX's
SM8250 image itself serves all of them because its bootloader picks the device tree ("Set device model"); this
project's GRUB path has one tree and no choice, and a second image per device is not built.

## How the image is made

**Approach: the official ROCKNIX image, with files added, nothing rebuilt.** The alternatives in
[IMAGE-AND-BOOT.md](IMAGE-AND-BOOT.md) part B (a ROCKNIX fork) cost days and would replace the kernel and system that
are verified on the device. This way the kernel and the boot files are byte for byte the ones that boot on the
author's device today. Since 2026-10-28 SYSTEM is repacked without eight paths whose licences do not allow passing
them on (below, "SYSTEM without a few files"); every other file in it is ROCKNIX's, unchanged.

1. `ROCKNIX-SM8250.aarch64-20261001.img.gz` from the GitHub release `20261001`, checked against the published
   sha256 (pinned in `scripts/image/pins.sh`), cached in `downloads/rocknix-20261001/`.
2. KERNEL is copied out of the FAT partition and checked (md5 `e313a71b…`, the same as the device's `/flash/KERNEL`).
   The Mini V2 device tree is taken from it and the internal UFS switched off with `fdtput`. The result is byte for
   byte the device's `miniv2-noufs.dtb` (md5 `1ff6d2a9…`). [V]
3. `scripts/make-sd-efi.sh` unpacks the kernel `Image` and builds GRUB. Both match the device's files byte for byte,
   GRUB included (Debian `grub-efi-arm64-bin 2.12-9+deb13u2`). [V] GRUB is made by `grub-mkimage` of the build
   machine's own architecture from the arm64 modules in Debian's package: until 2026-10-26 the script ran an arm64
   container, which fails on an x86_64 machine without qemu ("exec format error"). Built both ways, x86_64 and arm64
   give the device's `BOOTAA64.EFI` (md5 `3e6c69bc…`). [V] The GRUB version is recorded in
   `boot/grub/grub-efi-arm64-bin.version` for the source offer.
4. pocket-ui and pocket-core are built with `POCKETUI_VERSION=<version>`, as `scripts/release-app.sh` does.
5. The container half (`scripts/image/build.sh`, tools in `scripts/image/Dockerfile`) adds the files with
   `mtools` (FAT) and `debugfs` (STORAGE ext4), without loop mounts, as root, with the commit's time stamp.
6. `build.sh strip-system` repacks SYSTEM without `SYSTEM_REMOVE` (`scripts/image/pins.sh`): `unsquashfs` as root,
   `rm`, `mksquashfs` with ROCKNIX's settings (zstd level 19, 1 MiB blocks, no xattrs, ROCKNIX's own creation time
   from the superblock, so the md5 does not depend on our commit). It fails unless the new listing (path, mode, owner) is the old one minus exactly those paths, and the
   result must have the pinned md5 `POCKET_SYSTEM_MD5`. About 20 minutes with 4 cores (`SQUASH_CPUS`).
7. After the FAT partition is written, `scripts/image/fat-free-zero.py` zeroes its free clusters: replacing SYSTEM frees
   the clusters of ROCKNIX's original SYSTEM without clearing them, so its bytes (with the files left out on purpose)
   would otherwise ride along in the image (540 MiB, found on 2026-10-28 by the image's size: 2.07 GB instead of
   1.52 GB). `fsck.vfat -n` must then find the partition clean; `check.sh` checks both again.
8. The notes get ROCKNIX's `LICENSE.md` and `licenses/` folder from the tag (a sparse git clone, checked against
   the pinned commit, cached next to the ROCKNIX image), `deploy/image/firmware-licences/` and the source offer
   (`deploy/image/SOURCE-OFFER.txt.in`). `--source` and `--release-url` set where the notes say pocket-ui's source
   tarball and the issues page of the written offer are; for the public image: the public repository's release commit and release page.

### SYSTEM without a few files

Checked on 2026-10-28 in the unpacked ROCKNIX 20261001 SYSTEM and ROCKNIX's package files at the tag:

| Path in SYSTEM | What it is | Decision |
|---|---|---|
| `usr/config/amiberry/whdboot/save-data/Kickstarts/kick*.RTB` (6 files, 3.9–5 KB) | Kickstart relocation tables for WHDLoad in Amiberry, copied from Amiberry's repository (`whdboot/`, commit `5c54536`). Byte for byte the files in SKick 3.46 by Pavel Troller (aminet.net `util/boot/skick346.lha`). No ROM code, but SKick.doc section 1: "This archive may be freely redistributed, but only in totally unchanged state, i.e. no files can be added, deleted, modified". Loose files are not covered | **removed.** Amiberry is not the default Amiga emulator on SM8250 (PUAE is, `virtual/emulators/package.mk`); WHDLoad games in Amiberry need the `.RTB` files from that archive copied to `/storage/.config/amiberry/whdboot/save-data/Kickstarts` (the notes say so) |
| `usr/config/amiberry/whdboot/boot-data.zip`: `Devs/Kickstarts/*.RTB` | The same six SKick tables again, inside Amiberry's WHDLoad boot archive (found by `check-release-tree.py` on the unpacked SYSTEM) | **removed from the archive** with `zip -d` (`SYSTEM_ZIP_REMOVE`); its other entries (AmigaOS-style commands and libraries, WHDLoad configs) stay as Amiberry and ROCKNIX ship them; not investigated further |
| `usr/share/idtech/doom.tar.gz` | The DOOM shareware IWAD (`doom.wad` v1.9, 4,196,020 bytes, md5 `f0cefca4…`), downloaded by `idtech-lr`'s `PKG_DOOM_SHAREWARE` from `ROCKNIX/packages` for the manual "Scan id Tech Games" tool. A game, alone, without the shareware package's other files and terms | **removed**: the card says "no games". Without it `generate-idtech.sh` creates no Doom entry; prboom itself stays |
| `usr/share/bootloader/rocknix_abl/` | ROCKNIX's ABL: `abl_signed-SM8250.elf` and the scripts that flash it, from the `rocknix-abl` package, a prebuilt release asset of `github.com/ROCKNIX/abl` (README and `update.sh` only: no licence, no source). Its upstream asset no longer matches the package's sha256 (2026-10-28), so not even that can be fetched again | **removed**, as the same folder on the FAT partition already is: this boot path never flashes an ABL |
| `usr/lib/kernel-overlays/base/lib/firmware/qcom/sm8250/ayn/`, `…/mangmi/` | Firmware for the AYN Thor and the Mangmi Pocket Max from ROCKNIX's `extra-firmware` repository (`PKG_LICENSE="proprietary"`, no licence file in the repository) | **removed.** The Mini V2's device tree asks only for `qcom/sm8250/{a650_zap,adsp,cdsp,slpi}.mbn` |
| `usr/config/nanoboyadvance/bios/gba_bios.bin` (16 KiB, md5 `1876f71b…`) | Not Nintendo's BIOS: ROCKNIX's `nanoboyadvance-sa` package downloads `PKG_OPEN_SOURCE_BIOS` = `github.com/Nebuleon/ReGBA/raw/master/bios/gba_bios.bin`, the open-source replacement BIOS by Normmatt and the VBA-M team (`bios/source/*.c`: GPL-2.0-or-later). Same md5 as ReGBA's file at `7f37905` | **kept**; named in the notes, its source goes next to the image (`regba-bios-source.tar.gz`), `check.sh` pins its md5 |
| Firmware for the Mini V2 (GPU, DSP, video, Wi-Fi, Bluetooth, USB network, regulatory database) | linux-firmware 20260309 and wireless-regdb 2026.05.30, every file md5-identical to upstream | **kept**; their terms (WHENCE: "Redistributable", `LICENSE.qcom` requires the text to travel with the files) go into `pocket-ui-info/firmware-licences/` with a file list. ROCKNIX's SYSTEM itself carries none of these texts |

Proprietary or NC packages that stay, as in ROCKNIX: `aethersx2-sa` (CC BY-NC-ND 4.0 plus an LGPL library whose
source its authors offer by e-mail; `AETHERSX2.pdf` in `rocknix-licenses/licenses/`), `bigpemu` (free, non-commercial,
"may not distribute … without the inclusion of this document in unmodified form": `usr/share/bigpemu/ReadMe.txt` is in
SYSTEM unchanged), `duckstation-sa` (CC BY-NC-ND 4.0, passed on unchanged), the NC libretro cores (LICENCES.md §8). All
fit "free, non-commercial, unchanged, credited". DraStic is **not**
in the SM8250 SYSTEM (only two sleep scripts of the Anbernic RG DS name it).

### What changes on the card

| Partition | Added | Removed |
|---|---|---|
| FAT, label `ROCKNIX` (`/flash`) | `EFI/BOOT/BOOTAA64.EFI`, `boot/grub/grub.cfg`, `boot/grub/grub-efi-arm64-bin.version`, `boot/pocket/{Image,miniv2-noufs.dtb,KERNEL.source.md5}`, `post-flash.sh`, `mount-storage.sh`, `pocket-ui-info/` (`README.txt`, `SOURCE-OFFER.txt`, `LICENSE`, `NOTICE.md`, `licences/`, `rocknix-licenses/`, `firmware-licences/`) | `rocknix_abl/`: ROCKNIX's replacement for Retroid's bootloader and the scripts that flash it. This boot path never uses it, and flashing it is what this setup avoids. `SYSTEM` and `SYSTEM.md5` are replaced by the repacked SYSTEM (above) |
| STORAGE, ext4 | `pocket-ui/` (launcher, `releases/<version>/` with pocket-ui, pocket-core and pocket-splash, `current` and the three links, as `dev.sh` and the updater lay it out), `.pocket-seed/.config/system.d/`: `essway.service.d/10-pocket-ui.conf`, and the boot screen `pocket-splash.service` with its link `sysinit.target.wants/pocket-splash.service` ([BOOT-SPLASH.md](BOOT-SPLASH.md)) | nothing (`.please_resize_me` and `lost+found` stay) |

Partition table, partition sizes and KERNEL are unchanged; SYSTEM only lost the paths above. Image: 2,198,863,872 bytes raw, 1,518,697,476
bytes as `.img.xz` (version 0.1.0, commit 4cc9c97, 2026-10-26). STORAGE holds 16 MB of its 32 MB before the first start
(6.9 MB on 2026-10-22: the programs have grown; the check warns above 24 MB).

## How ROCKNIX sets up a new card, and how our files get through it

Sources: ROCKNIX tag `20261001` (commit `c445081a…`), and the same files read from the image's own initramfs and
SYSTEM. The initramfs `init` inside the kernel matches
`projects/ROCKNIX/packages/sysutils/busybox/scripts/init` except for the build-time `@…@` substitutions. [V]

- **STORAGE exists in the image**, 32 MiB ext4, label `STORAGE`, holding only `lost+found` and
  `.please_resize_me`. [V]
- **First boot.** `init` sees `/storage/.please_resize_me` and starts systemd with `--unit=fs-resize.target`
  (end of `init`). `fs-resize.service` runs `/usr/lib/rocknix/fs-resize`: `parted resizepart 2 100%`, `e2fsck`,
  `resize2fs`, new UUIDs (`tune2fs -U random`, `fatlabel -i -r`; the labels stay), then a reboot. [V]
- **`resize_inode` is off on our STORAGE** (ROCKNIX's has it). `fs-resize` ignores what `resize2fs` returns and prints
  "done". With `resize_inode` and our programs on STORAGE, `resize2fs` fails on a card over about 36 GB once more than
  about 18 MB is in use, after moving the root directory: the next boot says "Filesystem corruption", and the repair
  leaves an empty 32 MB STORAGE with everything in `lost+found`. Nothing needs the feature: it serves online growing
  only. `scripts/image/grow-check.sh`, part of the image checks, replays the steps from 8 GB to 2 TB. [V]
- **The catch:** `fs-resize` refuses to grow STORAGE if `/storage/.config` or `/storage/.cache` already exists
  ("Resizing is not permitted - the system has already been initialised"): it deletes the marker, reboots, and the
  card keeps a 32 MB STORAGE for good. [V] Our essway drop-in has to live in `/storage/.config/system.d`, so it
  cannot be on the card from the start.
- **`/storage/.config` is filled later**, at the second boot, by ROCKNIX's autostart: `001-setup` runs
  `chksysconfig verify`, which runs `rsync -a /usr/config/ /storage/.config` when its key files are missing. It has
  no `--delete`, and `/usr/config/system.d` holds only a README and a wireguard sample, so a drop-in already there
  survives. [V] `/storage/.config/system.d` is on systemd's unit path (`systemctl show -p UnitPath` on the device). [V]
- `init` also knows `/storage/.restore/*.tar` (`--unit=backup-restore.target`), but ROCKNIX ships no such unit,
  so that route would fail to boot. Not used. [V] (`SYSTEM` has no `backup-restore.*`)

**Our way through: the hooks ROCKNIX's `init` offers on `/flash`.** `init` sources `/flash/post-flash.sh` after
mounting `/flash`, `/flash/post-sysroot.sh` after mounting SYSTEM, and uses `/flash/mount-storage.sh` instead of its
own mount of `/storage` (`mount_flash`, `mount_sysroot`, `mount_storage`). [V]

- `deploy/image/mount-storage.sh` mounts `/storage` exactly as `init` would (`mount_part "$disk" /storage
  rw,noatime`). While `.please_resize_me` exists it does nothing else. At the second boot it copies
  `/storage/.pocket-seed/.` into `/storage` (`cp -a`) and removes the seed. This happens in the initramfs, before
  systemd starts, so the drop-in is there when systemd reads its units, and pocket-ui comes up at the very first
  normal start. The seed also holds `.config/autostart/pocket-ui-image-defaults` (`deploy/image/image-defaults.sh`):
  ROCKNIX's autostart runs it once, after `001-setup` has put `system.cfg` in place and before the UI starts; it sets
  `updates.enabled=0` and `system.hostname=pocket-<model>` (`pocket-mini-v2`, `pocket-rg35xx-plus`, `pocket-rg35xx-sp`, `pocket-rg28xx`, `pocket-odin2-mini`, ...; `image_hostname` in `scripts/image/pins.sh`, put on the script's `NAME` line by the build, so two handhelds on one network or Tailscale differ; ROCKNIX's own default is the family name) with ROCKNIX's `set_setting` and deletes itself. (A seeded
  `system.cfg` would not survive: `chksysconfig verify` copies all of `/usr/config` over `.config` while
  `retroarch.cfg` is missing.)
- **The hook stays on the card and guards every start against foreign system updates** (since 0.1.7). It runs before
  `init`'s `check_update`, which installs any `*.tar`, `*.img`, `*.img.gz` or `KERNEL`+`SYSTEM` in `/storage/.update`
  (from EmulationStation's updater, the Samba share `update`, a copy by hand) and would replace the system without
  GRUB's `boot/pocket/Image` and flash the ABL. pocket-ui's own `update::stage_os` writes
  `/storage/.update/.pocket-ui-approved` (`<sha256>  <tar name>`) before it moves a checked tar there. The hook leaves
  a tar only when that line names it and `sha256sum` agrees; everything else (no or another approval, a changed file,
  disk images, a loose KERNEL/SYSTEM) is moved, never deleted, to `/storage/.update-refused/` with a line in its
  `refused.log`, and the start goes on as usual; a name already there gets `.1`, `.2` ..., nothing is ever replaced. If
  that move fails, the file is renamed in place to `<name>.refused` (again a free name), which `check_update`'s
  patterns don't match. If even that fails (a read-only or damaged STORAGE), the hook, which `init` sources in its own
  shell, redefines `check_update` as a no-op for this start: the foreign file stays untouched and the start goes on
  normally. Every doubt ends in "refuse and boot normally". ROCKNIX's cleanup after an
  install removes the approval with the tar.
- `deploy/image/post-flash.sh` replaces `init`'s `load_splash` with the same function minus the call to
  `/usr/bin/rocknix-splash`, which draws the ROCKNIX logo. It prints "pocket-ui, based on ROCKNIX" and ROCKNIX's
  version line instead. It stays on the card and runs at every boot.
- Both run in the initramfs's BusyBox (1.36.1; its `busybox-init.conf` has `cp`, `mv`, `sha256sum`, `basename`, `ls`,
  `cut`, no `date` and no `test` applet). `deploy/image/test-mount-storage.sh` (in the gate, about 1 s) runs
  `mount-storage.sh` under BusyBox with a PATH of only those applets and fake `mount_part`/`mount`/`progress`:
  first, second and later boots, existing files in `.config`, and the guard: an approved tar stays, a tar without
  approval (ES), a second tar dropped next to an approved one (Samba), the approved name with other contents, empty
  or broken approvals, disk images and KERNEL/SYSTEM, repeated refusals, other files in `.update`, a refusal folder
  that can't be made (twice: the second never replaces the first), and a STORAGE where nothing can be moved (the
  update check is skipped; an approved update still installs). The fake init calls `mount_storage` and then
  `check_update` by name, as the real one does. [V] With the hook of 0.1.6, 24 of the first 42 checks fail; the
  first 0.1.7 hook fails the 3 checks of the review's findings 2 and 3. `deploy/image/test-hooks.sh` runs
  `post-flash.sh` in the virtual device's base image (Docker).

So a new card goes: **boot 1** grows STORAGE and restarts; **boot 2** puts the drop-in and the boot screen's unit in
place (`cp -a` keeps its link), the boot screen shows "Getting ready for the first start…", ROCKNIX fills
`/storage/.config`, and the essway unit starts pocket-ui, which finds no `settings.json` and opens its first-start
setup. [I] for the whole sequence on a device; each part is [V] as above.

## H700: RG35XX Plus, RG35XX SP, RG28XX (prepared, never booted)

Status 2026-10-30: the builder and the offline check exist and pass; **no image has been started on a handheld.**
Built on an x86_64 Linux machine (3.6 GB of images and 7.5 GB of source are too much to move around). Nothing is published.

```sh
scripts/make-image-h700.sh --version V           # make-image-family.sh --family H700: build/image-h700/pocket-ui-V-<model>.img.xz, .txt, SHA256SUMS
scripts/check-image-h700.sh build/image-h700/pocket-ui-V-rg35xx-sp.img.xz
scripts/image/rocknix-source.sh OUT --device H700   # the source bundle of the H700 build (source-bundle.sh --device H700)
sh scripts/image/test-h700-pins.sh                   # pins, notes and model list (part of the gate)
sh scripts/image/test-storage-budget.sh              # STORAGE room and what build.sh writes there (part of the gate)
sh scripts/image/storage-budget.sh BIN_DIR           # do these AArch64 programs fit into STORAGE? (release-check.sh runs it)
```

**Rebuilding for a release**: build the three binaries from the
exported public commit, then one `make-image.sh` run (Mini V2) and one `make-image-family.sh --family H700 --models "... rg40xx-h-ddr3"`
run (all H700 models including the DDR3 card; SYSTEM is repacked once per run, about 13 minutes). Every image ends with its own
`check-image-family.sh` (grow-check from 8 GB to 2 TB included). Then
`python3 -I scripts/image/compare-images.py OLD.img.xz NEW.img.xz --work DIR` against the same model of the last release (it takes the
pocket-ui version folder from the images, so `0.3.0-dryrun` works; the launcher and `portmaster-setup.sh` of the program are allowed
differences; anything else is a finding to explain from `git diff` of the two public commits), and
`sh scripts/image/release-sums.sh OUT_DIR BUILD_DIR...` for `SHA256SUMS-images` / `SHA256SUMS-images-more`. The export itself:
`export-public.sh --allow-hand-commits --update PUBLIC_CLONE ...` (the public repository has hand commits from 0.1.x); a test that times out
on a busy build machine gets one more try alone and is named as FLAKED in the report.

**Image and release receipts.** An output file and its checksum are the `assembled` stage. Both builders write
`<image>.img.xz.receipt.json` with the image and accompanying notes' SHA-256 and byte counts. `--no-check` leaves
that stage in place. For compressed images with notes and `SHA256SUMS`, the normal `check-image.sh` and
`check-image-family.sh` entrypoints supervise their existing checks, record `<image>.img.xz.check.log`, and hash both
files again afterwards. Only an unchanged pair with `IMAGE OK`, a successful grow check and the STORAGE headroom
from `pins.sh` becomes `checked`. A failed/interrupted check leaves it assembled. Standalone/raw diagnostic images
can still be checked without generating a release receipt.

After preparing the image-only release checksum lists, run:

```sh
python3 -I scripts/image/receipt.py release IMAGE_DIR --tag vVERSION
sh scripts/upload-images.sh vVERSION IMAGE_DIR --repo OWNER/REPO --dry-run
```

The first command verifies every image, note and checked receipt and writes `image-release.receipt.json` at stage
`releasable`, binding the exact `SHA256SUMS-images`, `SHA256SUMS-images-more` and/or `SHA256SUMS-images-sm8550` files.
The second fully rehashes the files and shows the upload plan. An upload uses that same command without `--dry-run`;
it refuses the whole set before calling `gh` if any image, note, checksum list or receipt differs. It uploads only
the enumerated images, notes and image checksum lists; the unrelated installer `SHA256SUMS`, local logs and receipts
are excluded. Keep the receipt beside the outputs when moving a finished set. A receipt attests offline file
checks, with binary-version capability recorded separately; hardware first-boot/game checks and publication approval
remain separate.

Existing finished builds need no rebuild. Import their original build/check logs and `SHA256SUMS`:

```sh
python3 -I scripts/image/receipt.py import-release IMAGE_DIR --tag vVERSION --log BUILD_LOG --log ANOTHER_BUILD_LOG
```

Each image needs exactly one supplied log containing its latest check block and the image/notes hashes printed
before that check. A matching name or a bare `IMAGE OK` is insufficient. The importer requires zero warnings inside
the image checks, the successful grow check and the landed STORAGE floor. The known missing-aarch64-container
`--version` warning remains an explicit limitation. Original logs and images are unchanged. The same fixtures cover
legacy import, current check entrypoints, altered bytes with restored size/mtime and refusal before any upload:
`python3 -I scripts/image/test-receipts.py` (also in the merge gate).

**What differs from the Mini V2 image.** ROCKNIX's H700 image boots through its own U-Boot (written 8 KiB into the card,
before the first partition) and `extlinux/extlinux.conf` (`LINUX /KERNEL`, `FDT /dtb.img`). So there is no GRUB, no
unpacked kernel and no edited device tree: the image is the official ROCKNIX H700 DDR4 image, SYSTEM repacked without
the removed files, our two initramfs hooks (`post-flash.sh`, `mount-storage.sh`: the same files as for the Mini V2; the
H700 initramfs has the same hook points and the same `load_splash`, checked in the unpacked KERNEL), the notes, and
pocket-ui on STORAGE. The partition table is MBR (the Mini V2's is GPT) with the same offsets.

**One image per model, one file apart.** ROCKNIX's install page has the player copy the model's tree from
`device_trees/` to the FAT root as `dtb.img`. We do it at build time (`pins.sh` `H700_MODELS`: RG35XX Plus
`sun50i-h700-anbernic-rg35xx-plus.dtb`, RG35XX SP `...-rg35xx-sp.dtb`, RG28XX `...-rg28xx.dtb`, each with its md5). More models are in `H700_MODELS` as untested expectations (same image, their own tree: RG35XX H, Pro, 2024, RG40XX H and V, RG34XX, RG34XX-SP, RG-SP, CubeXX, and the Plus rev6 and SP v2 panel trees); ROCKNIX does not say which model has DDR3, so the RG35XX 2024 (the only DDR3 overlay is named for it) may need the DDR3 image. The CubeXX image is for testing only: pocket-ui's profile holds that model back (`not_yet`).
The images differ in `dtb.img` and in the model name in the notes, nothing else. The newer screen revisions of the Plus
and the SP (`rg35xx-plus-r6` for the Plus's rev6 panel, `rg35xx-sp-v2` for the SP's v2 panel; short names so the hostname
`pocket-<model>` stays within 24 characters) have their own trees; a unit with a broken picture on the plain one needs that
variant. The other panel trees (RG35XX 2024 and H rev6, RG40XX H and V v2, RG34XX-SP v2) are not built yet.

**RAM type.** The base is the DDR4 image: U-Boot for LPDDR4 units. The RG35XX Plus reads `vdd-dram` 1.1 V (DDR4). The
RG35XX SP and the RG28XX are **expected** to be DDR4 (ROCKNIX's device pages), not confirmed on the units; a DDR3 unit
would not start (the card is not harmed). If the SP or the RG28XX turns out to be DDR3, the DDR3 image has other
hashes (`ROCKNIX-H700...-DDR3.img.gz.sha256`) and its own `H700_DDR3_` pins (already there).
**The DDR3 variant** (`rg40xx-h-ddr3`, a fifth field `ddr3` in `H700_MODELS`) is a DDR4 card with the two things that differ in
ROCKNIX's DDR3 image: its first 16 MiB (partition table and U-Boot with the DDR3 RAM set-up) and `extlinux/extlinux.conf` (one more
line, `FDTOVERLAYS /overlays/sun50i-h700-anbernic-rg35xx-2024-ddr3.dtbo`). KERNEL and SYSTEM are byte for byte the same in both images
(`build.sh` checks that against the DDR4 pins and refuses otherwise). `make-image-family.sh --models "... rg40xx-h-ddr3"` downloads
and pins the DDR3 image by itself; `check-image-family.sh` checks the loader and extlinux.conf against `H700_DDR3_LOADER_MD5` and
`H700_DDR3_EXTLINUX_MD5`. The notes name DDR3 and say it is a test build.

**SYSTEM without a few files** (`H700_SYSTEM_REMOVE`; read again for this family on 2026-10-30 with
`scripts/check-release-tree.py` on the unpacked SYSTEM): the six SKick tables loose and in Amiberry's `boot-data.zip`
and the DOOM shareware data, as on the Mini V2. **DraStic stays** (`usr/config/drastic`, decision 2026-10-31): the closed-source Nintendo DS emulator by Exophase, passed on
unchanged exactly as ROCKNIX does, with its `LICENSE.pdf` in that folder and ROCKNIX's `licenses/DRASTIC.pdf` on the card.
The basis is a 2017 forum thread: Exophase first asked people not to distribute the Raspberry Pi beta; after RetroPie took
it out of its setup script he answered (24 May 2017) that "at this point there's no real harm keeping it in there, if
you don't have any misgivings doing so". That is a tolerance, not a formal licence, and it is about RetroPie. The card's
notes, `NOTICE.md` and the note to the ROCKNIX team say so plainly, credit Exophase, and promise to remove it on request.
There is no source to offer for it (the source bundle says so). `check-release-tree.py` flags its two
`drastic_bios_*.bin` by name: they are an open BSD replacement BIOS (Gilead Kutnick), so they have a reasoned entry in
`scripts/release-tree-allowlist.txt`; the check on the image expects exactly those two BIOS files. The H700
SYSTEM has no `rocknix_abl`, no AYN/Mangmi firmware and no GBA BIOS (so no ReGBA source in its bundle; the only BIOS files are DraStic's open pair). The rest of the finds are the libretro cores ROCKNIX ships (not ours to remove) and glmark2 `.3ds` models (3D models, not
ROMs). Firmware in this SYSTEM is 15 files: three Realtek Wi-Fi/Bluetooth files and the regulatory database (all
identical to linux-firmware 20260309 / wireless-regdb 2026.05.30, "Redistributable"), and ten display panel files
(`panels/*.panel`, 387 to 557 bytes) that are ROCKNIX's own, committed in the distribution repository (so they are in
its source archive); the repository gives them no separate licence and we could not trace where the screen command
values first came from. `deploy/image/firmware-licences-h700/README.txt` says this next to every file's md5. The card
also carries U-Boot and TF-A binaries (GPL, BSD) whose source is in the bundle.

**The GAMES drive (by model, `pins.sh` `<FAMILY>_GAMES_DRIVE_MODELS`: today the RG28XX and the RG35XX 2024 (ROCKNIX's tree has no Wi-Fi node for it), handhelds with no Wi-Fi chip and so
no file sharing).** ROCKNIX's layout is the standard on every image (RG35XX Plus and SP, the SM8550 models, the Mini V2):
STORAGE over the whole card, with ROCKNIX's fs-resize and none of the files below on the card; the add-on zips and anything
already installed are unchanged too. After the first start an RG28XX card of 16 GB or more has a third partition, exFAT, label `GAMES`, that a Mac or
Windows PC opens as a normal drive, with `roms/<system>/` ready. What and why:

- *Layout.* p1 ROCKNIX (FAT, 2 GiB), p2 STORAGE (ext4, **6 GiB**), p3 GAMES (MBR type 7, the rest, from a 4 MiB boundary).
  6 GiB: settings, covers and snaps (5,000 games' art is 2-3 GB), logs, and room for a system update's 1.5 GB tar in
  `/storage/.update`; saves and states go with the games. Cards under 12 GiB (an 8 GB card: 7.4) keep ROCKNIX's layout:
  STORAGE over the whole card. The image itself still has two partitions: the card's size is only known at the first start.
- *Who does it.* `deploy/image/games-first-start.sh`, on the FAT partition. `mount-storage.sh` binds it over
  `/usr/lib/rocknix/fs-resize` while `.please_resize_me` exists, so ROCKNIX's `fs-resize.target` starts it; SYSTEM stays
  pinned, and with the file missing or not runnable from `/flash` ROCKNIX's script runs as it always did. It uses what
  SYSTEM has (`parted`, `partprobe`, `e2fsck`, `resize2fs`, `tune2fs`, `mkfs.exfat`, `fsck.exfat`, `blkid`; there is no `sfdisk`).
- *A power cut.* The grow marker stays until the card is ready. Its removal is synced before deleting the split's ownership
  state, so an interrupted final cleanup cannot send a completed GAMES partition through stock resize. Each step looks at what's there first. A
  table with two partitions, then p2 grown, then p3 are each a single sector written. `.pocket-games-state` (`table`, then
  `formatting`), on STORAGE and written before the table changes, tells our p3 from one that was already there. mkfs only runs
  on an empty p3 or on one whose formatting this script began and that doesn't check out. `.pocket-games-tries` ends a
  loop after three tries. ROCKNIX's own script would lose the marker first and keep 32 MB for good.
- *Anything else that fails* goes to ROCKNIX's script (the bind is taken off with `umount -l`, it's the one case it runs):
  it grows p2 as far as it can and starts the card with its games in STORAGE. `/flash` is never written.
- *A card flashed again* over an old GAMES finds it behind the new table, at the same place: it stays, games included,
  if it's exFAT and labelled GAMES. A third partition that isn't ours is never touched.
- *Finding it.* ROCKNIX's `automount` (`find_games`) takes any unmounted ext4/btrfs/FAT/NTFS partition of a device over
  8 GiB, in `sort -r` order, whether on the boot card or not, so no `system.gamesdevice` is needed. A card in the second
  slot sorts first and wins: an existing games card works as before, and GAMES waits. It binds `games-external/roms` to
  `/storage/roms`, with no overlay on exFAT (`.ms_unsupported`; `system.merged.storage=0` is the default anyway).
- *Its mount.* The seed's drop-in `rocknix-automount.service.d/10-pocket-games.conf` runs `deploy/image/games-mount.sh` after
  automount, for this drive only: mounted again with `noatime` (exFAT ignores options on a remount; file sharing writes as
  root, like everything else on the Pocket), and first `fsck.exfat -p` when the drive's dirty flag is set. There's nothing to
  replay on exFAT after a power cut, so this stands in for ext4's journal. Costs nothing on a clean drive.
- *Saves on exFAT.* Checked on the real file system (Linux 6.8 exfat, 2026-10-31): rename over a file, fsync, chmod (a
  silent no-op) and writing as an unprivileged user work; the dirty flag is on the card from the first write until a sync, so a power
  cut does leave it set. Hard links, symlinks, chown and `:` `?` in names are refused, and pocket-ui copes: Dolphin's and
  Azahar's hard links fall back to a copy, their states move with `fsutil::move_file` across the file systems, screenshot
  names are cleaned (`screenshot::clean`), chown failures only log. `write_atomic` (what pocket-core's saves go through)
  is fsync, rename, directory fsync, all of which work here. Weaker than ext4: no journal, so what a standalone emulator
  hasn't flushed can go in a power cut. PortMaster ports with symlinks in their zips can't be unpacked there, as on any
  exFAT games card.
- *On screen.* The same dark text screen as ROCKNIX's script, with lines saying what's happening. It adds the format to
  that phase: the whole script took 0.18 s on a fake 64 GB card with ROCKNIX's own programs, an SD card takes seconds more.

Tests: `deploy/image/test-mount-storage.sh` (gate: the state machine with stand-ins, a power cut at every tool call, small
card, foreign partition, each failure); `scripts/image/check-h700.sh` (the files, and that SYSTEM still has every tool
and automount's filter). Run once by hand with SYSTEM's own aarch64 programs on a loop device: 64, 16 and 8 GB cards, `kill -9`
at 20 points (during `parted` and `resize2fs` too), mkfs failing, a card flashed over an old GAMES, and ROCKNIX's real
`automount` logic with GAMES, with a card in the second slot, and with one too small (the second card wins; under 8 GiB it's
ignored). Never seen on a device yet: the fs-resize.target run itself, and a Mac or Windows PC opening the drive.

**Pins.** The `H700_` block of `pins.sh` holds the image hashes, KERNEL and SYSTEM md5, the model list and
`H700_POCKET_SYSTEM_MD5` (reproduced by two strip runs before it was pinned).

**What a card does that nobody has seen yet:** U-Boot reads `extlinux.conf`, starts KERNEL with `dtb.img`; ROCKNIX's
initramfs sources our two hooks; the first start splits the card and moves the seed in (`.please_resize_me` is in the
base, as on the Mini V2). Updates: ROCKNIX's `bootloader/update.sh` on the H700 rewrites `dtb.img` and the card's
U-Boot; the guard in `mount-storage.sh` keeps foreign updates out, and our own OS update is stage 2 of
`DISTRO-PLAN.md` (not built).

## SM8550: AYN Odin 2 family, Retroid Pocket 6 and Nova (test images, never booted)

Status 2026-10-31: built and checked offline; no card has been started on a handheld. Same builder and check as the H700
(`scripts/make-image-family.sh --family SM8550`, `scripts/check-image-family.sh`); a family is a block of pins plus its notes.

**Boot path.** The official image holds only `KERNEL` (an Android boot image with every model's device tree and the
command line `boot=LABEL=ROCKNIX disk=LABEL=STORAGE`), `SYSTEM` and `rocknix_abl/` on a GPT card with the same offsets as the
Mini V2's. So there is no GRUB and no `dtb.img`, and the five images differ only in their name and notes.

**What a player needs first.** ROCKNIX's ABL must be in the handheld: flashed once from Android with root
(`rocknix_abl/README` of the official image: `backup_abl.sh`, `flash_abl.sh`, then Volume Down at start). Its menu then wants
"Set device model" (the ABL's own text: "Device model not set, can't boot Linux") and "Switch boot mode"; it picks the tree in
KERNEL by that model. We never flash a bootloader and don't carry the ABL. The first-start notes say so plainly. Our SYSTEM goes without
`usr/share/bootloader/rocknix_abl`, so ROCKNIX's `update.sh` (which runs `updateabl`) has nothing to flash, on top of the
guard in `mount-storage.sh`.

**A ROCKNIX install on internal storage wins over the card** (`usr/bin/installtointernal`: partitions labelled `ROCKNIX` and
`STORAGE` on the UFS). `boot=LABEL=ROCKNIX disk=LABEL=STORAGE` then finds the internal ones. Read from the project's Odin 2 Mini
on 2026-10-31: `/flash` is `sda18`, `/storage` is `sda19`, and the card's own `ROCKNIX` and `STORAGE` partitions are ignored
except as games card. Such a unit can't test this image by swapping cards, and such a player needs the install package.

**SYSTEM without** (`SM8550_SYSTEM_REMOVE`, read with `check-release-tree.py` on the unpacked SYSTEM): the SKick tables, the
DOOM shareware data, `rocknix_abl`, and the firmware of devices we don't list (AYANEO Pocket family, AYN Thor). DraStic stays
as on the H700. The only other finds are the libretro cores, ReGBA's open GBA BIOS and glmark2 models.

**Open before publishing: firmware without a stated licence.** The Odin's sound and battery firmware (`qcom/sm8550/ayn`) and a
few more files come from ROCKNIX's `extra-firmware` repository, which has no licence file. On the Mini V2 such files could
simply go; these handhelds can't work without them. `deploy/image/firmware-licences-sm8550/README.txt` lists each file. The
check also proves every firmware the model's device tree names is still in SYSTEM. The source needs no bundle: the offer
points to ROCKNIX's repository at the tag (see the GPL source section).


## Offline check (`scripts/check-image.sh`)

Runs on the `.img.xz` (unpacks it into `build/image/check/`) or a raw `.img`. 58 checks until 2026-10-26, 63 with the
boot screen (the kernel line's quiet words, no `echo` or `fbcon=`, the boot screen's unit, pocket-splash an aarch64
program answering `--version`; [BOOT-SPLASH.md](BOOT-SPLASH.md)), 64 since 2026-10-27 (see the GRUB point below). For
0.1.0 (2026-10-27, built from main `c3a771f` plus these script fixes, on the build computer) every check passed except
the three `--version` ones, which warned because the virtual device's base image was not built there
(`scripts/vdev.sh image`); on 2026-10-26 both programs printed `0.1.0` in that base image on the Mac instead:

- SHA256SUMS; GPT with exactly ROCKNIX's two partitions (start, size, type, name); image size unchanged.
- FAT: label `ROCKNIX`; **exactly** the expected file list (ROCKNIX's licence folder counted, `ROCKNIX_LICENSE_FILES`);
  KERNEL and SYSTEM md5 against the pins and their `.md5` files.
- Inside SYSTEM (since 2026-10-28): none of `SYSTEM_REMOVE`; `ssh.enabled=0`, `samba.enabled=0`,
  `syncthing.enabled=0` in ROCKNIX's default `system.cfg`; no `authorized_keys` or host keys; no `ssh` on the kernel
  command line; the only BIOS-named file is ReGBA's (md5 pinned); every firmware file is listed in
  `firmware-licences/README.txt`.
- Notes: what was removed and why, the ReGBA BIOS, remote access off, the pointer to `SOURCE-OFFER.txt`; the offer
  itself (three years, how to ask, ROCKNIX commit, version); `firmware-licences/` is the repository's; a warning if the
  pocket-ui source named is a `-dirty` commit.
- GRUB: `grub.cfg` is `deploy/sd-efi/grub.cfg`, searches `--label ROCKNIX`, loads `/boot/pocket/Image` and
  `/boot/pocket/miniv2-noufs.dtb` with `boot=LABEL=ROCKNIX disk=LABEL=STORAGE`, the quiet start
  (`rootwait quiet loglevel=3 vt.global_cursor_default=0`, no `echo` command, no `fbcon=` outside the comments: the
  file's own comment names `fbcon=rotate` as a warning, which the check refused until 2026-10-27), that
  `deploy/grub-quiet.sh` (what `scripts/boot-quiet.sh` runs on a card) finds it already quiet and would change nothing
  (`scripts/image/grub-checks.sh`, test `scripts/image/test-grub-checks.sh`, part of the gate), and every file it names
  exists; `BOOTAA64.EFI` is an arm64 EFI application (and the device's, else only a warning).
- Kernel: `Image` md5 is the device's and it is re-unpacked from this KERNEL for comparison; dtb md5 is the
  device's, its model is "Retroid Pocket Mini V2", `ufshc@1d84000` and `phy@1d87000` are disabled;
  `KERNEL.source.md5` names this KERNEL.
- Hooks are the files in `deploy/image/`; the notes name the version, "based on ROCKNIX" with link, tag and commit,
  CC BY-NC-SA 4.0, non-commercial, no BIOS files.
- STORAGE: label `STORAGE`, `e2fsck -fn` clean, **exactly** the expected files with owner `0:0` and mode;
  `.please_resize_me` present, no `.config`/`.cache`; drop-in, boot screen unit, launcher and `portmaster-setup.sh` are
  the repository's; the three programs are aarch64 ELF; a failure when fewer than `P2_MIN_HEADROOM_BLOCKS` (128 blocks of
  4 KiB = 512 KiB) are free before the first start (see "STORAGE room").
- Nothing from a developer device: no `.config/pocket-ui` (so the first-start setup runs), no `dev-ssh`, no `.ssh`
  or host keys, no Wi-Fi files (`*.psk`, iwd, NetworkManager, wpa_supplicant), no recents, saves, ROMs or BIOS
  files, no `update-pending`, `previous` or `dev.env`.
- `pocket-ui --version` and `pocket-core --version` print the image's version (run in the virtual device's base
  image, which has ROCKNIX's libraries).

Against the unmodified ROCKNIX image the same script reports 29 failures.

## Licensing (not legal advice)

The notes (`pocket-ui-info/README.txt` on the card, and `pocket-ui-<version>-rp-mini-v2.txt` next to the image) say:

- The image is **based on ROCKNIX**, links rocknix.org and the GitHub repository, credits JELOS, says pocket-ui is not
  made or endorsed by the ROCKNIX team, and lists what was changed. The ROCKNIX logo is not shown at boot
  (`post-flash.sh`); ROCKNIX artwork inside SYSTEM (logos, EmulationStation themes) stays unchanged and is
  attributed under **CC BY-NC-SA 4.0** with a link. EmulationStation, ROCKNIX's own interface, still takes over if
  pocket-ui cannot start.
- **Free, non-commercial distribution only**: ROCKNIX "includes components licensed for non-commercial use only"
  (libretro cores such as snes9x, genesis_plus_gx, picodrive, fbneo) and its artwork is NC.
- **pocket-ui and pocket-core: GNU GPL version 2 or later.** `LICENSE`, `NOTICE.md` and `licences/` (the crates' texts,
  `THIRD-PARTY-CRATES.md`, the fonts' OFL, from `scripts/licence-files.sh`) go to `pocket-ui-info/`, `/storage/pocket-ui/licences`
  (Settings → About) is a link to that folder: STORAGE has no second copy (it has no room for one, see "STORAGE room").
- **ROCKNIX's licences:** its `LICENSE.md` and the 75 texts of its `licenses/` folder at the tag (none of them is
  inside SYSTEM) go to `pocket-ui-info/rocknix-licenses/`.
- **Firmware:** `pocket-ui-info/firmware-licences/` (see "SYSTEM without a few files").
- **No BIOS files and no ROMs** of our own; the one BIOS file in ROCKNIX's SYSTEM is ReGBA's GPL replacement.

### GPL source: pocket-ui's tarball, pointers and a written offer

Chosen route (changed 2026-11-02, before 0.1.12): **no big source bundles are uploaded.** ROCKNIX, Knulli
and pocknix publish no source archives with their releases either. Next to the image stays only
`pocket-ui-<version>-source.tar.gz` (the `git archive` of the public release commit, which GitHub also gives for every
tag: pocket-ui, pocket-core, pocket-splash and the scripts that build the image, `scripts/make-image*.sh`,
`scripts/image/`, `deploy/`). Everything else is a pointer in `pocket-ui-info/SOURCE-OFFER.txt` and the notes: ROCKNIX's
repository at the exact tag and commit (every package's recipe, the kernel configuration, all patches), Debian's snapshot
page of the GRUB source (Mini V2), and ReGBA's `bios/` at its commit. The card carries a **written offer** (GPLv2 §3(b),
GPLv3 §6(b)): for at least three years from the image's release, anyone can get the complete corresponding source of the
GPL and LGPL software on the card, on request through the repository's issues, for no charge beyond the cost of providing
it. The licence texts on the card stay as they are.

The scripts that made the bundles stay in the repository for the day an offer is taken up, or a bundle is wanted again:

```sh
scripts/image/rocknix-source.sh ~/pocket-review/rocknix-source-20261001      # once per ROCKNIX tag
scripts/image/source-bundle.sh OUT --version 0.1.1 --rocknix-source ~/pocket-review/rocknix-source-20261001 \
  --pocket-ui ../public-011 <public release commit>
```

They write `rocknix-<tag>-distribution.tar.xz`, `rocknix-<tag>-<family>-sources.tar.xz.part-NN`, `packages.txt`,
`regba-bios-source.tar.gz`, the GRUB `.dsc` with its tarballs and `SOURCE-SHA256SUMS`. The first run (2026-10-28) fetched 654
packages in about 85 minutes, 34 GB as fetched, packed without `.git` and xz-compressed. Packages whose source
`scripts/get` cannot fetch end in `failed.txt`; `source-bundle.sh` refuses to run unless each one is in its `NOT_ON_CARD`
list with a reason (today only `rocknix-abl`, which is removed from SYSTEM). Files a package downloads during its own build
(`portmaster`, `mangohud`, `xemu-sa`, `wine`'s `winetricks`, data files without code) are not in such a bundle; with the
offer asked for, they would be fetched then. The existing bundles stay where they are.

Open points:

- The ROCKNIX team has not been asked; a friendly note is drafted next to the release files.
- The written offer names GitHub issues as the way to ask. A postal or e-mail contact would make it independent of
  GitHub.

## Unverified

- Only one start on a device so far (2026-10-27, one fresh card, one Mini V2). The kernel, device tree and GRUB are the
  device's files byte for byte, so the boot chain itself is as verified as in INSTALL-DUAL-BOOT.md; new are the
  first-start sequence (resize, the two hooks) and a card written by Etcher or Imager. That start is the only
  evidence for them, and it ended in the Wi-Fi bug above, so the run to the first game is not proven.
- That ROCKNIX's `fs-resize` grows a card written from this image as it did on the author's card (its log on the
  device shows the same steps for the stock image). The partition table is the stock one.
- That other Mini V2 units have u-boot in `loader` from the factory (the author's has Retroid's 2024.10-rc4 build).
- That "Switch boot mode" is offered on a device with a locked bootloader (only seen on an unlocked one).
- That writing with balenaEtcher or Raspberry Pi Imager directly from `.img.xz` works for this file (both document
  `.xz` support).
- That builds on other machines give the same bytes. Two builds of the same commit here did (identical
  sha256 of the `.img.xz`); a build on x86_64 Linux gives the same GRUB, kernel and device tree as the Mac
  (byte for byte, 2026-10-26), the whole image was not compared across the two: added files carry the commit's time stamp, and mtools and e2fsprogs take their clock
  from `SOURCE_DATE_EPOCH` and `E2FSPROGS_FAKE_TIME`. GRUB comes from Debian's current package, so a newer
  Debian GRUB changes `BOOTAA64.EFI` (the check then warns).

## Test on a real card (needs a spare microSD and the owner's go)

The author's device keeps its current card; this test uses a second card, so nothing on the device changes except
which card is in it.

1. Build: `scripts/make-image.sh` (after the commit, so the notes name a clean commit), then
   `scripts/check-image.sh build/image/pocket-ui-<version>-rp-mini-v2.img.xz` must end in `IMAGE OK`.
2. Write `build/image/pocket-ui-<version>-rp-mini-v2.img.xz` to the spare card (8 GB or more) with balenaEtcher or
   Raspberry Pi Imager. Let the tool verify the write.
3. Optional, on the Mac: the card's FAT partition shows `pocket-ui-info/README.txt`, `EFI/`, `boot/`, no
   `rocknix_abl/`. Eject.
4. Switch the device off, swap the cards. The boot mode is already Loader on the author's device.
5. Switch on and watch:
   - No ROCKNIX logo and no kernel text (quiet start); "pocket-ui, based on ROCKNIX" and the version line, then the
     resize messages, then a restart.
   - The second start shows "Initializing, please wait...", then the boot screen (the spine, "Getting ready for the
     first start…"), then pocket-ui's first-start setup (Welcome) fading in, not EmulationStation. Note the time of both
     starts.
   - The third start (step 8) shows the boot screen without the line.
6. Run through the setup: a look, Wi-Fi with the on-screen keyboard, extras.
7. Then, with SSH switched on in Settings → Connections → Remote access (read only):
   - `df -h /storage` shows the card's full size.
   - `ls /flash`: `mount-storage.sh` and `post-flash.sh` still there (both run at every start). `cat /flash/fs-resize.log`.
   - `grep -E "^(updates.enabled|system.hostname)=" /storage/.config/system/configs/system.cfg` says `0` and
     the model's name (`pocket-mini-v2`, `pocket-rg35xx-plus`, ...); `hostname` says the same; `/storage/.config/autostart/pocket-ui-image-defaults` is gone.
   - Settings → Help & tools → About, System row: "pocket-ui OS <version> · built on ROCKNIX 20261001 (SM8250)".
   - `ls -la /storage/.config/system.d/essway.service.d/` holds `10-pocket-ui.conf`; `/storage/.config/system.d/`
     holds `pocket-splash.service` and `sysinit.target.wants/pocket-splash.service`; `/storage/.pocket-seed` is gone.
   - `journalctl -b -u pocket-splash`: its last line says it ended because sway started.
   - `systemctl show -p DropInPaths essway` names the drop-in; `readlink /storage/pocket-ui/current` is
     `releases/<version>`.
   - `/proc/cmdline` starts with `BOOT_IMAGE=/boot/pocket/Image`; no `sd*` devices in `/proc/partitions`.
8. Restart once more: pocket-ui comes up directly, with the settings kept.
9. Back to Android: Vol Down + Power → Switch boot mode → Android → Start. Android starts with the card in.
10. Swap the original card back and set Boot mode: Loader again.

## STORAGE room

ROCKNIX's STORAGE is a 32 MiB ext4 (8192 blocks of 4 KiB, a 4 MiB journal, 7027 blocks free; the same on the SM8250, SM8550
and H700 base images, `pins.sh` `P2_FREE_BLOCKS`) and stays that size until the first start grows it to the card. The three
programs are about 27.6 MB of it and grow with every release (0.3.0 to 0.3.1: 67 blocks). On 2026-11-08 the 0.3.1 images
stopped in `build.sh` ("Could not allocate block"): with the licence texts stored a second time (303 blocks) the programs were
43 blocks short. Since then the licence texts are on the card once, in `pocket-ui-info/licences` on the FAT partition, and
`/storage/pocket-ui/licences` is a link to `/flash/pocket-ui-info/licences`; an update (`install.sh`) replaces the link with a
real folder on the grown STORAGE. The journal cannot be made smaller (4 MiB is the least ext4 takes at 4 KiB blocks), and
ROCKNIX keeps it when it grows the file system, so it is left as it is.

Three things keep it from happening again: `build.sh` stops when ROCKNIX's free block count is not the pinned one;
`check.sh` and `check-h700.sh` fail an image with fewer than `P2_MIN_HEADROOM_BLOCKS` (128 blocks, about two releases) free;
and `scripts/image/storage-budget.sh BIN_DIR` works it out from the release programs without building an image: the release
check runs it (`POCKET_STORAGE_BIN_DIR`, or a build in `target/aarch64-unknown-linux-gnu*/release`) and fails when the
programs no longer fit or leave too little. With the link the 0.3.1 programs leave about 1 MiB.

For future release programs, `storage-budget.sh` now requires 2 MiB free (or a larger pinned image minimum).
Full LTO recovers 1.95 MiB in the measured successor build, leaving a conservative 730 blocks (2.85 MiB)
without changing the partition layout. The historical finished-image/receipt minimum above stays unchanged,
so existing immutable 0.3.1 image attestations remain valid. See [BINARY-SIZE.md](BINARY-SIZE.md) for attribution,
alternatives and runtime measurements.
