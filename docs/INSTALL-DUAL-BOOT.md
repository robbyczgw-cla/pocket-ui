# Installing pocket-ui next to Android (Retroid Pocket Mini V2)

Status: 2026-10-28. **The SD card image** (below) builds and passes every offline check, and was started on a fresh
card on a Mini V2 on 2026-10-27, which found a Wi-Fi first-start bug (fixed since; see [SD-IMAGE.md](SD-IMAGE.md)). A
complete first start as a new player has not been repeated with the fixed build. The **install package** for a card
that already runs ROCKNIX ([below](#install-on-a-rocknix-card-you-already-use)) passed its tests and was run on a real
AYN Odin 2 Mini on 2026-10-27 (an experimental device). On a Mini V2 it has not been run. The **manual path** (appendix) has been run on the
author's device, phases 1 to 6; nothing had to be flashed there, because the `loader` partitions already held
Retroid's u-boot. Phase 7 is partly planned.

**Goal:** a setting in Retroid's own bootloader menu picks what starts: **Boot mode: Android** starts Android as
before, **Boot mode: Loader** starts pocket-ui from the SD card. Android and its partitions stay unchanged, and
nothing is flashed.

[V] marks something checked (a file, a source, a measurement). [I] marks something inferred that a phase
confirms on the device.

## Install with the SD card image

You need:

- A Retroid Pocket Mini V2.
- A microSD card of 8 GB or more. Everything on it is erased.
- A computer with [balenaEtcher](https://etcher.balena.io) or [Raspberry Pi Imager](https://www.raspberrypi.com/software/).
- The image, `pocket-ui-<version>-rp-mini-v2.img.xz`. It is not published yet; until then it is built with
  `scripts/make-image.sh` ([SD-IMAGE.md](SD-IMAGE.md)). Check it against `SHA256SUMS` next to it.

No SSH, no `fastboot` on a computer, no unlocked bootloader. [I] for the bootloader: the boot mode is a menu
entry in Retroid's own fastboot menu, seen so far only on an unlocked device.

1. **Write the card.** Open balenaEtcher or Raspberry Pi Imager (in Imager: "Use custom"), choose the `.img.xz`
   file (both read `.xz` directly), choose the card, write. Let the tool verify the write.
2. **Put the card into the switched-off device.**
3. **Set the boot mode, once.** Hold **Vol Down** and press **Power**. Retroid's fastboot menu opens; its bottom
   line reads "Boot mode: Android". Pick **Switch boot mode** with the volume keys and press Power until the line
   reads **Boot mode: Loader**, then pick **Start**. [V] on the author's device.
   **Vol Up + Power does not work** on this firmware (`RPMiniV2_V1.0.0.275_20260407`): Android starts. [V] Use the
   boot mode.
4. **First start.** It takes a few minutes and restarts once by itself:
   - Start 1: the system grows the card's storage partition to the full card, then restarts.
   - Start 2: pocket-ui's start entry is put in place, the system sets up its configuration, and pocket-ui opens
     its first-start setup (welcome, look, Wi-Fi, extras).

   No ROCKNIX logo appears; the screen shows "pocket-ui, based on ROCKNIX" and the system version while it loads.

The boot mode is stored. From now on, switching on starts pocket-ui while the card is in.

What the image contains, how it is built and checked, and its licences: [SD-IMAGE.md](SD-IMAGE.md). The same
notes are on the card in `pocket-ui-info/README.txt`. The image may be shared free of charge for non-commercial use
only: the ROCKNIX system it is based on includes components and artwork licensed for non-commercial use.

## Install with two files on the official ROCKNIX card

For a card flashed with the **official** ROCKNIX image (`ROCKNIX-SM8250.aarch64-<version>.img`), nothing of ROCKNIX is
redistributed and no SSH is needed. `scripts/make-fat-install.sh --hakux <hakux dir>` builds `pocket-ui-<version>-rocknix-card.zip`
with `mount-storage.sh`, `pocket-ui-seed.tar.gz` and a `README.txt` for users; with `--hakux` the seed also carries the Xbox core
(the same files as the SSH package, `scripts/hakux-files.sh`), without it Xbox games keep ROCKNIX's xemu. The user flashes ROCKNIX,
copies the two files
to the FAT partition (label `ROCKNIX`) on a PC, and starts the device twice. Research and limits:
`RESEARCH-ROCKNIX-FAT-INSTALL-2026-10-28.md` (outside this repository).

How it works [V = read in ROCKNIX's initramfs of 20261001, the rest tested with fakes]:

- ROCKNIX's initramfs `init` sources `/flash/mount-storage.sh` instead of mounting `/storage` itself (an undocumented but
  long-standing developer hook; the project owner approved using it as a user feature). The SD image uses the same hook
  ([SD-IMAGE.md](SD-IMAGE.md)); `deploy/fat/mount-storage.sh.in` is the variant that reads the files from a tar on `/flash`.
- Start 1: `/storage/.please_resize_me` exists, so the hook only mounts `/storage` and does nothing (ROCKNIX grows STORAGE and
  refuses to if `/storage/.config` exists). Start 2 (or the next start on a card in use): the hook checks the free space
  (STORAGE not grown means no install), the tar's md5 (embedded by the build; `md5sum`, `tar`, `df`, `cp` are in the
  initramfs busybox [V], `sha256sum` too, `find`, `cmp`, `date`, `chmod` are not), unpacks into `/storage/.pocket-ui-stage`,
  copies it over `/storage` with `cp -a` (only adds files), writes `releases/<ver>/.installed-from-card`, and deletes
  itself and the tar. A retry after a half-finished copy is marked by `/storage/.pocket-ui-install-running`.
- The tar has the layout of the image's seed: `pocket-ui/releases/<ver>`, `current` and the fixed links, the launcher,
  `uninstall.sh`, `licences/`, and `.config/system.d` with the essway drop-in and the boot screen's unit. With `--hakux` it also
  has `pocket-ui/hakux/` (the Xbox core with its `.info`, `SOURCE.txt`, our patch, `BUILD-INFO`, `licenses/` and `SHA256SUMS`,
  the folder `src/hakux.rs` starts RetroArch from); the hook copies it like every other file, nothing more to do in the
  initramfs.
- If a pocket-ui is already installed (`pocket-ui/current` exists, e.g. by the SSH package), nothing is changed; the
  hook and tar are removed and the log says so.
- Any failure: the hook returns, ROCKNIX starts EmulationStation as usual, the hook and the tar stay for the next start, and
  the reason is in `pocket-ui-install.log` on the FAT partition (readable on a PC) and in `/storage`.
- Tests: `sh deploy/fat/test-hook.sh` (gate): not grown yet, installed, third start, second run, another pocket-ui present,
  damaged tar, missing tar, small STORAGE, not a tar, wrong content, with a PATH that holds only the initramfs applets.
  Not yet run on a device or with the initramfs's own aarch64 busybox (`tar -xzf` there is read from its applet list and
  `init`, which uses `tar -xf` and `gunzip` the same way): the first run is the fresh-card test.

### Anbernic H700 handhelds

All three installs (card hook, Ports installer, SSH package) also accept ROCKNIX's **H700** image on the Anbernic
models of [DEVICES.md](DEVICES.md#anbernic-h700-handhelds-2026-10-27-installable-2026-10-29-experimental), experimental.
The card for those is flashed from ROCKNIX's H700 image (DDR4 first, DDR3 if it doesn't start) and needs the model's
`dtb.img` on its FAT partition; the step-by-step guide is in
[Getting started](user/getting-started.md#anbernic-h700-handhelds-experimental). Another H700 handheld and the RG CubeXX are refused with a
sentence and nothing is written.

## Go back to Android

Switch off. Hold **Vol Down** and press **Power**. Pick **Switch boot mode** until the bottom line reads **Boot
mode: Android**, then **Start**. Android starts as before; the card can stay in. To return to pocket-ui, switch the
boot mode back to Loader the same way. [V] on the author's device.

## Install on a ROCKNIX card you already use

For a card that already runs ROCKNIX on the Mini V2 (however it boots), or on one of the experimental Pocket 5, Flip 2
and first Mini (ROCKNIX's own SM8250 image, installed as ROCKNIX's guide for the device says: the SD card image of
this project is the Mini V2's only), the package
`pocket-ui-<version>-rocknix-install.zip` puts pocket-ui on it in place of EmulationStation and keeps games, saves and
settings. It is built with `scripts/make-install-zip.sh` and does phase 6 of the appendix on the device itself, over
SSH, with no computer-side tools: copy the zip to `/storage`, unpack it, `sh install.sh --check`, then `sh install.sh`.
The `README.txt` inside is the user's guide. New to ROCKNIX on the Mini V2? Since September 2026 ROCKNIX there needs its own one-time bootloader setup; this guide does not repeat it, the user follows [ROCKNIX's guide](https://rocknix.org/devices/retroid/retroid-pocket-mini/) first.

**Without SSH: `Install pocket-ui.sh` in `roms/ports`** (`deploy/package/port-installer.sh`; the build writes it next to
the zip, with the whole package as a tar.gz after the script, about 10 MB). The player copies this one file into
`roms/ports` (ROCKNIX's file sharing: Samba on in its Network Settings, then `\\<name or IP>` on Windows or `smb://<name or IP>` on a Mac, see [ROCKNIX: Add Games](https://rocknix.org/play/add-games/); or the card in a computer), has EmulationStation reload its list (Start → Game
Settings → Update Gamelists) and starts **Install pocket-ui** in Ports. The script is only a front end: it unpacks the
package to `/storage/.pocket-ui-port`, runs `install.sh --check` and then `install.sh --no-restart` unchanged, so every
check and the "nothing written until all passed" rule are install.sh's. What the screen shows is ROCKNIX's own
`text_viewer` (full-screen text with a Close button the pad operates; ROCKNIX's BIOS and download errors use it), running
a `tail -f` of the log of a worker started detached, so closing the viewer early never ends an install halfway. Steps:
1 of 3 looking at the device (the checks, as install.sh words them, and a count), 2 of 3 installing, 3 of 3 switching.
A refusal ends at step 1 with install.sh's sentence ("pocket-ui was NOT installed: this device is 'X' …"), "Nothing on
the card was changed", and "Press Close to go back to EmulationStation"; the same for too little space (about 40 MB are
needed to unpack), a damaged file or a bare starter without a zip. On success the entry removes itself, writes `Uninstall
pocket-ui.sh` (the same script without the package; under that name it runs `/storage/pocket-ui/uninstall.sh`) next to
it, and the interface restarts into pocket-ui through `systemd-run` (a port runs inside essway, where install.sh cannot
restart essway itself). A second start with the same version installed says "already installed" and changes nothing.
Without the package inside, the file looks for `pocket-ui-*-rocknix-install.zip` next to itself, in `/storage` or
`/storage/roms`. Tested by `test-package.sh` against the fake root (launch, checks, install, switch, second run, refusals,
uninstall) and on the Odin 2 Mini on 2026-10-28, launched through ROCKNIX's `runemu.sh` as EmulationStation does
(`INSTALLER-TEST-2026-10-28`, ssh, no button pressed: Close was simulated by ending the viewer).

`install.sh` writes nothing until every check has passed: the package's own checksums, ROCKNIX for SM8250 or SM8550 (aarch64) in the
tested version or a newer one (`TESTED_ROCKNIX`, today 20261001; newer gets a note, older is asked to update ROCKNIX first), the device tree's model "Retroid Pocket Mini V2" (or, noted as
experimental, a known handheld: "Retroid Pocket 5", "Retroid Pocket Flip2", their "… Visionox" variants, Pocket 6 (including TOP-DPAD), Nova, the AYN Odin 2 family and
"Retroid Pocket Mini"; [DEVICES.md](DEVICES.md#installing)), essway and
`start_es.sh`, ROCKNIX's first start done, free space on `/storage`, both programs running and printing the package's
version, and `pocket-ui --check-os-update /` on the running system: pocket-ui's own compatibility check
([UPDATES.md](UPDATES.md#os-what-pocket-ui-needs-from-the-new-system)) without its bootloader guard, which is about
update tars and refuses every upstream system. `--allow-untested-rocknix` (for a ROCKNIX older than the tested one) and `--allow-other-device` relax the version
and the model, never the compatibility check. The compatibility check, not the version number, decides about a newer ROCKNIX. Then it lays out `/storage/pocket-ui` as `dev.sh` and the updater do
(`releases/<version>`, `current`, `previous`, the three links, the launcher), puts the drop-in in place, checks that
systemd took it (else removes it again), puts the boot screen's unit and its link in place (from the next start,
[BOOT-SPLASH.md](BOOT-SPLASH.md)) and restarts essway, unless it runs inside essway itself. Running it again
changes nothing. `uninstall.sh` (also copied to `/storage/pocket-ui/`) removes the drop-in and the boot screen first, then the programs,
and puts back what pocket-ui changes at run time outside its folders (its Syncthing drop-in, `smb.conf`); pocket-ui's
settings stay unless `--purge`, `/storage/roms` is never touched.

Tested against a fake root by `deploy/package/test-package.sh` (part of the gate), and the real package once in the
virtual device's base image (ROCKNIX 20261001's own userland, arm64, with a stand-in `systemctl`, 2026-10-26):
`--check`, the install, a second install and the uninstall passed; the compatibility check found everything but the
flycast and scummvm cores, which that base image lacks (notes only). Not run on a device yet.

## If something goes wrong

- **Black screen with Boot mode: Loader.** The `loader` partition holds no working u-boot (an Android update can
  replace it, see phase 7). Switch back to Android as above; the fix is in phase 5 of the appendix, "Only if the
  `loader` holds no u-boot".
- **EmulationStation appears instead of pocket-ui.** Someone chose it (SELECT + START held, Settings → Help & tools → About, safe
  mode), or pocket-ui could not start at all: it is missing or not executable, it could not run even in safe mode, or
  sway stopped answering. The next start of pocket-ui says which (`/storage/pocket-ui/es-fallback`). Restart the
  Pocket to try pocket-ui again. Its log is `/var/log/pocket-ui.log` (in RAM; a copy on the card survives a
  restart: `/storage/.config/pocket-ui/logs/`, see [troubleshooting](user/troubleshooting.md#what-to-send-when-something-crashes)).
- **"Safe mode" appears.** pocket-ui kept stopping, even in the last version that worked; see
  [Safe mode](user/safe-mode.md).
- **The storage stayed small** (`df -h /storage` shows about 32 MB). Something created `/storage/.config` before the
  first start, so ROCKNIX refused to grow it ([SD-IMAGE.md](SD-IMAGE.md)). Write the card again.
- **Do not install ROCKNIX updates** on this card: they replace KERNEL and SYSTEM but not `boot/pocket/Image`, and
  the two must match (phase 7).

## How the start works

```
Boot mode: Android → Retroid ABL → Android                          (factory setting)
Boot mode: Loader  → Retroid ABL → loader partition: Retroid's u-boot
                   → SD card: /EFI/BOOT/BOOTAA64.EFI (GRUB)
                   → /boot/pocket/Image + miniv2-noufs.dtb → ROCKNIX system → pocket-ui
```

To switch: power off, hold **Vol Down** and press **Power**. Retroid's fastboot menu shows "Boot mode: …" at the
bottom. Pick **Switch boot mode** with the volume keys and Power, then **Start**. The choice is stored and stays
until you switch again. [V] on the author's device.

- **`loader`** is a partition Retroid created for other systems. It does not belong to Android. [V]
  [RetroidPocket/u-boot README](https://github.com/RetroidPocket/u-boot): "loader partition to boot alternative
  OSes … boot a loader using (Power + Volume Up)".
- **Until 2026-09-03 ROCKNIX used this same chain** on the Mini V2 (`BOOTLOADER="arm-efi"`, u-boot, GRUB). It then
  switched to its own ABL, which overwrites `abl_a` and `abl_b`. This guide does **not** take that route, because
  a faulty ABL can make the device unusable. [V] ROCKNIX `projects/ROCKNIX/devices/SM8250/options`, commits
  `0878019203` and `ad843ff62b`.
- **Retroid's u-boot** (`rp-v1.0.1`, 2024.10) starts with `bootcmd=… bootefi bootmgr` and looks for
  `/EFI/BOOT/BOOTAA64.EFI`. [V] strings from the binary.
- **Android stays invisible to Linux.** The device tree used here switches off the internal storage (UFS), as in
  the memory test of 2026-10-21 ([FASTBOOT-TEST.md](FASTBOOT-TEST.md)). No ROCKNIX script can reach Android or the
  bootloader. [V] The memory test showed no `sd*` devices.
- Retroid's README also names **Power + Vol Up** to start the loader once. On Android firmware
  `RPMiniV2_V1.0.0.275_20260407` that did not work: Android started both times. [V] Use the boot mode instead.

## The files on the card

On the FAT partition (label `ROCKNIX`, readable on any computer), besides ROCKNIX's unchanged `KERNEL` and `SYSTEM`:

| File | What it is |
|---|---|
| `EFI/BOOT/BOOTAA64.EFI` | GRUB, which Retroid's u-boot starts |
| `boot/grub/grub.cfg` | `deploy/sd-efi/grub.cfg`: no menu, goes straight into pocket-ui, with a quiet start (no kernel text on the screen, [BOOT-SPLASH.md](BOOT-SPLASH.md#quiet-start)) |
| `boot/pocket/Image`, `boot/pocket/miniv2-noufs.dtb` | The kernel from `KERNEL`, and the Mini V2 device tree with the internal UFS disabled |
| `post-flash.sh`, `mount-storage.sh` | Hooks ROCKNIX's start-up runs: no ROCKNIX logo; pocket-ui's start entry put in place once, and at every start system updates pocket-ui did not check set aside to `/storage/.update-refused` |
| `pocket-ui-info/` | Notes and licences |

On STORAGE: `pocket-ui/` (programs and launcher) and, until the second start, `.pocket-seed/` with the essway
drop-in and the boot screen's unit. Details: [SD-IMAGE.md](SD-IMAGE.md).

## Appendix: manual install on an existing ROCKNIX card

The way pocket-ui was first installed on the author's device, before the image existed: an SD card with plain
ROCKNIX, the boot files copied over SSH, the start entry installed with `scripts/autostart.sh`. It is still the way
to add pocket-ui to a ROCKNIX card you already use, and the place to look when the `loader` partition needs Retroid's
u-boot. The image above does phases 2 and 6 for you; for phase 6 alone on a card you already use, the install
package ([above](#install-on-a-rocknix-card-you-already-use)) is the safer way.

### What you need for the manual path

- A Retroid Pocket Mini V2 whose bootloader is already unlocked (`fastboot getvar unlocked` says `yes`).
  `fastboot boot` is refused on a locked bootloader. **Do not unlock it for this:** unlocking erases all Android
  data.
- A microSD card with the ROCKNIX SM8250 image (tested with version 20261001), started at least once so that it
  has grown its STORAGE partition, with SSH access to the running system.
- A computer with `fastboot` (Android platform tools), a USB-C **data** cable, Docker (to build the SD boot
  files) and this repository.
- Retroid's u-boot for the Mini V2: `u-boot-sm8250-retroidpocket-rpminiv2.img` from the
  [RetroidPocket/u-boot](https://github.com/RetroidPocket/u-boot) release `rp-v1.0.1` (647,168 bytes).

Read each phase before you run it. Phases 1 to 4 do not write to the device's internal storage. Phase 5 changes
the boot mode, a setting in Retroid's bootloader menu; it flashes the `loader` partitions only if they hold no
u-boot.

### The boot files

`scripts/make-sd-efi.sh KERNEL miniv2-noufs.dtb OUT` builds them reproducibly:

| File on the SD card (FAT partition, label `ROCKNIX`) | Origin |
|---|---|
| `EFI/BOOT/BOOTAA64.EFI` | GRUB from Debian `grub-efi-arm64-bin` 2.12 (built in Docker) |
| `boot/grub/grub.cfg` | `deploy/sd-efi/grub.cfg`: no menu, goes straight into pocket-ui |
| `boot/grub/grub-efi-arm64-bin.version` | Which Debian GRUB package `BOOTAA64.EFI` came from (for the GPL source offer) |
| `boot/pocket/Image` | The arm64 kernel, unpacked from `/flash/KERNEL` of the ROCKNIX image |
| `boot/pocket/miniv2-noufs.dtb` | The Mini V2 device tree from KERNEL (`scripts/make-test-bootimg.py`) with `ufshc@1d84000` and `phy@1d87000` set to `disabled`. `scripts/image/build.sh extract` makes it reproducibly with `fdtput`, byte for byte the device's file |
| `boot/pocket/KERNEL.source.md5` | md5 of the KERNEL the `Image` came from |

The script prints the sha256 of every file (`SHA256SUMS`). The existing `KERNEL`, `SYSTEM` and `rocknix_abl/`
on the card are not touched. About 61 MB of space is needed on the FAT partition. [V]

Each phase below can be undone. Do one at a time.

### Phase 1: look around (writes nothing)

1. Switch the device off completely. Hold **Vol Up** and press **Power**. Note what appears (on firmware
   `RPMiniV2_V1.0.0.275_20260407` Android starts even with u-boot in `loader`; phase 4 shows for sure):
   - A Retroid or u-boot screen or menu: the `loader` already holds a u-boot, so you may not need to write
     anything internally.
   - Android starts normally, or the screen stays black: `loader` is empty or broken. If the screen hangs,
     hold Power for 15 seconds.
2. Switch off, hold **Vol Down** and press **Power** (fastboot), connect USB-C to the computer. Read only:
   `fastboot getvar current-slot`, `getvar partition-size:loader_a`, `getvar partition-size:loader_b`,
   `getvar unlocked`.

**Undo:** nothing was changed. Hold Power; Android starts.

### Phase 2: prepare the SD card (writes only to the SD card)

In the running system, over SSH: make `/flash` writable for a moment, copy the five files into `EFI/` and
`boot/`, check the checksums, mount `/flash` read-only again. When you copy from a Mac, set
`COPYFILE_DISABLE=1`, or `._*` files appear.

**Undo:** delete the `EFI/` and `boot/` folders on the SD card.

### Phase 3: trial start from memory (writes nothing internally)

Fastboot mode (Vol Down + Power), then on the computer:

```
fastboot boot u-boot-sm8250-retroidpocket-rpminiv2.img
```

u-boot runs from memory only, finds GRUB on the SD card, and pocket-ui starts. Check:

- The picture is the right way up.
- Buttons, Wi-Fi and sound work.
- Sleep works.
- Android is invisible (no `sd*` devices).
- Note the boot time.

**Undo:** restart; Android starts. Nothing was flashed.

### Phase 4: back up `loader` and `abl` (writes nothing important)

In Android: Settings → Handheld Settings → Advanced → **Run Script as Root**, then
`scripts/android/backup_loader.sh` (copy it to the internal storage first). It copies `loader_a`, `loader_b`,
`abl_a` and `abl_b` to `/sdcard/pocket-backup/` with `dd` and writes the sha256 of each partition and each copy.
Copy the folder to your computer afterwards (with USB debugging on: `adb pull /sdcard/pocket-backup`).

Retroid's "Run Script as Root" runs every line **on its own**: no variables, loops or functions carry over from
one line to the next. That is why the script is one plain command per line. [V] logcat, `PServiceBridgeV2`.

**Undo:** not needed. Only files on Android's storage are created.

### Phase 5: switch the boot mode to Loader (a setting, nothing flashed)

Fastboot menu (Vol Down + Power) → **Switch boot mode** until the bottom line reads **Boot mode: Loader** →
**Start**. The `loader` u-boot finds GRUB on the SD card, and the system starts.

**Undo:** the same menu, **Switch boot mode** back to Android, then Start.

#### Only if the `loader` holds no u-boot

Phase 4 shows it: a `loader_a.img` that starts with `ANDROID!` holds a boot image (Retroid's u-boot). If it is
empty, or the start in this phase stays black, write Retroid's u-boot:

```
fastboot flash loader_a u-boot-sm8250-retroidpocket-rpminiv2.img
fastboot flash loader_b u-boot-sm8250-retroidpocket-rpminiv2.img
```

Verify after each write.

- This touches the `loader` partitions only: nothing that belongs to Android, and not `abl`.

**Undo:** `fastboot flash loader_a/b` with the backups from phase 4. Even a broken `loader` does not stop Android:
with Boot mode: Android the ABL starts Android without it. [I] Retroid README and ROCKNIX's fix instructions.

### Phase 6: pocket-ui as the start screen (writes only under /storage)

Until now `scripts/dev.sh` started pocket-ui by hand each time, and EmulationStation was visible briefly before the
launcher ended it. For permanent use pocket-ui becomes the `essway` unit, the unit ROCKNIX starts for
EmulationStation.

**How:** one file, a drop-in for `essway`. `deploy/essway-pocket-ui.conf` goes to
`/storage/.config/system.d/essway.service.d/10-pocket-ui.conf`.

- `/storage/.config/system.d/` is a systemd unit folder that takes precedence over `/usr/lib` and survives
  restarts. [V] on the device.
- The drop-in clears `ExecStart=` and starts `/storage/pocket-ui/run-pocket-ui.sh` instead. If the launcher is
  missing it starts `/usr/bin/start_es.sh` directly.
- It sets `POCKETUI_AS_ESSWAY=1`. It has no start limit (`StartLimitIntervalSec=0`): a unit that hit one would stay
  "failed", which is a black screen with only sway. Instead `RestartSteps=5` and `RestartMaxDelaySec=60` stretch the
  pause before each restart from 2 s to a minute, so something that fails at once does not restart every 2 seconds
  forever either (systemd 254 or later; ROCKNIX 20261001 has 255).
- `Restart=always`, `RestartSec=2`, `HOME=/storage` and `EnvironmentFile=/etc/profile` are copied from ROCKNIX's
  own unit.
- The name `essway` stays, because `runemu.sh` ties games to it with `StopPropagatedFrom=essway.service`.
- `systemctl disable essway` does not work, because the unit is enabled from the read-only `/usr`. Hence the
  drop-in.

**The launcher as `essway`** (`POCKETUI_AS_ESSWAY=1`, tested by `deploy/test-run-pocket-ui.sh`):

- It never calls `systemctl stop/start essway`, because it is `essway`.
- Cleanup, restarts after crashes, rollback after a failed update and exit code 75 work as before. A crash loop goes
  on to the last known good version, then to safe mode ([ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)).
- It replaces itself with `/usr/bin/start_es.sh` through `exec` only when pocket-ui asks for it (exit 77, after the
  player chose Open EmulationStation) or cannot start at all (not even in safe mode, exit 126/127, sway gone).
  Exit 0 starts pocket-ui again. EmulationStation then runs in the same unit. When EmulationStation ends, systemd
  (`Restart=always`) starts the launcher, and so pocket-ui, again.
- If `/storage/pocket-ui/pocket-ui` is missing or not executable, it starts EmulationStation at once. (A fresh
  update without a program is first rolled back to the previous version, a release without one to the known good
  version.) A broken installation therefore always ends in a usable interface.

**Install** (from your computer, over SSH). `scripts/autostart.sh install` only shows what would happen (the
default is `--dry-run`). `scripts/autostart.sh install --apply` does it:

1. Read-only checks: SSH login, `essway.service`, `/usr/bin/start_es.sh`, `essway` not masked (desktop mode),
   `/storage/.config/system.d/` present.
2. Also checks that pocket-ui is installed and that the launcher on the device knows the essway mode. An old
   launcher would stop `essway`, which is itself; run `dev.sh deploy` first in that case. It notes when systemd is
   older than 254 (then the growing restart pause is ignored).
3. Copies the drop-in (first as `.new`, then `mv`), and the boot screen's unit `deploy/pocket-splash.service` with its
   link in `sysinit.target.wants/` ([BOOT-SPLASH.md](BOOT-SPLASH.md); it shows from the next start).
4. `systemctl daemon-reload`.
5. Checks that systemd loaded the drop-in (`DropInPaths`, `POCKETUI_AS_ESSWAY=1`). Otherwise it deletes the file
   again and stops.
6. Stops an old `pocketui` unit from `dev.sh`, removes the `/tmp/pocket-ui.es-only` marker, runs
   `systemctl reset-failed essway` and `systemctl restart essway`.
7. After 5 seconds it shows the state (`scripts/autostart.sh status`: also `current` and `known-good`, and why the
   launcher opened EmulationStation by itself, if it did). If pocket-ui is not running it shows the launcher log.

**Developing with it:**

- `dev.sh deploy` detects the drop-in. It stops `essway` before copying and restarts it afterwards with
  `systemctl restart essway`, instead of `systemd-run --unit=pocketui`. With the drop-in it also puts the boot
  screen's unit and link in place (never starts it).
- A quiet start on a card set up by hand (its `grub.cfg` predates it): `scripts/boot-quiet.sh apply`, undo with
  `scripts/boot-quiet.sh undo` ([BOOT-SPLASH.md](BOOT-SPLASH.md#on-a-card-set-up-before-the-image-had-it)).
- `dev.sh stop` creates `/tmp/pocket-ui.es-only` and restarts `essway`. The launcher sees the marker and starts
  EmulationStation at once, until the next `dev.sh deploy` (which removes the marker) or a restart (`/tmp` is
  then empty).

**At boot** `essway` starts as soon as `sway.service` has started, before sway accepts connections. The launcher
therefore waits until `swaymsg -t get_version` answers (at most `POCKETUI_SWAY_WAIT`, 30 s) before the first start;
without that wait the first cold start died with `NoCompositor`. [V]

Clock and time zone are right after a cold start without `start_es.sh` (it normally loads `es_settings`). [V]

**Undo:** `scripts/autostart.sh uninstall --apply` deletes the drop-in and the boot screen's unit and link, runs
`daemon-reload` and restarts `essway`; EmulationStation is back. By hand: `rm
/storage/.config/system.d/essway.service.d/10-pocket-ui.conf /storage/.config/system.d/pocket-splash.service
/storage/.config/system.d/sysinit.target.wants/pocket-splash.service`, then `systemctl daemon-reload` and `systemctl
restart essway`, or simply restart. Without those files ROCKNIX is as it was.

### Phase 7: protection for permanent use

- **Switch off ROCKNIX's own auto-updater.** A ROCKNIX update replaces `KERNEL` and `SYSTEM` but not our
  `boot/pocket/Image`, so kernel and system would no longer match. Updates go through pocket-ui only.
  *Planned:* regenerating `Image` automatically from a new `KERNEL` (`KERNEL.source.md5` detects the difference).
  Until then, rerun `scripts/make-sd-efi.sh` by hand after any system change.
- **Known fault in Retroid's Android OTA** (see rocknix.org): an Android update wrote a faulty u-boot into
  `loader`. The GRUB selection menu was invisible afterwards, and users booted with the wrong device model
  (rotated picture). This does not affect this setup: our `grub.cfg` has no menu and always loads the Mini V2
  device tree. Phase 3 tests with the corrected `rp-v1.0.1`. Phases 1 and 4 show which u-boot is in the device;
  if it is the faulty one, phase 5 replaces it with `rp-v1.0.1`, the file ROCKNIX links as the fix.
- **Android OTA:** a Retroid update can overwrite `loader`. Then Boot mode: Loader no longer reaches pocket-ui, but
  Android runs normally. Write the u-boot as in phase 5.

## Results on the author's device

- **Phase 1 (2026-10-22):** Vol Up + Power started Android, tried twice. (Phase 4 later showed that `loader` does
  hold a u-boot; Vol Up just does not select it on this firmware.)
  `fastboot getvar`: `product` kona, `current-slot` b, `unlocked` yes, `loader_a` and `loader_b` 0xA00000
  (10 MiB) each, `abl_a` 0x100000.
- **Phase 2 (2026-10-22):** The five files are on the SD FAT partition, `sha256sum -c` is OK, the card is `ro`
  again. `KERNEL`, `SYSTEM` and `rocknix_abl/` are unchanged.
- **Phase 3 (2026-10-22):** `fastboot boot u-boot…img`, then u-boot, then GRUB (no menu), then the kernel. SSH was
  reachable after 25 seconds.
  - `/proc/cmdline` starts with `BOOT_IMAGE=/boot/pocket/Image`, and `/sys/firmware/efi` exists.
  - Model "Retroid Pocket Mini V2", no `sd*` devices, picture the right way up.
  - Sound (speaker and headphones), Wi-Fi and battery work.
  - `mem_sleep` is `[s2idle]`. dmesg shows no new errors.

- **Phase 4 (2026-10-22):** the four copies match their partitions byte for byte. `loader_a` and `loader_b` each
  hold Retroid's u-boot 2024.10-rc4 (built 2024-10-17, `bootcmd=bootefi bootmgr; pause`), not `rp-v1.0.1`.
  `abl_a` and `abl_b` are identical.
- **Phase 5 (2026-10-22):** Boot mode: Loader, then Start: the existing u-boot found GRUB, and the system came up
  (`BOOT_IMAGE=/boot/pocket/Image`, EFI present, no `sd*`). Nothing was flashed.
- **Phase 6 (2026-10-22):** drop-in installed with `scripts/autostart.sh install --apply`. First cold start: pocket-ui
  died once with `NoCompositor` and started on the next try (fixed by the wait above). Second cold start: pocket-ui
  on the first try, no EmulationStation, about 33 s from switching off to Wi-Fi.


## Developing after the install

- `dev.sh deploy`, screenshots, logs, `devlock.sh` and the virtual device work as before. A `dev-ssh` marker in
  `/storage/.config/pocket-ui/` keeps SSH on permanently ([SECURITY-HARDENING.md](SECURITY-HARDENING.md)).
- A restart is no longer a problem: with Boot mode: Loader, power-on brings pocket-ui back without a computer.
- After a kernel or system change, rerun `scripts/make-sd-efi.sh` and replace `boot/pocket/` on the SD card.

## What this guide does not do

- No `installtointernal`, no `updateabl`, no OpenABL or ROCKNIX ABL in `abl_a/b`.
- No writes to Android partitions, no bootloader unlock or lock.


### Experimental SM8550 handhelds

The **Retroid Pocket 6** (including TOP-DPAD), **Retroid Pocket Nova**, **AYN Odin 2 Mini**,
**AYN Odin 2** and **AYN Odin 2 Portal** are experimental. Only the Odin 2 Mini has been tried on a real device
(2026-10-27: the install package, the first-start screens, the shelf, a GBA game with the pause menu, screen off and
wake, driven over the command pipe; no button or touch pressed, no Bluetooth, no sleep); the other four are computer
tests only, with no hardware verification. Use ROCKNIX's own **SM8550 aarch64 image**
(20261001), set up following ROCKNIX's guide for your device, boot it once, then add pocket-ui
with the install package. The Mini V2 SD image must not be used on these devices.

Pocket 6 and the Odin 2 family use 1920×1080; Nova uses 1280×960. First start says once that the device is untested;
About marks it experimental. **Stock ROCKNIX still disables sleep on these devices (checked 2026-10-27):** the power button
turns the screen off, and idle turns it off with the game paused. Any button brings it back.
Settings → System → Sleep & power offers **Switch off after** (Off by default) to save and power
off after a longer dark spell on battery. Screen off still uses battery. ROCKNIX controls the fan;
pocket-ui's fan choices have no effect here. Touch, LEDs, vibration, battery drain, 120 Hz pacing
and Zink on the real GPU still need a tester. See [device notes](DEVICES.md).
