# Updates

One signed channel file, `update.json` + `update.json.sig` (Ed25519, the store key), lists the
newest app and OS release. Code: `src/update.rs`; page: Settings → System → Updates.

```json
{ "format": 1,
  "app": { "version": "0.2.0", "url": "pocket-ui-0.2.0-aarch64.tar.gz", "sha256": "…", "size": 2078495, "notes": "…" },
  "os":  { "version": "20261101", "url": "…-SM8250.aarch64-20261101.tar", "sha256": "…", "size": …, "notes": "…" } }
```

`url` is absolute or relative to update.json. The channel URL is `update::DEFAULT_URL`,
`https://pocketui.xyz/updates/stable/update.json` (the list on the website, the files on GitHub Releases; nothing is
published there yet), or `update_url` in `/storage/.config/pocket-ui/settings.json` (`file://` works too, which the
device test uses). Only `https://` and `file://` are fetched, and a redirect is followed only to `https://`
(`--proto =https,file --proto-redir =https`); hosting, signing and the release procedure are in
[UPDATE-CHANNEL.md](UPDATE-CHANNEL.md). `sd_image` and `install_zip` in the list are for people; a device ignores them.

## App: live, with automatic way back

```
/storage/pocket-ui/
  releases/<version>/      pocket-ui, pocket-core, portmaster-setup.sh
  releases/dev/            what scripts/dev.sh deploy copies
  current  -> releases/0.2.0      switched by one rename()
  previous -> releases/dev
  known-good -> releases/0.1.9    last version that ran POCKETUI_STABLE_SECS (120 s) with a picture (the launcher sets it)
  releases/dev-good/       a known good dev build, copied aside by dev.sh deploy before it overwrites releases/dev
  pocket-ui, pocket-core, portmaster-setup.sh -> current/…   (paths stay stable)
  update-pending           version just installed, until it has presented its first frame
  rolled-back              version the launcher gave up on (shown once, then removed)
  es-fallback              why the launcher opened EmulationStation by itself (shown once, then removed)
```

Install = download → size + SHA-256 against the signed list → tar paths checked → unpacked to
`.staging-<v>` → **both binaries must print exactly `pocket-ui <version>` / `pocket-core <version>`** (catches a
wrong architecture or a broken build before anything changes; a substring test once mistook
11.2.3 for 1.2.3) → staging renamed into
`releases/<v>` → `previous` and `current` switched → pocket-ui exits with 75.

`deploy/run-pocket-ui.sh` restarts on 75. If pocket-ui then exits with an error while
`update-pending` is still there, it points `current` back at `previous` and writes `rolled-back`.
A version that fails later, after it confirmed itself, goes back to `known-good` once its crash restarts are used up,
then to safe mode; pruning keeps `current`, `previous` and `known-good` (the whole sequence:
[ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)).
Only the new version itself may remove `update-pending` (the old process is still alive for a
moment after installing; it once confirmed the new version by mistake).

`update-pending` goes away when the new version has **presented its first frame** and the event
loop has turned once more (`update::watch_health`, started at the very beginning of `main`), not
after a fixed time. A version that hangs before that is killed by the same thread after
`WATCHDOG_AFTER` (20 s): it exits with `WATCHDOG_EXIT` (124), which the launcher treats like a crash
and rolls back. (Before, 10 s of uptime counted as healthy: a build that hung at 13 s stayed.)

Downloads (`update::download`) give curl `--connect-timeout 20`, `--speed-limit 10240
--speed-time 30` (under 10 KB/s for 30 s counts as stalled and is retried) and
`--max-filesize <size from the signed list>`; the poll loop also stops a file that grows past that
size. The Updates page offers **A: Cancel** while a download runs (the partial file stays and is
resumed by the next try). A running job keeps auto-sleep off for at most 30 minutes.

Releasing to the public channel: `scripts/release-channel.sh` makes and signs the list from the release artefacts
and writes the folder for the website ([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#releasing-step-by-step)). For a
channel of your own (a folder served anywhere, or `file://`), `scripts/release-app.sh <version> <out-dir> "notes"`
builds with `POCKETUI_VERSION=<version>`, packs the tarball and adds it to `<out-dir>/update.json` with relative URLs
(`store-tool update`, which signs it). Upload the folder.

Verified on the device (21.10.2026, channel via file://):
- dev → 0.2.0: installed, restarted by itself (exit 75), 0.2.0 confirmed itself after 10 s (the
  timer back then; since then it is the first frame, see above).
- 0.2.0 → a 0.2.1 whose pocket-core reports another version: refused before any change.
- 0.2.0 → a 0.2.1 that passes the check but crashes on start: see the result in the log below.

## OS: next start, never the bootloader

The initramfs installs any `*.tar` in `/storage/.update` at boot: it copies KERNEL and SYSTEM to
`/flash` and then runs **the new SYSTEM's** `usr/share/bootloader/update.sh`. ROCKNIX's SM8250
version calls `/usr/bin/updateabl` unconditionally, which rewrites the ABL in the internal UFS,
the bootloader Android starts from (docs/BOOT-NOTES.md). Today our RAM-boot DTB hides UFS, so
it finds nothing; with a permanent boot through the stock DTB it would flash.

So `update::stage_os` downloads to `/storage/.pocket-update` first, checks size and hash, then
opens the SYSTEM squashfs with `unsquashfs` and refuses the update if
- `usr/bin/updateabl` exists in it at all, or
- `update.sh` / `canupdate.sh` mention updateabl, abl_, block devices, partition labels,
  fastboot or `dd`, or
- `HW_DEVICE`/`HW_ARCH` in its os-release differ from the running system, or its version
  doesn't match the signed list.

Only then is the tar renamed into `/storage/.update`, with `/storage/.update/.pocket-ui-approved` (`<sha256>  <tar
name>`, `update::OS_APPROVAL`) written just before it; "Remove the waiting system update" takes both away. On our SD
image the boot hook installs nothing else: any other update in `/storage/.update` (EmulationStation's updater, the
Samba share, a copy) is moved to `/storage/.update-refused` before ROCKNIX's `check_update` sees it
([SD-IMAGE.md](SD-IMAGE.md), `deploy/image/mount-storage.sh`). Every upstream ROCKNIX update is refused by design: OS
updates come from our own image build, which drops updateabl.

Not tested on the device yet (no image of our own exists). The kernel does not come from the SYSTEM that an OS
update replaces:
- While ROCKNIX is started with `fastboot boot`, it comes from the computer: after an OS update the boot image must
  be rebuilt from the new KERNEL (`scripts/make-test-bootimg.py`); the page says so.
- With the dual-boot setup (`docs/INSTALL-DUAL-BOOT.md`) it is `boot/pocket/Image` on the SD card: rerun
  `scripts/make-sd-efi.sh` after an OS update. Doing this automatically is planned (`KERNEL.source.md5` already
  records which KERNEL the `Image` came from).

## OS: what pocket-ui needs from the new system

pocket-ui and pocket-core are built separately from ROCKNIX (zigbuild against glibc 2.38, [DEVELOPING.md](DEVELOPING.md)),
so a new system can break them while EmulationStation, built with the image, keeps working. Before an OS update is
staged, `update::inspect_system` therefore also runs `os_compat::check` on the opened SYSTEM (one `unsquashfs -lls`
listing, single files with `unsquashfs -cat`, links followed inside the image):

| What | How | Missing |
|---|---|---|
| The libraries pocket-ui and pocket-core link to, and every symbol version they use from each (`GLIBC_2.35` …) | the installed binaries' ELF `DT_NEEDED` and `.gnu.version_r`, against the new library's `.gnu.version_d` | refused ("… need glibc 2.38 or later (libc.so.6), the new system has glibc 2.37") |
| The libraries they open at run time | `os_compat::DLOPEN`: libwayland-client, libxkbcommon, libEGL, libwayland-egl; libasound or libpulse-simple | refused; sound only a note |
| The programs and ROCKNIX scripts they call | `os_compat::FILES`: sh, bash, sway, swaymsg, systemctl, runemu.sh, start_es.sh, emulationstation, 001-functions, es_systems.cfg; wifictl, nmcli, iwd, PipeWire, pactl, Bluetooth, steamos-set-timezone, curl, tar, unsquashfs, timeout, quantum, grim, moonlight | the first group refused, the rest a note |
| The libretro cores pocket-ui routes to pocket-core | `hostsys::routed_cores` (the handhelds and the verified systems) in `usr/lib/libretro` | a note, unless the Store's copy is in `/storage/cores` |
| ROCKNIX's `get_setting` and `set_setting` | defined in `etc/profile.d/001-functions` | a note |
| `start_es.sh`, `es_settings` | sha256 against `os_compat::KNOWN_UI_START` | logged only: compare the new steps with the launcher ([ROCKNIX's UI start](#rocknixs-ui-start)) |

Refused means the tar is deleted and the page says what is missing; a note is added to "Ready. It installs the next
time the system starts." The whole report goes to the log (`update: system … checked:`). The bootloader guard above
is unchanged and comes first.

**Dry run.** `pocket-ui --check-os-update <path>` runs the same checks without staging anything and prints every
finding (`BLOCKS`, `WARNS`, `info`). `<path>` is an update tar (its SYSTEM is unpacked to
`/storage/.pocket-update/check-dry-run` and removed), a SYSTEM squashfs or an unpacked system folder. Exit code 0:
it would be installed, 1: refused, 2: no path. On the device:

```sh
/storage/pocket-ui/pocket-ui --check-os-update /storage/ROCKNIX-SM8250.aarch64-20261101.tar
```

Run on the virtual device's copy of the 20261001 SYSTEM (`pocket-vdev-base` container, ROCKNIX's own unsquashfs,
23.10.2026): glibc 2.41, every library, program and core there, `start_es.sh` and `es_settings` known, and refused
only by the bootloader guard (`usr/bin/updateabl`), as every upstream image is.

## ROCKNIX's UI start

`essway.service` runs `start_es.sh`, which loads `es_settings` and then starts EmulationStation. With pocket-ui the
launcher takes `start_es.sh`'s place, so `deploy/run-pocket-ui.sh` repeats what those two do before the UI comes up
(section "ROCKNIX's UI start" in the script):

| ROCKNIX | Since | The launcher |
|---|---|---|
| `export SDL_AUDIODRIVER=pulseaudio` (es_settings) | 20261001 | the value read from `/usr/bin/es_settings`, exported for pocket-ui and the games it starts |
| `system.language` → `LANG`, `LANGUAGE`, `LOCPATH`, `localedef`, `systemctl import-environment` | 20261001 | the variables before the start; the locale compiled once and imported after the first picture |
| `system.timezone` → `/storage/.cache/timezone`, `system_timezone`, `systemctl restart tz-data` | 20261001 | after the first picture, only when the files say another zone |
| `systemctl import-environment` of every variable in `999-export` | 20261001 | after the first picture, the set ones in one call |
| `set_kill set "emulationstation"` | 20261001 | after the first picture (it matches no process under pocket-ui; runemu.sh sets the same after each game) |
| `prerotate_env "$WLR_CON_TRANSFORM" <mode width>` (start_es.sh) | ROCKNIX main after 20261001 | ROCKNIX's own function in bash while the launcher waits for sway; `vk_wsi_wayland_prerotate`/`egl_wayland_prerotate` exported before the first start. With the Mini V2's plane rotation (`DEVICE_PLANE_ROTATION=true`) it sets nothing |
| `scanout_reset_stale` (start_es.sh) | ROCKNIX main after 20261001 | before the first start and at the end of every `clean_up`: a game killed before its runemu.sh put the output scale and the display controller's sharpening back leaves `/run/rocknix/scanout-owner`; ROCKNIX's function resets them unless that runemu.sh still runs |

Each step is skipped when the running system lacks its function (`rocknix_has` looks in 001-functions) or file, so the
same launcher works on 20261001 and on the next release. ROCKNIX's profile is loaded only for a step that needs one
of its functions: on 20261001 not before the first picture, and the bash for the pre-rotation runs while the launcher
waits for sway anyway. `deploy/test-run-pocket-ui.sh` covers every step with a fake profile that carries ROCKNIX main's
own `prerotate_env` and scanout functions.

ROCKNIX main also changed how `system.cfg` is locked (`flock`, [ARCHITECTURE.md](ARCHITECTURE.md#rocknixs-systemcfg));
`system_cfg` follows whichever the running system uses.

## A ROCKNIX bump

The launcher's UI-start steps, `system_cfg`'s lock and the compatibility check all follow ROCKNIX as it is now. Before
the device or our image moves to a newer ROCKNIX:

1. `scripts/rocknix-contract.sh --root <unpacked SYSTEM>` on the new image (or `--ref <tag>` on its source, `--host
   <device>` on a running one): every promise pocket-ui relies on, ok, CHANGED or MISSING with the evidence, ending in
   `CONTRACT: ok` or `CONTRACT: <n> changed` ([ROCKNIX-CONTRACT.md](ROCKNIX-CONTRACT.md)). Each CHANGED row names the
   code that relies on it; after fixing that code, update the row in `scripts/rocknix-contract.txt` (cargo test keeps
   both in step).
2. `scripts/rocknix-diff.sh <old> <new>` (refs or images, [DEVELOPING.md](DEVELOPING.md#a-new-rocknix-version)). It
   prints what changed in what pocket-ui depends on and a checklist. 20261001 → main 8b70f69 (23.10.2026): the new
   `start_es.sh` steps (handled, above), the flock lock (handled), get_setting rewritten (same order of keys), the
   SM8250 quirk `075-scanout-offload`, `wifi.country` in system.cfg, PipeWire 1.2 → 1.6, systemd 255 → 261,
   glibc 2.41 → 2.44 (newer is fine for a binary built against 2.38).
3. For each new step in `start_es.sh` or `es_settings`: the same in `deploy/run-pocket-ui.sh` ("ROCKNIX's UI start",
   guarded by `rocknix_has` or the file), a launcher test, and the new sha256 in `os_compat::KNOWN_UI_START`.
4. For each changed function pocket-ui imitates or calls (`set_setting`, `wait_lock`, `get_setting`): `src/system_cfg.rs`
   and its tests against the new function text.
5. A new program, script or library pocket-ui starts using: add it to `os_compat::FILES` or `DLOPEN`, to the table
   in `scripts/rocknix-diff.sh`, and a row to `scripts/rocknix-contract.txt` (with a content check where pocket-ui
   relies on more than the file being there).
6. On the new image: `pocket-ui --check-os-update <tar>` (dry run above), then the virtual device on the new image
   (`ROCKNIX_IMG=… scripts/vdev.sh image --rebuild-base`), then the device.
