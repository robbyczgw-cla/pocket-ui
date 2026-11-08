# Updates

One signed channel file, `update.json` + `update.json.sig` (Ed25519, the store key), lists the
newest app and OS release. Code: `src/update.rs`; page: Settings → System → Updates.

```json
{ "format": 1,
  "app": { "version": "0.2.0", "url": "pocket-ui-0.2.0-aarch64.tar.gz", "sha256": "…", "size": 2078495, "notes": "…" },
  "os":  { "version": "20261101", "url": "…-SM8250.aarch64-20261101.tar", "sha256": "…", "size": …, "notes": "…" } }
```

`url` is absolute or relative to update.json. The channel URL is `update::DEFAULT_URL`,
`https://pocketui.xyz/updates/stable/update.json` (the signed list on the website; app files can be hosted there with GitHub Releases as fallback), or `update_url` in `/storage/.config/pocket-ui/settings.json` (`file://` works too, which the
device test uses). Only `https://` and `file://` are fetched, and a redirect is followed only to `https://`
(`--proto =https,file --proto-redir =https`); hosting, signing and the release procedure are in
[UPDATE-CHANNEL.md](UPDATE-CHANNEL.md). `sd_image` and `install_zip` in the list are for people; a device ignores them.

## Bounded downloads and alternate sources (0.3)

A release may add `"mirrors": ["https://github.com/owner/repo/releases/download/vX/file"]`.
`url` remains the first source, so older clients still work; all sources serve **identical bytes**.
The complete list of sources is signed with the existing Ed25519 key. Size and SHA-256 checks
apply equally to every source. HTTPS and HTTPS-only redirects remain enforced (`file://` for offline tests).
A changed mirror list without a fresh signature is refused.

The worker tries at most four signed sources, once each. It stops a source after 30 seconds
without any file growth, or after five minutes plus the file's size at 10 KB/s (`source_deadline`: a 15 MB
app gets about 30 minutes, a 1.5 GB system update a day), including DNS, TLS and redirects. A flat five minutes
cut every system update and, on weak Wi-Fi, the app, and the next source began from nothing. Curl also has
20-second connect and 30-second low-speed timeouts. There are no hidden curl retries. A host that stalls is left
after 30 seconds; one that trickles is bounded by that deadline. When the deadline ends while bytes
still come, the page says "The download is too slow on this network"; curl's own limit is 10 s after the
worker's. Before the download the app update checks the SD card's room: the rest of the download, three times
the archive unpacked, and 32 MB to spare (`update::room_for_app`); curl's write error (23) says the card is
full, not Wi-Fi. A failed secure connection (curl 35, 60) with the clock before this build (`clock_check`) names
the clock.
Cancellation stops the operation rather than starting the next host. On a host change, partial
bytes are discarded: a corrupt prefix from one host cannot poison the next. A partial file from
the same source can still resume on a later attempt; the final full-file hash remains mandatory.

The page names connecting, trying the next server, downloading, checking and preparing phases;
a stalled final source reports “The download server stopped responding. Nothing was installed.
Try again.” Failure leaves the active installation unchanged. Source-index failures go to the
UI log without URLs or credentials. The progress counter restarts on a change of host.

`scripts/release-channel.sh --direct-app …` prepares the app beside the signed website list
under `/updates/<channel>/`, with GitHub as the signed fallback. It refuses an app over
80,000,000 bytes to leave margin below the Hobby upload limit. It publishes nothing.
`store-tool mirror KEY update.json app URL [URL] [URL]` signs alternate sources;
`store-tool list` and `release-channel.sh --check` check every source, not only the primary.
Card images, source archives and installers can stay on the release. Check the complete
website deployment size and remaining monthly transfer before requesting a public deploy.

## The hint on the shelf

`src/update_hint.rs` (rules, `update-hint.json`) and `src/app_update_hint.rs` (the App's side). `FIRST_LOOK` (60 s)
after the start, with Wi-Fi up (`status_corner::status().wifi`; no Wi-Fi = no timer, the change of Wi-Fi wakes the
loop), a thread fetches the same signed list (`update::fetch_manifest`, `UpdateEvent::Looked`) and keeps it for the
Updates page. `update_hint::due` says a version when `app` is newer than the running one and not `said` yet; the
notice (`Notice::update_ready`, A to the page, B closes) waits for `shelf_notice_free()` and writes `said` when it is
shown. Next look `EVERY` (24 h) after a look that found nothing or said its hint; after a failed one `retry_after` (1 h, 2, 4, 8, at most 16 h). `update_hint::step(Facts)` is the one decision (Stop / Pause / Retry / Say / Look); a look also needs the buttons left alone for `IDLE_FOR` (10 s).
Never during a game, never in Kids Mode, never on a host-owned device (Steam Deck) or with `POCKETUI_MOCK`, and only
in a release build (`update::release_build`: the release scripts set `POCKETUI_VERSION`; a main or branch build carries
Cargo.toml's older version and would announce the release over itself, `update_hint::looks`);
The switch Settings → System → Updates → "Check for updates automatically" (On by default) is `{"off": true}` in the file: Off makes no request, says nothing and forgets what was found; the page's first-open check is skipped too ("Check again" still works by hand). A known newer version puts an amber dot (`App::update_dot`) on System and on its Updates row. On the page "Install pocket-ui X" asks first ("Install now" / "Later", B = Later; `Updates::confirm`); under 20 % and off the charger the line under it says to plug in (`update_hint::battery_warning`). Nothing installs without that second press. Only `app`: the system image and the Store are not announced from here.

## App: live, with automatic way back

```
/storage/pocket-ui/
  releases/<version>/      pocket-ui, pocket-core, portmaster-setup.sh
  releases/dev/            what scripts/dev.sh deploy copies
  current  -> releases/0.2.0      switched by one rename()
  previous -> releases/dev
  known-good -> releases/0.2.0    last version that ran POCKETUI_STABLE_SECS (120 s) with a picture (the launcher sets it)
  releases/dev-good/       a known good dev build, copied aside by dev.sh deploy before it overwrites releases/dev
  pocket-ui, pocket-core, portmaster-setup.sh -> current/…   (paths stay stable)
  update-pending           version just installed, until it has presented its first frame
  rolled-back              version the launcher gave up on (shown once, then removed)
  es-fallback              why the launcher opened EmulationStation by itself (shown once, then removed)
```

Install = download → size + SHA-256 against the signed list → tar paths checked → unpacked to
`.staging-<v>` → **both binaries must print exactly `pocket-ui <version>` / `pocket-core <version>`** (catches a
wrong architecture or a broken build before anything changes; a substring test once mistook
11.2.3 for 1.2.3) → every file of the staging folder `fsync`ed → staging renamed into
`releases/<v>` → `previous` → `update-pending` (synced with its folder) → `current` switched → pocket-ui exits with 75.
The order is what a power cut needs: a switch reaches the card's journal within seconds, the programs' bytes only when
the kernel writes them back, up to half a minute later; without the sync a cut in between left `current` pointing at
empty programs. Cut off at any step (`update::crash_tests`: SIGKILL at each step, then the real launcher on what is
left), the next start runs the old version or the new one with `update-pending` and `previous` in place, and the next
try of the install finishes.

An existing release folder is never removed to make room for a build with the same version: the new build uses
`<version>-install-N`, and the links still keep the old build available. Converting an older flat install copies and
syncs its programs before replacing the flat paths with atomic links, so interruption cannot remove its boot path.
The crash tests also cover these two cases, including a rebuilt version that fails on startup.

The ROCKNIX package installer follows the same ordering: sync the staged programs before publishing their folder,
sync the new launcher before replacing it, keep `previous` durable before switching `current`, and replace links by
rename. The first boot entry is published only after its bytes and the boot chain are synced; a final sync precedes
restart. A changed package with the same version gets a separate folder, and rerunning that package reuses it.
`deploy/package/test-power-cut.py` observes every filesystem-command boundary of the real installer under dash,
bash and BusyBox ash, for first install, upgrade and same-version replacement. It checks the existing programs and
boot chain with all writes retained and with unsynced file data lost while the latest directory entries survive.
Together with the process-kill/real-launcher tests this checks interruption and write ordering; it is not a physical
SD-card power-loss test or protection against a card that ignores sync.

`deploy/run-pocket-ui.sh` restarts on 75. If pocket-ui then exits with an error while
`update-pending` is still there, it points `current` back at `previous` and writes `rolled-back`.
A version that fails later, after it confirmed itself, goes back to `known-good` once its crash restarts are used up,
then to safe mode; pruning keeps `current`, `previous` and `known-good` (the whole sequence:
[ARCHITECTURE.md](ARCHITECTURE.md#rescue-path)).
Only the new version itself may remove `update-pending` (the old process is still alive for a
moment after installing; it once confirmed the new version by mistake).

`update-pending` goes away when the new version has **presented its first frame** and the event
loop has turned once more (`update::watch_health`, started at the very beginning of `main`), not
after a fixed time. Before it does, the new version syncs its own folder and checks that the pocket-core next to it
prints its version (`update::release_settled`): a release installed by a pocket-ui up to 0.2.x, which did not sync, could
otherwise be confirmed with a player the power cut had left empty. If that check fails it exits with 124 as below, and
the launcher (or on the Brick card `start.sh`) goes back. A version that hangs before that is killed by the same thread after
`WATCHDOG_AFTER` (20 s): it exits with `WATCHDOG_EXIT` (124), which the launcher treats like a crash
and rolls back. (Before, 10 s of uptime counted as healthy: a build that hung at 13 s stayed.)

Downloads (`update::download`) give curl `--connect-timeout 20`, `--speed-limit 1 --speed-time 30`,
`--max-time` (the source's deadline, `update::source_deadline`, plus 10 s) and `--max-filesize <size from the signed
list>`, without curl's own retries; the poll loop also stops a file that grows past that size, one that has not grown
for 30 s (the next source) and one whose time is up ("too slow"), see [Bounded downloads](#bounded-downloads-and-alternate-sources-03).
Before the download the space on the card is checked (`update::room_for_app`). A kept part that turns out bad
after it was continued is loaded once more from nothing on the same source. The Updates page offers **A: Cancel** while a download runs (the partial file stays and is
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
