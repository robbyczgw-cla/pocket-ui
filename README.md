# pocket-ui

A controller-first game frontend for the **Retroid Pocket Mini V2**, running on [ROCKNIX](https://rocknix.org).
It replaces ROCKNIX's EmulationStation with a shelf of your games that starts in tens of milliseconds, puts
"continue where you left off" first, and gives every system the same small pause menu, from Game Boy to
PlayStation 2. It brings its own libretro host, **pocket-core**, for the systems where that works better than
RetroArch.

The interface is inspired by [NextUI](https://github.com/LoveRetro/NextUI) (one flat list, "continue" first, a
small pause menu, game-time tracking). No NextUI code or assets are used.

> **Early, one device, tested by its developer.** pocket-ui is built on and for one Retroid Pocket Mini V2 with
> ROCKNIX 20261001. There is no published image yet: installing it today means the manual setup described below.

<p align="center">
  <img src="docs/media/home.png" width="49%" alt="Home: the last game, favourites, Curator and the systems">
  <img src="docs/media/home-split.png" width="49%" alt="Home in the Split look">
</p>
<p align="center">
  <img src="docs/media/game-card.png" width="49%" alt="A game's card with facts, play time and notes">
  <img src="docs/media/home-cabinet.png" width="49%" alt="The Cabinet look: Curator's shelves as a bookcase">
</p>

The pictures are rendered from the code with made-up data (`pocket-ui --snapshot`, see
[docs/DEVELOPING.md](docs/DEVELOPING.md)); more are in [docs/media](docs/media/README.md).

## Status

Early. pocket-ui is developed and tested on **one** Retroid Pocket Mini V2, plus a virtual device in Docker.

- Measured or checked on the device: the shelf and its speed, pocket-core for its systems up to Nintendo DS and
  Dreamcast, the pause menu with save slots and options, PlayStation 2 and GameCube through their standalone
  emulators, run-ahead, cover art, Wi-Fi setup, file sharing, UI sounds, waking from deep sleep, Smooth fan,
  Curator and play history, the web companion over a real tailnet (a smoke check), and signed app updates with
  automatic rollback. Starting from the SD card next to Android was tested with a u-boot loaded from memory and set up
  by hand; the permanent setup is described in the install guide.
- Much of what came since was built and tested in the virtual device or in tests only, and is **not confirmed on a
  real device yet**; among others touch, Bluetooth pads, Wii and 3DS, Bluetooth audio (heard distorted on the test device on 2026-10-27; a fix is in progress), the new picture effects and frames, the night
  light, RetroAchievements, Syncthing with a second device, the Store and the starter pack. Nobody has measured the
  battery drain of a long sleep. [docs/STATUS.md](docs/STATUS.md) lists every feature with how it was verified and
  what is still open; the user guide says so on each page where it matters.
- Not available: a published SD card image (it builds and passes its offline checks and was started on a fresh card
  on the Mini V2 on 2026-10-27; that run found a Wi-Fi first-start bug, since fixed, and a full first start as a new
  player has not been repeated), a public update server, a public store catalogue.
- The Retroid Pocket 5, Flip 2 (Snapdragon 865) and first Retroid Pocket Mini are **experimental**: they run the same
  ROCKNIX SM8250 build, pocket-ui recognises them and lays out for their screens, but nobody on the project has one
  and nothing has run on their hardware. Pocket 6, Nova and the AYN Odin 2 family are experimental on the SM8550 image, with screen off
  instead of suspend and ROCKNIX-controlled cooling ([docs/DEVICES.md](docs/DEVICES.md)). Other devices are not supported.

Expect rough edges. Please do not use it on a device you cannot afford to reflash.

## Supported devices

| | |
|---|---|
| Device | Retroid Pocket Mini V2 (Snapdragon 865 / SM8250) |
| System | ROCKNIX, SM8250 build, version 20261001 (the version pocket-ui is tested with) |
| Screen | 1240 x 1080 landscape (the panel is portrait and rotated by the compositor) |

**Experimental, untested on real hardware:** the Retroid Pocket 5 and Retroid Pocket Flip 2 (1920 x 1080; the Flip 2's
lid puts it to sleep) and the first Retroid Pocket Mini (1280 x 960), on the same ROCKNIX SM8250 build. pocket-ui says
so once at the first start and in Settings › System › About. Install ROCKNIX for the device as ROCKNIX describes, then
pocket-ui's install package; the SD card image is for the Mini V2 only. If something doesn't work, Settings › System ›
About › Report a problem makes a file for a report. Details: [docs/DEVICES.md](docs/DEVICES.md).

## What it does

The [user guide](docs/user/index.md) describes each of these; it is also the manual on the device. How far each
feature has been proven is in [docs/STATUS.md](docs/STATUS.md).

- **The shelf:** Home shows your last game, recent games, favourites, Curator and systems. Five looks (Shelf,
  Minimal, Split, Cards, Cabinet), dark, light or automatic, with motion you can reduce or switch off; cover art from
  the libretro thumbnail collection, console pages, play-time tracking.
- **Finding games:** a search over every game as you type, forgiving of accents and typos, and an L2/R2 letter jump
  in every list.
- **Your library and Curator:** a card per game with offline facts (Wikidata), play status and notes; the Lens
  (filters with live counts), smart shelves, your own stickers, Echoes ("more like this" from your own games) and
  the versions of a game you own on several systems.
- **pocket-core, our own libretro host** ([docs/HOST.md](docs/HOST.md)) for Game Boy, Game Boy Color, Game Boy
  Advance, NES, SNES, Mega Drive, Master System, Game Gear, SG-1000, PC Engine, WonderSwan, Atari 2600,
  PlayStation, Nintendo 64, PSP, Dreamcast, Nintendo DS, Saturn (with its BIOS), DOS, ScummVM, PICO-8 and TIC-80.
  It uses the same cores as RetroArch.
- **Pause menu on HOME:** Continue, Save and Load with 8 slots and previews (plus the state of the last quit and
  the one before a load), Options (picture scaling, sharpness and screen effects, screen frames with a live power
  light, the core's own options, button remapping, shortcuts, fast-forward, rewind, run-ahead, cheats, rumble),
  quick settings, one Quit (whether it keeps the moment is a setting), RetroAchievements, and Pixel Path, which grades how a game's picture
  and sound reach you. Settings can be saved per console or per game.
- **PlayStation 2, GameCube, Wii and 3DS** run on ARMSX2, Dolphin and Azahar with the same pause menu for Save,
  Load, Options and Quit ([docs/BIG-EMULATORS.md](docs/BIG-EMULATORS.md)). Other systems start with ROCKNIX's
  default emulator.
- **Fast:** the shelf's first frame comes tens of milliseconds after start, a game starts from the picture of where
  you left it, Settings → On start can go straight back into the last game, and the switcher hands over between two
  pocket-core games as soon as the first has saved. A speed guard measures it ([docs/LATENCY.md](docs/LATENCY.md)).
- **Safety net:** a game that pocket-core cannot start runs through RetroArch from then on. Saves are written
  through temporary files, settings are replaced atomically, the launcher restarts a crashed UI, and a nearly empty
  battery saves the game and switches off.
- **First-start setup** with Wi-Fi, a gamepad keyboard, cover art and PortMaster games.
- **Touch screen** on the shelf, lists, Settings, the search and the game switcher; the bottom screen in Nintendo DS
  games.
- **System:** menu sounds (their own volume), optional menu haptics, quick settings, sleep, a battery page with time
  left, stick LEDs, Settings → Performance & fan, time zone, Bluetooth controllers, and Wi-Fi that reconnects by
  itself.
- **Sync and network:** saves and states kept in step with Syncthing (paused while a game runs), Tailscale with a QR
  sign-in, PC streaming with Moonlight. File sharing (Samba) shares only the games folder, with a per-device
  password. SSH is off by default and, when switched on, shows a random password on the screen
  ([docs/SECURITY-HARDENING.md](docs/SECURITY-HARDENING.md)).
- **Updates:** signed app updates installed live, with an automatic way back. System updates that could touch
  the bootloader are refused ([docs/UPDATES.md](docs/UPDATES.md)).
- **Dual boot next to Android:** a boot chain from the SD card leaves Android and its partitions untouched
  ([docs/INSTALL-DUAL-BOOT.md](docs/INSTALL-DUAL-BOOT.md)).

## Installing

There is no download yet. What exists today:

| Way | State | Guide |
|---|---|---|
| **SD card image** next to Android (Retroid's boot mode switch, nothing flashed) | Builds and passes its offline checks; **started once on a fresh card** (2026-10-27, which found the Wi-Fi first-start bug, since fixed), not yet repeated as a full new-player first start. Not published | [docs/SD-IMAGE.md](docs/SD-IMAGE.md), [docs/INSTALL-DUAL-BOOT.md](docs/INSTALL-DUAL-BOOT.md) |
| **Manual install on a ROCKNIX card** next to Android | Done on the developer's device (phases 1 to 6) | [docs/INSTALL-DUAL-BOOT.md](docs/INSTALL-DUAL-BOOT.md#appendix-manual-install-on-an-existing-rocknix-card) |
| **Developer deploy** onto a Mini V2 running ROCKNIX, over SSH | The daily development path | [docs/DEVELOPING.md](docs/DEVELOPING.md#the-dev-loop-on-a-real-device) |

pocket-ui replaces exactly one thing: the program ROCKNIX's `essway` service starts. Remove its drop-in file and
stock ROCKNIX with EmulationStation comes back. Android on the internal storage is not touched.

You need your own games and, for some systems, BIOS files. pocket-ui includes neither; for an empty SD card it can
download a [starter pack](docs/STARTER-PACK.md) of nine free homebrew games, each from its author's page.

## Building

You need a Rust toolchain with the `aarch64-unknown-linux-gnu` target, [Zig](https://ziglang.org) and
[cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild):

```sh
rustup target add aarch64-unknown-linux-gnu
cargo install cargo-zigbuild
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.38 --bin pocket-ui --bin pocket-core
```

The `.2.38` suffix links against glibc 2.38, which is older than the device's 2.41. The programs land in
`target/aarch64-unknown-linux-gnu/release/`.

Without a device, `cargo test --release` runs the unit tests, the snapshot tests and the documentation checks on a
Linux or macOS computer, and `scripts/vdev.sh` starts a virtual Mini V2 in Docker from a ROCKNIX image you supply
([docs/VIRTUAL-DEVICE.md](docs/VIRTUAL-DEVICE.md)). Everything else, the dev loop and the tests are in
[docs/DEVELOPING.md](docs/DEVELOPING.md).

## Documentation

| For | Start here |
|---|---|
| Players | [docs/user/index.md](docs/user/index.md): getting started, playing, the pause menu, options, controls, settings, your library, achievements, systems, extras, sync, Tailscale, PC streaming, updates, troubleshooting |
| What works on the device | [docs/STATUS.md](docs/STATUS.md): every feature with how it was verified and what is still open |
| Installing next to Android | [docs/INSTALL-DUAL-BOOT.md](docs/INSTALL-DUAL-BOOT.md) |
| Developers | [docs/DEVELOPING.md](docs/DEVELOPING.md) and [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) |
| Everything else | [docs/README.md](docs/README.md): an index of all documents |

The documents name commits of the private development history (for example in STATUS.md); the public repository
starts with release 0.1.0, so those references don't resolve here.

## Contributing

Reports from real devices help most. See [CONTRIBUTING.md](CONTRIBUTING.md), the [code of conduct](CODE_OF_CONDUCT.md),
and [SECURITY.md](SECURITY.md) for security problems. Changes are listed in [CHANGELOG.md](CHANGELOG.md).

## Licence

pocket-ui (the `pocket-ui` and `pocket-core` programs and everything else written for this repository) is free
software under the **GNU General Public License, version 2 or (at your option) any later version**
(`GPL-2.0-or-later`). The full text is in [LICENSE](LICENSE). It comes without any warranty.

pocket-ui is based on [ROCKNIX](https://rocknix.org) and is not endorsed by the ROCKNIX team. Everything that is
not ours keeps its own licence. [NOTICE.md](NOTICE.md) lists it, with sources: ROCKNIX, the emulators and
libretro cores, the system pictures (public-domain photos by Evan Amos, converted with PixelLab), the fonts
(Barlow and Titillium Web, SIL Open Font License) and the Rust crates
([docs/THIRD-PARTY-CRATES.md](docs/THIRD-PARTY-CRATES.md)).

- Several libretro cores that ROCKNIX ships (snes9x, genesis_plus_gx, picodrive, fbneo) are licensed for
  non-commercial use only, and so are ROCKNIX's branding and artwork (CC BY-NC-SA 4.0). pocket-ui contains none of
  them, but an image that bundles them must not be sold. Check [NOTICE.md](NOTICE.md) before you redistribute one.
- pocket-ui does not include games, ROMs, BIOS files or ROCKNIX itself. `scripts/check-release-tree.py` checks
  a tree, a tar or an image for them.
- `assets/gamedb.bin` holds data from Wikidata, released under CC0. No Wikipedia text is included.
- `assets/curatordb.bin` and `assets/data/curator-tags.json` (Curator's feel tags) are our own data, derived at
  build time from those CC0 facts, under pocket-ui's licence. No Wikipedia or other copyrighted text is included.

## Credits

- [ROCKNIX](https://rocknix.org) and its contributors: the operating system pocket-ui runs on.
- [libretro](https://www.libretro.com) and the authors of the cores pocket-core and RetroArch load.
- The [libretro thumbnail collection](https://thumbnails.libretro.com) for the box art.
- [Wikidata](https://www.wikidata.org) for the game facts on the game card (year, developer, publisher, genre,
  series, players, review scores). They are CC0 (public domain); `assets/gamedb.bin` is a compact copy built by
  `scripts/build-gamedb.py`.
- [Wikipedia](https://en.wikipedia.org) for the game descriptions, under
  [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). They are not part of pocket-ui: the device
  loads an article's lead when a card is opened and names the article and the licence under the text.
- [PortMaster](https://portmaster.games) for the ports catalogue.
- ARMSX2, Dolphin and Azahar, the standalone emulators for PlayStation 2, GameCube and Wii, and 3DS.
- [RetroAchievements](https://retroachievements.org) and its rcheevos library (MIT), in `vendor/rcheevos`.
- Retroid's [u-boot for the Pocket devices](https://github.com/RetroidPocket/u-boot), used by the dual-boot setup.
- [NextUI](https://github.com/LoveRetro/NextUI) and MinUI for the ideas behind the interface.
- Barlow, by Jeremy Tribby, for the interface font, and Titillium Web for the test store's font package.
- [Evan Amos](https://commons.wikimedia.org/wiki/User:Evan-Amos), whose public-domain console photographs on Wikimedia
  Commons are the base of the system pictures ([assets/systems/SOURCES.md](assets/systems/SOURCES.md)).


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
and Zink on the real GPU still need a tester. Mini and Portal menus start with Xbox buttons
(A bottom); the original Odin 2 uses Nintendo buttons (A right). Saved layout choices win. See [device notes](docs/DEVICES.md).
