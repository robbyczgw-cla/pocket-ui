# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may change anything).

## [0.1.1] - 2026-10-28

### Changed

- A newer ROCKNIX no longer blocks the install. The Ports installer and `install.sh` accept the tested ROCKNIX 20261001
  and anything newer, say "tested with 20261001; your ROCKNIX is newer" and let pocket-ui's own compatibility check
  (`pocket-ui --check-os-update /`) decide; that check still blocks. An older ROCKNIX is asked to update first. The card
  drop-in never looked at the version.
- The install guides are honest about the start: ROCKNIX on the Mini V2 needs its own one-time bootloader setup since
  September 2026. README, getting started, the install guide and the card zip's README point to ROCKNIX's own guide
  for the Mini V2 instead of saying "flash and switch on".
- The device list is the same everywhere: the Mini V2 is the target; the Pocket 5, Flip 2, first Mini, Pocket 6, Nova
  and the AYN Odin 2 family are experimental.
- A hint how to copy files into `roms/ports` with ROCKNIX's file sharing (Samba) from Windows and macOS.

## [Unreleased]

### Added

- Install with two files on the official ROCKNIX card: copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the card's
  `ROCKNIX` partition on a PC, start twice, done (`scripts/make-fat-install.sh`, `deploy/fat/`; uses ROCKNIX's
  `/flash/mount-storage.sh` hook, waits for STORAGE to be grown, checks the tar, leaves ROCKNIX untouched and a log
  on the card if anything fails). Tested against a fake card only.
- New menu sounds: warm, tactile hardware samples (wooden tick, cartridge click, drawer, knob) instead of synthesised
  sine beeps, and a default level of 70 % (was 30 %) so they match a game's loudness (`assets/ui-sounds/`).
- A calm start: no kernel text on the screen any more (`quiet` on the SD image's kernel command line;
  `scripts/boot-quiet.sh` for a card set up before, with a backup and an undo), a boot screen from systemd's start
  until sway takes over (pocket-splash: one shelf spine whose stripe crossfades through the console colours), and the
  shelf fading in from black after a power-on ([docs/BOOT-SPLASH.md](docs/BOOT-SPLASH.md)).
- The update channel's address: pocketui.xyz for the signed list, GitHub Releases for the files; https only.
  `scripts/release-channel.sh` makes and signs the list ([docs/UPDATE-CHANNEL.md](docs/UPDATE-CHANNEL.md)).
  Nothing is published yet.
- The licence texts travel with the programs: the install package and the SD card image carry `LICENSE`, `NOTICE.md`,
  the licence and notice files of the Rust crates (`THIRD-PARTY-LICENSES.txt`, from `scripts/third-party.sh --texts`),
  the crate list (`THIRD-PARTY-CRATES.md`) and the fonts' SIL Open Font License (`FONTS-OFL.txt`) in a `licences/`
  folder (`scripts/licence-files.sh`). The package copies it to `/storage/pocket-ui/licences`; Settings → System →
  About has a Licences row that names that folder. `deploy/package/test-package.sh` and `scripts/image/check.sh`
  assert the files.
- `net.log` in the logs folder (and in Report a problem): what NetworkManager, iwd and Tailscale said since the start,
  and the names (never the passwords) of the saved Wi-Fi networks, so a network that was missing after a restart can
  be explained.
- A Wi-Fi test bench (`scripts/wifi-bench/`, [docs/WIFI-BENCH.md](docs/WIFI-BENCH.md)): ROCKNIX's own NetworkManager
  and iwd and the real pocket-ui against simulated radios and access points; eight scenarios (roaming mesh, a hotspot
  that goes away and comes back, a hotel sign-in, a wrong password, forgetting a network, a blocked radio). It
  simulates the radio; it is not a test on a device.
- Experimental support for the AYN Odin 2 family (Odin 2 Mini, Odin 2, Odin 2 Portal; ROCKNIX SM8550). The Odin 2
  Mini was installed with the install package and smoke-checked on a real device on 2026-10-27 (driven over the command
  pipe, no button or touch pressed: the install checks, the first-start screens, the shelf, a GBA game with the
  pause menu, screen off and wake); the Odin 2 and Odin 2 Portal were not tried. [docs/DEVICES.md](docs/DEVICES.md#ayn-odin-2-family-2026-10-27-experimental) says what is still open.

- Pixel art in the interface: small pictures above the words of empty pages (no games, nothing played, no
  screenshots, no match, an empty store, no network, a hotel's sign-in, no history, no favourites), icons for the
  Settings categories, the handheld on first start and About, a picture of the Mini V2 with its controls named on the
  manual's Controls page, and stickers for milestones in the Year in review
  ([assets/ui-art/SOURCES.md](assets/ui-art/SOURCES.md)).

- Pictures for the SuperGrafx and the Pokémon mini on the shelf, made from text prompts alone (no photo: none of them is
  public domain), and a fact sheet each ([assets/systems/SOURCES.md](assets/systems/SOURCES.md)). They are in the style of the
  other pictures, not exact pictures of the hardware.

### Changed

- The manual's longest pages are split in two, so none grows past what reads well: Settings (Look & feel, Playing,
  Games) and [Connections and System](docs/user/settings-system.md); Controls and
  [Controls in Settings, setup and on the touch screen](docs/user/controls-settings.md); Your library and
  [Curator](docs/user/curator.md). Nothing was removed; links to moved sections point at the new pages.
- **Switch off after sleeping is Off by default** (it was 3 h): a sleeping Pocket stays asleep, as on other handhelds.
  A Pocket that has saved settings keeps its choice; if yours still switches off after 3 hours asleep, check
  Settings → System → Sleep & power once and set the row to Off.
- Power off and Restart clear the clock alarm and log it first (`logs/shutdown.log`, also in Report a problem), and the
  start clears one left over: a Pocket must not power itself back on after Power off.

- The English of the menus, messages, manual and README was checked and made consistent: British spelling throughout
  (Favourites, colour, catalogue), "45%" without a space, Settings paths written with "›", and clearer wording in many
  places. Nothing works differently.

### Fixed

- Settings: the line under each category ("Look, language, text size, ...") was cut with "…" at Medium and Large text.
  It now wraps to a second line, whole, in English and German; the rows get taller and the page scrolls with the
  selection when five no longer fit. Normal text looks as before.
- Wi-Fi after a boot: quick settings and Settings → Wi-Fi now read one shared picture of the radio, the connection and the
  last scan, and one worker does everything that changes them, so neither shows what the other has since changed. The
  radio switch decides from a fresh read (not from what the row last showed), stores `wifi.enabled` only after the radio
  was read back in the state asked for, and a switch that did not work says so calmly and keeps showing the radio's real
  state. A radio that the boot hooks have blocked shows "Wi-Fi is off" with A to turn it on (never "No networks found");
  a scan that failed says "Couldn't search for networks. Try again (X)." and only a scan that worked and found nobody says
  "No networks found". When the radio comes up after being off, or a scan failed, one more scan follows by itself. If Wi-Fi
  is on in your settings but the system blocked the radio while starting, pocket-ui switches it on once by itself (never when
  you switched it off). A switch no longer waits behind a running scan, a wish that could not be stored is said and stored
  later, and the Settings list's Wi-Fi value comes from the same picture.
- Wi-Fi in the first-start setup: after a wrong password the refused key stayed stored, iwd kept connecting with it, and
  the page offered the network as "Saved" and later showed "Connected" while nothing was connected. A refused
  password is now dropped at once, the keyboard opens again with "Wrong password. Try again.", "Connecting…" is no
  longer shown as "Connected" (and after 60 seconds the page says the network isn't letting the Pocket in), and
  forgetting a network twice is harmless. Found on 2026-10-27 when the SD card image was started on a fresh card for
  the first time.
- Wi-Fi hotspot that goes away and comes back (a phone's hotspot, a travel router): the watchdog's try while the access
  point was gone made iwd leave it alone for a long time after its return (offline for 95 seconds in the bench; now
  back in 4 to 21 seconds). Only networks the radio has heard since the drop are tried, and a saved network heard
  again is joined at once. Settings → Connections reads the connection afresh and follows the net state, so its row no
  longer shows the old network.
- Sign in… on a hotel or café network said "No answer" the first time: the helper gave up after 5 seconds, exactly
  when the system's own second lookup was due. It now waits 12 seconds. Why the first lookup is slow on such a
  network is not settled.
- ROCKNIX's own volume and brightness box (black, top centre) no longer sits over pocket-ui's or pocket-core's
  display while they are in front; it stays for RetroArch, standalone emulators and apps, where we draw nothing.
  Uninstalling puts it back. Seen working on the Mini V2 on 2026-10-28.
- The Bluetooth page's wording says "devices", not only "controllers": headphones and speakers are listed there too.
  Wording only.
- Bluetooth audio no longer sounds broken (f042c1f, 9730d7a, c316e2c; [HOST.md](docs/HOST.md#bluetooth-audio-2026-10-25)).
  Games on Bluetooth headphones were heard as extremely distorted on the Mini V2 on 2026-10-27, the menu sounds
  distorted and quiet; the speaker was fine. Cause: pocket-core and the shelf asked for a 10 ms ALSA buffer, shorter
  than the 21 ms cycle of the Bluetooth sink's graph (ROCKNIX's quantum 1024), so PipeWire padded every cycle with
  silence; the pitch control also steered on every raw reading of the queue and wobbled, and glided after a start and
  after the pause menu. The buffer now fits the cycle that runs (10 ms only at the speaker's 256, 32 ms otherwise),
  and the pitch control steers on a one-second average, waits a second after the start and starts afresh after the
  pause menu. **Measured off the device** with `scripts/audio-bench` (ROCKNIX's PipeWire in a container): Bluetooth
  373 glitches in 8 s before, 0 in 10 minutes after, THD+N -67.9 dB, pitch wobble 0.05 / 0.35 cents rms / max; the
  speaker went from -34.5 dB to -66.1 dB. **Not yet heard on the device with real headphones.**
- Loading a save state that was missing or empty said "There is no the auto state" or "The the auto state is empty".

## [0.1.0] - not released yet

The first public source release. It is early: developed and tested on one Retroid Pocket Mini V2 with ROCKNIX
20261001, plus a virtual device. [docs/STATUS.md](docs/STATUS.md) says, feature by feature, how far each one has
been proven.

### Included

- The shelf: Home with Continue, recent games, favourites, Curator and systems; five looks, dark and light; search
  as you type; cover art from the libretro thumbnail collection; play time.
- pocket-core, a libretro host for the handheld systems up to Nintendo DS, PSP, Dreamcast and Saturn, with resume,
  8 save slots, the pause menu on HOME, per-console and per-game options, run-ahead and rewind.
- The same pause menu for PlayStation 2, GameCube, Wii and 3DS through ARMSX2, Dolphin and Azahar.
- Curator: a card per game with offline facts from Wikidata, filters, smart shelves, "more like this", history.
- First-start setup, Wi-Fi, Bluetooth controllers, sleep, battery, fan profiles, Syncthing, Tailscale, Moonlight,
  file sharing, a web companion, signed app updates with rollback.
- The manual on the device, in English, with menus in English or German ([docs/user/](docs/user/index.md)).

### Not included yet

- A published SD card image. It builds and passes its offline checks, and it was started on a fresh card on the Mini V2
  on 2026-10-27, which found the Wi-Fi first-start bug fixed above; a complete first start as a new player (card,
  resize, setup, first game without help) has not been repeated with the fixed build.
- A public update server and Store catalogue.
