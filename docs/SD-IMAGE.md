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
   and the source archives are; for the public image: the public repository's release commit and release page.

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
fit "free, non-commercial, unchanged, credited". DraStic, whose author asked people not to redistribute it, is **not**
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
  `updates.enabled=0` and `system.hostname=pocket-ui` with ROCKNIX's `set_setting` and deletes itself. (A seeded
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
  first 0.1.7 hook fails the 3 checks of Sol's review findings 2 and 3. `deploy/image/test-hooks.sh` runs
  `post-flash.sh` in the virtual device's base image (Docker).

So a new card goes: **boot 1** grows STORAGE and restarts; **boot 2** puts the drop-in and the boot screen's unit in
place (`cp -a` keeps its link), the boot screen shows "Getting ready for the first start…", ROCKNIX fills
`/storage/.config`, and the essway unit starts pocket-ui, which finds no `settings.json` and opens its first-start
setup. [I] for the whole sequence on a device; each part is [V] as above.

## H700: RG35XX Plus, RG35XX SP, RG28XX (prepared, never booted)

Status 2026-10-30: the builder and the offline check exist and pass; **no image has been started on a handheld.**
Built on an x86_64 Linux machine (3.6 GB of images and 7.5 GB of source are too much to move around). Nothing is published.

```sh
scripts/make-image-h700.sh --version V           # build/image-h700/pocket-ui-V-{rg35xx-plus,rg35xx-sp,rg28xx}.img.xz, .txt, SHA256SUMS
scripts/check-image-h700.sh build/image-h700/pocket-ui-V-rg35xx-sp.img.xz
scripts/image/rocknix-source.sh OUT --device H700   # the source bundle of the H700 build (source-bundle.sh --device H700)
sh scripts/image/test-h700-pins.sh                   # pins, notes and model list (part of the gate)
```

**What differs from the Mini V2 image.** ROCKNIX's H700 image boots through its own U-Boot (written 8 KiB into the card,
before the first partition) and `extlinux/extlinux.conf` (`LINUX /KERNEL`, `FDT /dtb.img`). So there is no GRUB, no
unpacked kernel and no edited device tree: the image is the official ROCKNIX H700 DDR4 image, SYSTEM repacked without
the removed files, our two initramfs hooks (`post-flash.sh`, `mount-storage.sh`: the same files as for the Mini V2; the
H700 initramfs has the same hook points and the same `load_splash`, checked in the unpacked KERNEL), the notes, and
pocket-ui on STORAGE. The partition table is MBR (the Mini V2's is GPT) with the same offsets.

**One image per model, one file apart.** ROCKNIX's install page has the player copy the model's tree from
`device_trees/` to the FAT root as `dtb.img`. We do it at build time (`pins.sh` `H700_MODELS`: RG35XX Plus
`sun50i-h700-anbernic-rg35xx-plus.dtb`, RG35XX SP `...-rg35xx-sp.dtb`, RG28XX `...-rg28xx.dtb`, each with its md5).
The three images differ in `dtb.img` and in the model name in the notes, nothing else. The newer screen revisions
(`rg35xx-plus-rev6-panel`, `rg35xx-sp-v2-panel`) have their own trees and are not built; a unit with a broken picture
on the plain one needs that variant (one more entry in `H700_MODELS`).

**RAM type.** The base is the DDR4 image: U-Boot for LPDDR4 units. The RG35XX Plus reads `vdd-dram` 1.1 V (DDR4). The
RG35XX SP and the RG28XX are **expected** to be DDR4 (ROCKNIX's device pages), not confirmed on the units; a DDR3 unit
would not start (the card is not harmed). If the SP or the RG28XX turns out to be DDR3, the DDR3 image has other
hashes (`ROCKNIX-H700...-DDR3.img.gz.sha256`) and its own `H700_` pins.

**SYSTEM without a few files** (`H700_SYSTEM_REMOVE`; read again for this family on 2026-10-30 with
`scripts/check-release-tree.py` on the unpacked SYSTEM): the six SKick tables loose and in Amiberry's `boot-data.zip`
and the DOOM shareware data, as on the Mini V2, and **the DraStic emulator** (`usr/config/drastic`: a closed-source
binary by Exophase with its databases and the two `drastic_bios_*.bin`; `check-release-tree.py` flagged the BIOS names,
reading the folder showed the rest). ROCKNIX's only permission is a 2017 forum thread, kept as `LICENSE.pdf` in that
folder, in which Exophase says he asked people not to distribute the Raspberry Pi beta and that free use there is up to
his discretion: that is no licence to pass it on, so it is out (the BIOS files were a BSD-licensed open replacement and
would have been fine alone). ES still lists DraStic as a DS emulator; the entry points at a missing program. The H700
SYSTEM has no `rocknix_abl`, no AYN/Mangmi firmware and no GBA BIOS (so no ReGBA source in its bundle; with DraStic out it holds no BIOS file at all). The rest of the finds are the libretro cores ROCKNIX ships (not ours to remove) and glmark2 `.3ds` models (3D models, not
ROMs). Firmware in this SYSTEM is 15 files: three Realtek Wi-Fi/Bluetooth files and the regulatory database (all
identical to linux-firmware 20260309 / wireless-regdb 2026.05.30, "Redistributable"), and ten display panel files
(`panels/*.panel`, 387 to 557 bytes) that are ROCKNIX's own, committed in the distribution repository (so they are in
its source archive); the repository gives them no separate licence and we could not trace where the screen command
values first came from. `deploy/image/firmware-licences-h700/README.txt` says this next to every file's md5. The card
also carries U-Boot and TF-A binaries (GPL, BSD) whose source is in the bundle.

**Pins.** The `H700_` block of `pins.sh` holds the image hashes, KERNEL and SYSTEM md5, the model list and
`H700_POCKET_SYSTEM_MD5` (reproduced by two strip runs before it was pinned).

**What a card does that nobody has seen yet:** U-Boot reads `extlinux.conf`, starts KERNEL with `dtb.img`; ROCKNIX's
initramfs sources our two hooks; the first start grows STORAGE and moves the seed in (`.please_resize_me` is in the
base, as on the Mini V2). Updates: ROCKNIX's `bootloader/update.sh` on the H700 rewrites `dtb.img` and the card's
U-Boot; the guard in `mount-storage.sh` keeps foreign updates out, and our own OS update is stage 2 of
`DISTRO-PLAN.md` (not built).

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
  the repository's; the three programs are aarch64 ELF; a warning when STORAGE is more than 24 MB full before the first start (the programs
  have to fit into its 32 MB).
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
  `THIRD-PARTY-CRATES.md`, the fonts' OFL, from `scripts/licence-files.sh`) go to `pocket-ui-info/`, the texts also to
  `/storage/pocket-ui/licences` (Settings → About).
- **ROCKNIX's licences:** its `LICENSE.md` and the 75 texts of its `licenses/` folder at the tag (none of them is
  inside SYSTEM) go to `pocket-ui-info/rocknix-licenses/`.
- **Firmware:** `pocket-ui-info/firmware-licences/` (see "SYSTEM without a few files").
- **No BIOS files and no ROMs** of our own; the one BIOS file in ROCKNIX's SYSTEM is ReGBA's GPL replacement.

### GPL source: hosted next to the image, plus a written offer

Chosen route (2026-10-28): **we host the complete corresponding source ourselves**, as release assets next to the
image, and the card carries a **written offer** (GPLv2 §3(b), GPLv3 §6(b)) as a second way, valid three years, asked
for through the repository's issues. `pocket-ui-info/SOURCE-OFFER.txt` lists the files:

| File | Made by | What |
|---|---|---|
| `pocket-ui-<version>-source.tar.gz` | `scripts/image/source-bundle.sh` (`git archive` of the public release commit) | pocket-ui, pocket-core, pocket-splash and the image scripts |
| `rocknix-20261001-distribution.tar.xz` | `scripts/image/rocknix-source.sh` | ROCKNIX's build tree at the tag: package recipes, kernel config, all patches |
| `rocknix-20261001-SM8250-sources.tar.xz.part-NN`, `packages.txt` | `scripts/image/rocknix-source.sh` | every source archive and checkout (without git history) of the SM8250 build: the package list of ROCKNIX's own `scripts/genbuildplan.py --build image` for `ARCH=aarch64` and `ARCH=arm` (the Makefile's `SM8250` target builds both), fetched with ROCKNIX's `scripts/get` (the input of `make src-pkg`, without building anything) |
| `regba-bios-source.tar.gz` | `source-bundle.sh` | ReGBA's `bios/` at `7f37905`, checked against the BIOS md5 |
| `grub2_2.12-9+deb13u2.dsc`, `.orig.tar.xz`, `.debian.tar.xz` | `source-bundle.sh` | Debian's GRUB source, checked against the `.dsc` |
| `SOURCE-SHA256SUMS` | `source-bundle.sh` | |

```sh
scripts/image/rocknix-source.sh ~/pocket-review/rocknix-source-20261001      # once per ROCKNIX tag
scripts/image/source-bundle.sh OUT --version 0.1.1 --rocknix-source ~/pocket-review/rocknix-source-20261001 \
  --pocket-ui ../public-011 <public release commit>
```

First run (2026-10-28, on the build machine): 654 packages, about 85 minutes to fetch, 34 GB as fetched (git checkouts with their
history); packed without `.git` and xz-compressed. Packages whose source `scripts/get` cannot fetch end in
`failed.txt`; `source-bundle.sh` refuses to run unless each one is in its `NOT_ON_CARD` list with a reason (today only
`rocknix-abl`, whose release asset changed upstream and which is removed from SYSTEM). Not covered by `scripts/get`:
files a package downloads during its own build (in SYSTEM: the ReGBA BIOS, whose source is added separately, and the
DOOM shareware, which is removed). Others found by `grep -E '(curl|wget) '` over the packages of the SM8250 plan, not in
the bundle yet (open point; the written offer covers them): `portmaster` (`PortMaster.zip`, `compat.tar.gz`),
`mangohud` and `xemu-sa` (meson subproject tarballs), `wine` (`winetricks`, a script), and data files without code
(`ppsspp-sa` cheat database, `rpcs3-sa` config database, `armsx2-sa` patches).

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
     `pocket-ui`; `hostname` says `pocket-ui`; `/storage/.config/autostart/pocket-ui-image-defaults` is gone.
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
