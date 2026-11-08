# TrimUI Brick: pocket-ui on its own card (Beta)

An empty FAT card with one folder copied onto it, power on, straight into pocket-ui. No NextUI, no MinUI (their licences are
not ours; nothing of them is read into the card or the repository). Take the card out, put the normal card back, and the Brick
is what it was. Based on the first version under NextUI ([TRIMUI-BRICK.md](TRIMUI-BRICK.md)).

**Beta (0.3.0).** The Brick is a public Beta: it runs from a microSD card of your own, the Brick's stock firmware is not touched, and
new versions arrive over Wi-Fi as a signed update of the card's program folder. It has been tested on one Brick so far, so expect rough edges.

**Boots on a real Brick since 2026-11-03** (card 1, then beta updates 2 to 7 over Wi-Fi), on a Brick that had run NextUI before;
a Brick that never ran NextUI is still to be seen (the TrimUI entry below). Where a statement below says "read", it comes from a
file of the running firmware; the rest of the page notes what only the device showed.

The Brick (read on 2026-11-02, nothing changed): TinaLinux, kernel 4.9.191 aarch64 (sun50iw10), glibc 2.33, ALSA card
`audiocodec` without PipeWire, `/dev/fb0` 1024x768 at 32 bpp (virtual 1024x16384: the page shown is `fb0/pan`), about 1 GB RAM,
input `TRIMUI Player1` (made by the firmware's `trimui_inputd`), `wpa_supplicant` and `udhcpc` from the firmware.

## How the Brick finds the card: two firmware states, one start script

Both entries are three-line files that call `pocket-ui/start.sh` (the one script of ours). We wrote them from reading the two start
scripts on the Brick (2026-11-02, read only); none of NextUI's or MinUI's code is used.

| Firmware state | Read from | What it does with the card | Our entry |
|---|---|---|---|
| **TrimUI's own firmware** (`/usr/trimui/bin/runtrimui-original.sh`, 15.12.2024) | the file on the Brick that runs NextUI (it is kept next to NextUI's replacement). **Not seen booting** | Runs `/mnt/SDCARD/System/starts/*.sh`, then `syslogd`, `bluetoothd`, the LED effect, the USB mode script, and stops `wpa_supplicant` and `udhcpc` and puts `wlan0` down when its Wi-Fi setting is off. Then, if `/mnt/SDCARD/trimui` exists, it looks in `trimui/app` for `keymon`, `trimui_inputd`, `trimui_scened`, `trimui_btmanager`, `hardwareservice` (starts the ones it finds), runs `premainui.sh`, `MainUI`, `preload.sh`, and decides from `preload.sh`'s exit code whether a card UI ran; if not it starts its own `MainUI` from `/usr/trimui` | `trimui/app/MainUI` (calls `start.sh`), `premainui.sh` (exit 0), `preload.sh` (exit 1: when pocket-ui is gone, TrimUI's own menu comes up, not a dead screen) |
| **NextUI's start script** (`/usr/trimui/bin/runtrimui.sh`, 20.10.2026) | the file on the Brick that runs NextUI now. **Not seen booting from our card** | Waits for the card, then runs `/mnt/SDCARD/.tmp_update/updater` if it exists, else the original script above | `.tmp_update/updater` (calls `start.sh`) |

Both entries stand on the card; whichever script the Brick has uses its own. A Brick that was never touched by NextUI has the first,
and a card with only the `.tmp_update` entry would not have started there: that was the first version's mistake.

What differs for `start.sh`: after TrimUI's script the 5 V rail and rumble pin are set, `syslogd`, Bluetooth and the USB mode ran,
and **Wi-Fi may have been stopped**; after NextUI's hook none of that ran and `wpa_supplicant` is still up (the firmware's rc script
starts it at boot). So `start.sh` repeats the pin set-up (harmless twice), starts `trimui_inputd` from `/usr/trimui` itself in both
cases (neither firmware does it for a card without its own helpers), and starts Wi-Fi again unless pocket-ui's own switch put it off.
Not done by either path for us: `keymon` (volume, brightness, power key: pocket-ui reads those keys itself; the 6 s hold is `brick-hw power-watch`), `hardwareservice`,
`trimui_scened`, `trimui_btmanager`. The firmware's own long-press of power was `keymon`'s job: the 6 s hold is now ours (see Power below).

## Build and card

**The Brick has no libwayland and only `libffi.so.7`**: the bundled `libwayland-client` needs `libffi.so.8`, so that comes along too (found by running pocket-ui with the Brick's libraries: without it pocket-ui dies with `NoWaylandLib`).

| Step | Command | Result |
|---|---|---|
| Cores | `BRICK_SYSROOT=<the Brick's libraries> scripts/brick-build-cores.sh OUT [WORK]` (needs zig, make, cmake, git, network) | `OUT/cores/*_libretro.so` for aarch64 and glibc 2.33, `OUT/licences/`, `OUT/sources/` (the exact source of each), `OUT/CORES.txt` |
| Check the cores | `scripts/brick-core-check.sh OUT/cores SYSROOT ROMS` (zig and an aarch64 Docker) | each core loaded and run 120 frames by `brick-core-check.c` with the Brick's own `ld-2.33.so` and libraries |
| Stack check | `scripts/brick-docker-check.sh CARD SYSROOT ROMS` | the card's real programs under the Brick's loader and libraries (aarch64 Docker): the updater runs 25 s, then one game per core |
| Programs | `cargo zigbuild --release --features brick --target aarch64-unknown-linux-gnu.2.33 --bin pocket-ui --bin pocket-core --bin pocket-fb`, and `zig cc -target aarch64-linux-gnu.2.33 -O2 -s -ffile-prefix-map=$PWD=/pocket-ui deploy/brick/brick-hw.c -o brick-hw` (both without build paths: set `CARGO_TARGET_DIR`, then `. scripts/release-rustflags.sh`; the card refuses programs with one) | the 2.33 target links only against symbols the Brick's libc has |
| Network parts | `scripts/brick-remote-parts.sh PARTS` | Tailscale, curl (pinned upstream builds) and dropbear (built with our patch), licences, sources ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| Cheat data | `python3 -I scripts/brick-cheats-data.py APP [--archive UPSTREAM.tar.gz]` (also run by the card builder) | pinned, unchanged `.cht` subset for the 26 Brick systems with upstream entries, packed into the one file `APP/cores/cht.pack`; inventory and CC BY-SA 4.0 notices |
| Card | `BRICK_PARTS=PARTS [BRICK_UPDATE_URL=…] scripts/brick-make-card.sh OUT BINS LIBS CORES VERSION` | `OUT/card/` (the folder) and `pocket-ui-trimui-card-<version>.zip`; fails when anything the updater looks for is missing |
| Update channel | `scripts/brick-publish.sh OUT/card --channel beta` | the program folder as a signed update in the private channel ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)) |
| Messages | `PYTHONPATH=<Pillow> python3 scripts/brick-messages.py deploy/brick/msg` | the three pictures (starting, files missing, does not start), each as 1024x768 (Brick) and `-wide` 1280x720 (Smart Pro; start.sh picks by the screen, pocket-fb does not scale); committed, rerun only when a text changes (the card test redraws them and compares) |
| Tests | `scripts/test-brick-card.sh`, `cargo test --features brick --test brick_hwroot` | see "What is proven" |

Card folder (copy the contents to the root of an empty FAT32 card):

    .tmp_update/updater            entry for NextUI's start script (3 lines)
    trimui/app/MainUI, premainui.sh, preload.sh   entry for TrimUI's own firmware (3 small files)
    pocket-ui/start.sh             the fixed part (deploy/brick/boot.sh): runs app/start.sh, takes a failed update back
    pocket-ui/app/                 the program, replaced as a whole by an update ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md));
                                   the folders below marked app/ are in it: start.sh (deploy/brick/start.sh), RELEASE, update-url
    pocket-ui/storage/, logs/, authorized_keys    the player's (made at the first start; not in the zip)
    pocket-ui/app/bin/             pocket-ui, pocket-core, pocket-fb, brick-hw; tailscale, tailscaled, curl, dropbearmulti
    pocket-ui/app/lib/             libwayland-client/-cursor/-egl, libxkbcommon, libffi.so.8 (Debian bookworm arm64; pocket-ui loads them at run time)
    pocket-ui/app/cores/cht.pack   the bundled cheats as ONE file, available offline; database/MANIFEST.json pins source, checksum, systems and size
    pocket-ui/app/cores/           the base cores plus the additional software and 3D cores listed in TRIMUI-BRICK-CORES.md
    pocket-ui/app/hwroot/usr/bin/  the stand-in helpers (volume, pactl, brightness, systemctl, wifictl, nmcli, brick-power-off, brick-tailscale, brick-ssh, tailscale)
    pocket-ui/app/asoundrc         ALSA: the "pipewire" device pocket-ui and pocket-core play to is the sound chip, mixed (dmix), 21 ms
    pocket-ui/app/msg/             the on-screen messages; app/es_systems.cfg: the systems
    pocket-ui/app/licences/, sources/  licences, the network parts' source (the cores' source is in pocket-ui/sources/cores/ beside app/, and in the update channel), VERSION, BUILD-INFO.txt; logs/boot.log (written at every start)
    Roms/{gb,gbc,gba,nes,snes,psx,megadrive,mastersystem,gamegear}/ the games go here (a game's own save file, .srm, is written next to it)
    Roms/<system>/.media/          covers: <game file name without extension>.png (also where pocket-ui keeps covers it fetches)
    Saves/savestates/              pocket-ui's save states (bound onto /storage/roms/savestates; nothing is saved in the Brick)

## Cheat database in card and OTA

The Brick firmware has no libretro cheat database. The card builder therefore runs `scripts/brick-cheats-data.py`,
which verifies the SHA256 of libretro-database commit `4de37d2c8042d9d0f608c43006c9b35438c536ce` (the ROCKNIX pin)
and packs only the `.cht` files for systems with a core on this card into the single file `app/cores/cht.pack`
(`src/cheat_pack.rs` documents the layout: an index per console folder, every file stored unchanged and deflated on its
own, with its CRC-32). No ROMs or unsupported console databases are included. The helper downloads the archive once into
`downloads/`; an offline build can set `BRICK_CHEATS_ARCHIVE` to a copy of that exact tar.gz. A wrong checksum or missing
upstream notices stops the card build.

**Why one file (08.11.2026):** the first version put the 23,174 files into `app/database/cht`. On the card's FAT with
32 KB clusters that is 837 MB for 165 MB of text, and the OTA install of 0.3.0.1 → 0.3.1 took 10 min 12 s. Now the card
and the OTA carry one 31.4 MB file. A lookup reads the index of one console folder and the few entries that match
(`cheats::find_all`, same matching as before), never the whole file; a damaged or cut-off pack finds nothing instead of
wrong codes. The folder form (`app/database/cht`, `/tmp/database/cht`, ROCKNIX's `/usr/share/libretro-database/cht`) is
still searched too.

**In the OTA the pack is an asset package of its own** (like a core, `scripts/brick-update-pack.py`): downloaded once,
checked, and moved on unchanged by every later update, so a regular app update is ~23 MB again. It is in `cores/` on
purpose: a Brick on 0.3.0.1 checks the asset list with its own old code (and `boot.sh`), which only accepts `cores/`,
`bios-data/` and the six helpers, so a new folder would make the update refuse. `cores/` is bound to `/storage/cores`;
nothing there reads a file that is not named `<core>_libretro.so`.

The database is under **CC BY-SA 4.0**, as stated by upstream's LICENSE. `app/licences/libretro-database/` retains
the complete LICENSE, README credits and a source/selection attribution. Cheat contents are unchanged; the selected
collection keeps CC BY-SA 4.0. On startup the three bundled notices are also refreshed in the existing
About licence folder when changed, so older cards receive them after OTA; other files are preserved. `app/database/MANIFEST.json`
records the commit, source archive URL, checksum, systems, file count, raw byte size and the pack's name and size. The pinned
subset has **23,174 cheat files / 165,144,228 bytes** (157.5 MiB as text) across 26 system folders; the pack is
**31,367,177 bytes** (29.9 MiB). No database copy or download runs during boot or menu navigation.

Packaging regressions: `python3 -I scripts/test-brick-cheats-data.py`, `python3 -I scripts/test-brick-update-pack.py`,
and `BRICK_LICENCE_TESTS_ONLY=1 bash scripts/test-brick-card.sh` (the real startup under dash and BusyBox ash).

## The start script

`start.sh` shows "starting" at once, checks the card (a missing programme, library, core, message or `es_systems.cfg` ends on the
"files missing" picture; if even `pocket-fb` is missing, on the raw frame, else on grey: never black), does the firmware's own
hardware set-up for what we use (5 V rail gpio107, rumble off gpio227, `trimui_inputd` from `/usr/trimui`), binds the card's
folders onto `/storage` (the only change in the Brick's flash is the empty folder `/storage`, made once; if a bind fails it stops with the "does not start" picture, so nothing is ever saved into the flash), builds the stand-in
root, then runs `pocket-fb` and `pocket-ui` and starts them again when they end. Three endings within 20 s each show "does not
start" for 10 minutes (the reason is in `pocket-ui/logs/boot.log`), then the Brick switches itself off. `BRICK_*` variables move
every path so the whole script runs on a computer (`scripts/test-brick-card.sh`, under dash and BusyBox ash).

## What NextUI's helpers do today, and what replaces each

Read on the running Brick (processes, open files, the firmware's own `keymon`, `hardwareservice` and start scripts; nothing of
NextUI's was copied). Everything that is a sysfs file or an ioctl of the firmware is plain Linux and ours to use.

| NextUI part | What it does on the Brick | Replacement |
|---|---|---|
| `trimui_inputd` (the firmware's; TrimUI's script would start it only from the card's `trimui/app`, where we have none) | `/dev/uinput` device `TRIMUI Player1` from the pad's keys (A=305, B=304, X=308, Y=307, L/R 310/311, L2/R2 312/313, Select/Start 314/315, Menu 316, D-pad as a hat) | **kept**: `start.sh` starts the firmware's own binary from `/usr/trimui/bin` with its library path, as the firmware's start script does |
| `keymon.elf`: volume keys | event0 `sunxi-keyboard` (KEY_VOLUMEUP/DOWN 115/114): steps the volume, stores it | pocket-ui opens every event node with such keys (`pad.rs`: `Button::VolUp/VolDown`) and sets the volume through the `volume` stand-in |
| `keymon.elf`: power key | event1 `axp2202-pek` (KEY_POWER 116): short = suspend, long = power off | pocket-ui's own power key (`POCKETUI_POWER_KEY=1`) and power menu; the `systemctl` stand-in does suspend / poweroff / reboot |
| `keymon.elf`: brightness | the keys' combination changes the panel through `/dev/disp` | pocket-ui's brightness (Start + volume) writes a backlight file; `brick-hw watch` follows it with the display ioctl (0x102; reading it, 0x103, answered `16` on the real Brick) |
| `keymon.elf`: rumble, DIP switch | motor on gpio227 / `/sys/class/motor/voltage`; DIP switch gpio243 | pocket-core's rumble is not wired (nothing on the host can show it); the updater only switches the motor off |
| `audiomon.elf` | applies the volume to ALSA (`digital volume` 0..63; `HpSpeaker Switch`, `Headphone Switch` on), keeps `.asoundrc`, jack changes | `volume N`, `restore`, `mute` and `unmute` share a lock and the volume profile (gentler steps below 50%; unchanged above). The exact target is in `brick-volume-curve3`; curve 2 remains a rollback value; `brick-volume` is its OTA-predecessor compatibility value. `pactl get-sink-volume/-mute` reports the saved target when it matches the chip and the slider marker; `RealHw` reads these state files directly. The separate speaker-amp override does not freeze jack/Bluetooth volume. See [the curve and rollback limits](TRIMUI-BRICK-HW.md#volume-curve-and-mute). **The jack is not followed** |
| `batmon.elf` | reads `power_supply/axp2202-battery/capacity` and `axp2202-usb/online`, low-battery LED, charging stop | pocket-ui reads the battery itself (`POCKETUI_BATTERY`, the fake root links `battery` to `axp2202-battery`); its low-battery guard saves and powers off at 2 %. **The low-battery LED is not done** |
| `suspend` script | saves the mixer, stops `bluetoothd` and `wpa_supplicant`, `echo mem > /sys/power/state` up to 3 times, starts them again | `systemctl suspend`: screen off, Bluetooth rfkill off, Wi-Fi client stopped, `mem` (retried), then everything back; **UNPROVEN** |
| `poweroff_next`, `reboot_next` | sync, firmware poweroff / reboot | `systemctl poweroff|reboot`: `sync`, then BusyBox `poweroff` / `reboot` |
| `rfkill.elf`, `syncsettings.elf`, `nextval.elf`, `show2.elf` | Wi-Fi/Bluetooth switches, NextUI's own settings and boot picture | `wifictl enable|disable` (rfkill `soft` of the `wlan` entry and `ifconfig wlan0 up|down`); the rest is NextUI's own and has no use here |
| `governor.sh` | CPU governor and frequency range per game | **not done** (the kernel's default governor stays; TrimUI's `preload.sh` sets `ondemand`, 408 MHz to 2 GHz, after its UI) |
| Wi-Fi | NextUI keeps `wpa_supplicant` running; TrimUI's own start script stops it (and `udhcpc`, `wlan0`) when its own setting is off (the Brick read has `"wifi":0` while NextUI runs Wi-Fi anyway) | `start.sh` starts it again unless pocket-ui's switch is off (`wifictl enable|disable`, state in `pocket-ui/storage/.config/pocket-ui/wifi-off`); **the networks saved in the Brick's flash work as they are**. Joining, forgetting, scanning and the radio switch go through an `nmcli` stand-in over `wpa_cli` (output checked against pocket-ui's own parsers with a fake `wpa_cli`; never run against the real supplicant) |
| Bluetooth, USB mode, LEDs | the firmware's start script: `bluetoothd`, `usb_device.sh`, LED effects | Bluetooth controller service prepared (not device-verified), see [Bluetooth](TRIMUI-BRICK-BLUETOOTH.md); no Bluetooth audio or USB mass storage mode yet |

pocket-ui reaches the stand-ins through its existing hardware seam: `RealHw::with_root` (what its own tests use), switched on by
`POCKETUI_HW_ROOT` in a `--features brick` build (the only change in `src/`, 6 lines in `system_ctl.rs`). The root holds sysfs
files (`sys/class/backlight/panel`, `power_supply/battery`, `rfkill`), `usr/bin/` with our stand-ins, and `run/systemd/system`
(so pocket-ui offers power actions). The same folder is first on `PATH` for the commands pocket-ui starts by name.

## PlayStation and the BIOS

`pcsx_rearmed` is built like the others (same commit as the ROCKNIX images, reproducible: two builds, same bytes; no physical CD drive support, `-z nodelete`).
Games go in `Roms/psx` (`.cue`/`.bin`, `.chd`, `.pbp`, `.m3u`, `.iso`). **With no BIOS on the card nothing is said and nothing fails:** pocket-ui lists PlayStation
without a BIOS requirement (`hostsys.rs`) and the core runs on its built-in HLE BIOS (the load test ran exactly that, no BIOS file anywhere). HLE is
less compatible than a real BIOS (some games do not start or glitch, no memory-card or CD-player menu). A real BIOS (`scph5501.bin` or similar, not on
the card, not ours to ship) put in `Roms/bios` is used by the core. Not run: a real PlayStation game, speed on the Brick's CPU.

## Power, covers, GBA core, governor

- **Power.** Short press: pocket-ui's own (Settings, System, Sleep & power: sleep or screen off). Held 0.8 s: its power menu (Power off there
  saves and quits the game first, then calls the `systemctl poweroff` stand-in). **Held 6 s (our own helper, replaces keymon's "6 seconds power off"):**
  `brick-hw power-watch` finds the key by its input name (`axp2202-pek`) and runs `hwroot/usr/bin/brick-power-off`: the running `pocket-core` gets SIGTERM
  (it writes its save and quits), is waited for up to 10 s, and on up to 120 s while pocket-core's mark (`/tmp/pocket-core.saving`) says that very
  game is still writing its exit save (a big moment on the `sync`-mounted card can take longer than 10 s); then SIGKILL (a hung game must not keep the
  Brick on), `sync`, then `poweroff`. It does
  not ask pocket-ui (which may be what hangs), so no resume note is left. Host-tested with a fake key (a FIFO of input events), a fake game that saves
  slowly on SIGTERM, one whose marked save outlasts the 10 s, one that only claims to save (killed at the limit), a stale mark, one that ignores it,
  none running, and the whole chain: the log shows "game: saved" before "poweroff". The key itself, the 6 s feel
  and the real PMIC behaviour are only proven by a boot.
  **Screen off follows the press at once:** `brick-hw watch` used to look at the backlight file twice a second, so the panel went dark or came on up to 0.5 s after pocket-ui wrote it (that was the slow short press); it now follows the file with inotify (a few ms; measured on the host with the display device replaced by a file: one ioctl per write, 370 ms apart, instead of on a fixed 500 ms grid). The sound goes on behind a dark screen (`dark_game`), so there is nothing to fade; pocket-core now logs its underrun count at "screen off" and "screen on": if the count stays put across a click, the click is not a buffer underrun and comes from the hardware (panel/amp supply), if it jumps, it is the core.
- **Covers** lie next to the games: `Roms/<system>/.media/<game file name without extension>.png`, bound onto pocket-ui's cover folder
  (`<data>/covers/<system>`), so a `.media` folder from another card works as it is, and covers pocket-ui fetches land there too.
- **GBA** asks for gpSP (`brick_profile::core_for`: mgba becomes gpsp on the Brick; with the standalone flag Nestopia stays for NES). gpSP is built with
  its aarch64 dynarec (`CPU_ARCH=arm64 HAVE_DYNAREC=1`). **zig's `__clear_cache` does nothing useful on aarch64-linux** (zig 0.17): a two-instruction JIT test died with SIGILL on its first call, and gpSP's
  first generated block did too (stale instruction cache). The build therefore gives gpSP, and only gpSP, `scripts/brick-clear-cache.h` (`-D__clear_cache=brick_clear_cache -include`), the usual
  `dc cvau` / `ic ivau` / `isb` sequence by CTR_EL0; with it the same JIT test ran 20000 times without a wrong result and gpSP runs 120 frames of a test GBA
  program on the Brick's own libraries (Docker, aarch64). pcsx_rearmed's ari64 has its own sync and needs nothing. The 59.98 fps and 1.35 ms per frame were measured on the real Brick with NextUI's own gpSP, not
  with this build; mGBA is on the card as the other choice.
- **Governor.** The fix from the first version is in: `standalone.rs` settles to `performance` on the Brick instead of `ondemand`
  after a game. This card never sets `ondemand` itself.
- **Not taken from the NextUI library work:** that work was for NextUI's card (its `Roms/<Name> (<TAG>)` folders as systems, `Saves/<TAG>`
  copied in at the first start, `hostsys::route`, "cannot run" notices). None of that fits a card of our own, so it is not merged; the covers
  idea is done above with two lines of `start.sh`.

## Settings pages the card keeps

With NextUI's card all of these stay hidden (NextUI owns them); `POCKETUI_BRICK_STANDALONE=1` brings them back. Host tests
(`tests/brick_hwroot.rs`, `scripts/test-brick-card.sh`, unit tests in `app_settings_groups.rs`, `soft_screen.rs`, `device.rs`).

| Page | How it works on the card | Only the device can show |
|---|---|---|
| **Clock and time zone** | The card carries the IANA zone data (`pocket-ui/zoneinfo`, public domain). The choice is the zone's *name* in `storage/.config/pocket-ui/timezone` (a FAT card takes no symlink); `start.sh` rebuilds `etc/localtime` and `usr/share/zoneinfo` in the stand-in root from it and exports `TZ` for every program it starts; a name the card has no file for falls back to the firmware's zone, then UTC. Setting the clock by hand writes the system clock and `/dev/rtc0`. Network time: one try in the background at start (`systemctl restart systemd-timesyncd` stand-in = one shot of the firmware's `ntpd -q`), and the page can ask again; `timedatectl show` stand-in reports it. A wrong clock makes every TLS download fail, so this comes first | whether the firmware has `ntpd`; the RTC write |
| **Night light** | The Soft backend (no compositor gamma on `pocket-fb`): pocket-ui tints its own frames; in the CPU copy (`POCKET_CORE_SOFTWARE`) `soft_screen` passes each pixel through the look's three 256-entry tables (nothing is paid while the look is neutral); the GL path keeps the multiply layer pocket-core already draws. The fade, the schedule and the zone work as on every device | how the warm tint looks on this panel; CPU cost of the tables at 1024x768 |
| **Sleep & power** | The Brick counts as *screen off only*, like the Snapdragon 8 Gen 2 handhelds: short Power = screen off, the game pauses behind a dark screen (backlight 0), any button wakes it; hold = the power menu (Power off saves and quits the game first); "Switch off after" saves the game and powers off after N hours dark on battery. **No `mem` suspend is ever requested**, so a firmware that does not wake cannot lose a game. If suspend is proven later (read-only checks on the device), the one switch is `Device::screen_off_only` | the wake from a dark screen on the real panel; battery use while dark |
| **Starter pack** | Free games into `Roms/<system>` (nine games for gb, gbc, gba, nes, snes, megadrive: all folders of the card's shelf, tested). Work files in `storage/.config/pocket-ui/starter` on the card, SHA-256 checked before anything is placed, one atomic write. Needs a network (`ip` stand-in), the clock and `curl` with a CA bundle: the card's `curl` is the one from `feat/trimui-brick-remote` (`pocket-ui/bin/curl`, first on `PATH`). **Without that curl the page says "no network"/"didn't finish", nothing is damaged** | a download over the Brick's Wi-Fi |
| **RetroAchievements** | pocket-core's own rcheevos (`pocket_rcheevos`), sign-in on the gamepad keyboard, unlock cards, offline cache in the data dir on the card (survives power-off; synced at the next game start with a network). Network through the same `curl` and clock | sign-in and an unlock over the Brick's Wi-Fi; a core that exposes memory on the aarch64 builds |

## Not done, on purpose or for lack of time

- Settings still hides Store and Tools on this card (NextUI's list). The pages that are back are in "Settings pages the card keeps" above, plus Wi-Fi (checked on the empty shelf: "Needs Wi-Fi. Connect in Settings"), Remote access, Tailscale and Updates ([TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md)); `POCKETUI_BRICK_STANDALONE=1`, set by `start.sh`.
- Sound: `pocket-ui/asoundrc` (dmix, 21 ms) plays without dropouts in long runs (beta 7: a 22-minute SNES run at 60.01 fps, 0 underruns);
  the latency from a button to the sound is not measured.
- The headphone jack is not followed (plugging in or out during a game), no low-battery LED, no USB storage mode.
- The public card's source: `BRICK_SOURCES_APART=1` puts the cores' source and pocket-ui's own (`scripts/source-archive.sh`, from the
  exported tree) into `pocket-ui-trimui-sources-<version>.zip`, published next to the card zip ([UPDATE-CHANNEL.md](UPDATE-CHANNEL.md#the-brick-card)).

## Cores and licences

Same upstream commits as the ROCKNIX images ([NOTICE.md](../NOTICE.md), `scripts/core-licences.txt`), built by zig as a cross
compiler for glibc 2.33 (no libstdc++ needed, only libc, libm, libpthread, libdl). Linked with `-z nodelete`: pocket-core drops a core
with `dlclose` when a game ends, and the three C++ cores (zig's libc++ is inside each) crashed there until the flag kept them mapped.

| Core | Systems | Commit | Licence | For free redistribution |
|---|---|---|---|---|
| gambatte | GB, GBC | `d9d6cd0` | GPL-2.0-only | The same as on every other image (cannot be combined with pocket-ui's Apache-2.0 crates in one binary; it is loaded at run time) |
| mgba | GBA | `e31759b` | MPL-2.0 | fine |
| nestopia | NES | `b08b473` | GPL-2.0-or-later | fine |
| pcsx_rearmed | PlayStation | `da2cb8e` | GPL-2.0-or-later (its LGPL-3.0 GNU Lightning part is not built here: the aarch64 build uses the ari64 dynarec) | fine |
| gpsp | GBA (the default on the card) | `5819380` (upstream master, 2026-11-03; not on ROCKNIX's list) | GPL-2.0-or-later (files say "or later", COPYING is GPLv2) | fine |
| fceumm | NES (on the card to choose; Nestopia stays the default) | `e0f3086` (same) | GPL-2.0-or-later | fine |
| picodrive | Mega Drive, Master System, Game Gear, Sega CD and 32X | `733c711` (the ROCKNIX commit) | **non-commercial** (`COPYING`: "Redistributions may not be sold, nor may they be used in a commercial product or activity"; modified versions must come with their complete source) | **free redistribution is allowed**, selling is not: the same class as snes9x. The card's source archive holds the submodules' files (libchdr, zstd, lzma, emu2413, dr_libs), and its licence folder PicoDrive's `COPYING` and `AUTHORS`. Built with the AArch64 SH2 recompiler and explicit instruction-cache synchronization (the same helper as gpSP) |
| snes9x | SNES | `890b5d4` | **non-commercial** ("freeware for PERSONAL USE only") | **free redistribution is allowed** ("Permission to use, copy, modify and/or distribute Snes9x … for non-commercial purposes, is hereby granted without fee", with its licence text), selling is not. Shipped, like ROCKNIX, NextUI, MinUI (snes9x2005), Knulli and Batocera do: see "The non-commercial cores" below |

### The non-commercial cores (decision of 2026-11-05)

snes9x (SNES), PicoDrive (Mega Drive, Master System, Game Gear, Sega CD, 32X) and FBNeo (arcade) are under licences that allow free
redistribution but forbid selling. They stay on the card, the way every comparable project ships them: the Brick's own NextUI card
(snes9x, PicoDrive, FBNeo), MinUI on the same device (snes9x2005, PicoDrive), ROCKNIX, Knulli, muOS and Batocera. They are separate
programs loaded at run time, not part of pocket-ui's GPL work, and the card says so (`licences/README.txt`, `CORES-LICENCES.txt`).
What their terms ask, and how the card does it:

- **Their licence text, word for word:** `licences/cores/snes9x/LICENSE`, `picodrive/COPYING`, and FBNeo's `src/license.txt` (its
  top-level `LICENSE.md` is only a link; `brick-make-card.sh` takes the text from FBNeo's source and refuses a card without any of the three).
- **Source:** the exact commit of each, in the card's source zip and the update channel (`CORES-LICENCES.txt` names the commits).
- **Never sold:** the card and pocket-ui are free; nobody may sell the card with these cores on it (the licence files say so).
- **FBNeo: "You may not ask for donations to support your work on any project that uses the FB Neo source code."** pocket-ui
  asks for none today (no donation, sponsor or tip links in the repository or on the website). As long as FBNeo is on the card, it stays that way;
  adding a donation link means taking FBNeo off the Brick card first.

A GPL SNES core instead (Mednafen's Supafaust, which NextUI and MinUI also carry for this chip) would need its own speed
measurement on the Brick; not needed for the licence, so not done.

## What is proven, and what only a real boot can answer

Proven on computers: the cores load and run 120 frames each (gambatte 160x144, mgba 240x160, nestopia 256x240, snes9x 256x224, pcsx_rearmed 640x480 on a small original PlayStation program)
with the Brick's own `ld-2.33.so`, `libc`, `libm`, `libpthread`, `libdl` (copied off the Brick, run natively on aarch64);
`pocket-ui` resolves against them and, with the card's own updater, runs 25 s on `pocket-fb` into a file screen, after which each core plays 6 s through `pocket-core` (CPU copy, exit 0; the timing in that container says nothing about the Brick's CPU); the core build gives the same bytes twice (`CORES.txt`); the updater (dash and ash) ends on a
picture when a part is missing, shows "does not start" after three quick endings, restarts a healthy pocket-ui and hands it the
environment; the stand-ins answer pocket-ui's own hardware seam (volume, mute, brightness, battery, Wi-Fi switch, power);
`brick-hw` builds without warnings; `bl-get` read the real panel (16 of 255) through the ioctl.

Only a real boot on a spare card answers: whether the firmware starts the updater from an otherwise empty card and what is on
screen first; whether pocket-fb draws to the page that is shown (`fb0/pan` was `0,768` under NextUI); the colours; whether
`/storage` can be made and bound at that point of the boot; whether `trimui_inputd` works without the firmware's other helpers and
X/Y come out right; whether the volume and brightness keys, `brick-hw`'s write ioctl and the speaker work; suspend and wake;
Wi-Fi coming up by itself; speed of the CPU copy and sound; and the stand-in power-off. The card's first boot is the first
time any of this runs on the Brick.

## Additional software cores

The standalone card also builds Neo Geo Pocket/Color (`beetle_ngp`), WonderSwan/Color (`beetle_wswan`),
PC Engine (`beetle_pce_fast`), Lynx (`handy`), Atari 2600 (`stella`), Atari 7800 (`prosystem`) and
arcade/Neo Geo/CPS1–3 (`fbneo`). See [core builds and verification](TRIMUI-BRICK-CORES.md).
FBNeo is noncommercial, with source and licence conditions; its matching ROM sets and any BIOS must
be supplied separately. DS and Saturn, and the other additional software systems, are described in the second software-core addition
in that document.

### 3D cores: N64, PSP, Dreamcast

`mupen64plus_next` (system `n64`), `ppsspp` (`psp`) and `flycast` (`dreamcast`) are built for GLES 3 (no Vulkan) and draw through
pocket-core's own GLES context on the Brick's EGL (`src/brick_gles.rs`, [TRIMUI-BRICK.md](TRIMUI-BRICK.md) "3D cores"); frames are read back to
the CPU picture. They link the Brick's own `libGLESv2.so.2` / `libEGL.so.1` (never shipped). Folders `Roms/n64`, `Roms/psp`, `Roms/dreamcast`
(extensions as ROCKNIX uses: n64 `.z64 .n64 .v64 .zip .7z`, psp `.iso .cso .pbp .chd`, dreamcast `.cdi .gdi .chd .m3u .cue`).
**Dreamcast BIOS:** the owner puts `dc_boot.bin` and `dc_flash.bin` (from their own console) in `Roms/bios/dc/` (in pocket-core's `/storage/roms/bios/dc`);
the card never contains them. Flycast also starts without them on its built-in replacement, with lower compatibility. The speed on the GE8300 is
not measured; only the device can say.
