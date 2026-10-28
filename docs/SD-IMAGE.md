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
sh deploy/image/test-hooks.sh         # tests of the two initramfs hooks
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
[IMAGE-AND-BOOT.md](IMAGE-AND-BOOT.md) part B (repacking SYSTEM, or a ROCKNIX fork) cost days and would replace
the kernel and system that are verified on the device. This way the kernel, the system and the boot files are
byte for byte the ones that boot on the author's device today.

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

### What changes on the card

| Partition | Added | Removed |
|---|---|---|
| FAT, label `ROCKNIX` (`/flash`) | `EFI/BOOT/BOOTAA64.EFI`, `boot/grub/grub.cfg`, `boot/grub/grub-efi-arm64-bin.version`, `boot/pocket/{Image,miniv2-noufs.dtb,KERNEL.source.md5}`, `post-flash.sh`, `mount-storage.sh`, `pocket-ui-info/` (notes and licences) | `rocknix_abl/`: ROCKNIX's replacement for Retroid's bootloader and the scripts that flash it. This boot path never uses it, and flashing it is what this setup avoids |
| STORAGE, ext4 | `pocket-ui/` (launcher, `releases/<version>/` with pocket-ui, pocket-core and pocket-splash, `current` and the three links, as `dev.sh` and the updater lay it out), `.pocket-seed/.config/system.d/`: `essway.service.d/10-pocket-ui.conf`, and the boot screen `pocket-splash.service` with its link `sysinit.target.wants/pocket-splash.service` ([BOOT-SPLASH.md](BOOT-SPLASH.md)) | nothing (`.please_resize_me` and `lost+found` stay) |

Partition table, partition sizes, KERNEL and SYSTEM are unchanged. Image: 2,198,863,872 bytes raw, 1,518,697,476
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
  `/storage/.pocket-seed/.` into `/storage` (`cp -a`), removes the seed, and deletes itself from `/flash`. This
  happens in the initramfs, before systemd starts, so the drop-in is there when systemd reads its units, and
  pocket-ui comes up at the very first normal start. From the third boot on, ROCKNIX mounts `/storage` itself.
- `deploy/image/post-flash.sh` replaces `init`'s `load_splash` with the same function minus the call to
  `/usr/bin/rocknix-splash`, which draws the ROCKNIX logo. It prints "pocket-ui, based on ROCKNIX" and ROCKNIX's
  version line instead. It stays on the card and runs at every boot.
- Both run in the initramfs's BusyBox (1.36.1: `cp -a`, `rm`, `mount`, `sync` are there; checked against the
  initramfs binary). `deploy/image/test-hooks.sh` runs them in the virtual device's base image with fake
  `mount_part`/`mount`: first boot, second boot, later boots, a hook that could not delete itself, existing files
  in `.config`, and the splash. [V] Without the hooks 10 of its 21 checks fail.

So a new card goes: **boot 1** grows STORAGE and restarts; **boot 2** puts the drop-in and the boot screen's unit in
place (`cp -a` keeps its link), the boot screen shows "Getting ready for the first start…", ROCKNIX fills
`/storage/.config`, and the essway unit starts pocket-ui, which finds no `settings.json` and opens its first-start
setup. [I] for the whole sequence on a device; each part is [V] as above.

## Offline check (`scripts/check-image.sh`)

Runs on the `.img.xz` (unpacks it into `build/image/check/`) or a raw `.img`. 58 checks until 2026-10-26, 63 with the
boot screen (the kernel line's quiet words, no `echo` or `fbcon=`, the boot screen's unit, pocket-splash an aarch64
program answering `--version`; [BOOT-SPLASH.md](BOOT-SPLASH.md)), 64 since 2026-10-27 (see the GRUB point below). For
0.1.0 (2026-10-27, built from main `c3a771f` plus these script fixes, on the build computer) every check passed except
the three `--version` ones, which warned because the virtual device's base image was not built there
(`scripts/vdev.sh image`); on 2026-10-26 both programs printed `0.1.0` in that base image on the Mac instead:

- SHA256SUMS; GPT with exactly ROCKNIX's two partitions (start, size, type, name); image size unchanged.
- FAT: label `ROCKNIX`; **exactly** the expected file list; KERNEL and SYSTEM md5 against the pins and their
  `.md5` files.
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
- **pocket-ui and pocket-core: GNU GPL version 2 or later.** `make-image.sh` copies the repository's `LICENSE` and
  `NOTICE.md` to `pocket-ui-info/` on the card; third-party credits (fonts, Rust crates) are in `NOTICE.md`, not
  repeated in the notes. The licence texts that have to travel with the programs (the Rust crates' from
  `scripts/third-party.sh --texts`, `THIRD-PARTY-CRATES.md`, the fonts' `assets/fonts/OFL.txt`) are collected by
  `scripts/licence-files.sh` into `pocket-ui-info/licences/` on the card and `/storage/pocket-ui/licences` in STORAGE. `check-image.sh` compares both with the repository's files, and only warns while the
  repository has none (the licence files arrive from another branch): **an image built before they reach main is
  not releasable.**
- **GPL source:** ROCKNIX tag `20261001`, commit `c445081a59518f37d9776e5412dd7b14910696f7` (kernel and patches
  included; `make src-pkg` packs every source archive); GRUB's Debian source package at snapshot.debian.org;
  pocket-ui, pocket-core and this build script at the pocket-ui commit the image was built from.
- **No BIOS files and no ROMs** (checked, see above).

Open points:

- **The repository is not public yet.** Now that pocket-ui itself is
  GPL, its source has to be available to everyone who gets the image: publish the repository (or the source of
  the exact commit) before the image is shared.
- For a public release, consider hosting a copy of the corresponding source (`make src-pkg` of the tag, the GRUB
  source package, pocket-ui's commit) next to the image instead of only linking to it.

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
   - `ls /flash`: no `mount-storage.sh` any more; `post-flash.sh` still there. `cat /flash/fs-resize.log`.
   - `ls -la /storage/.config/system.d/essway.service.d/` holds `10-pocket-ui.conf`; `/storage/.config/system.d/`
     holds `pocket-splash.service` and `sysinit.target.wants/pocket-splash.service`; `/storage/.pocket-seed` is gone.
   - `journalctl -b -u pocket-splash`: its last line says it ended because sway started.
   - `systemctl show -p DropInPaths essway` names the drop-in; `readlink /storage/pocket-ui/current` is
     `releases/<version>`.
   - `/proc/cmdline` starts with `BOOT_IMAGE=/boot/pocket/Image`; no `sd*` devices in `/proc/partitions`.
8. Restart once more: pocket-ui comes up directly, with the settings kept.
9. Back to Android: Vol Down + Power → Switch boot mode → Android → Start. Android starts with the card in.
10. Swap the original card back and set Boot mode: Loader again.
