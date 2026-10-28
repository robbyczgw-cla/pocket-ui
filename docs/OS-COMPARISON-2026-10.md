# pocket-ui compared with ROCKNIX, KNULLI, Batocera, muOS and NextUI (October 2026)

What other handheld Linux systems do, where pocket-ui stands against them, and what is worth taking over.
Sources were checked on 22 Oct 2026; pocket-ui facts come from the code and docs at main `e44a492`.

## The systems

| | Base and frontend | Runs on the Mini V2? | Licence | Idea |
|---|---|---|---|---|
| **pocket-ui** | Our own UI and libretro host (pocket-core) on top of ROCKNIX | Yes, the only target | GPL-2.0-or-later | A small library you actually play, resume first, one pause menu for every system |
| **ROCKNIX** | JELOS lineage, EmulationStation, RetroArch plus standalone emulators; immutable image | Yes, SM8250 build ([device page](https://rocknix.org/devices/retroid/retroid-pocket-mini/)) | GPL | Everything for many devices, configured in EmulationStation menus |
| **KNULLI** | Batocera fork, EmulationStation | Yes, since Scarab (May 2026): RP5, Flip 2, Mini, Mini V2 ([changelog](https://github.com/knulli-cfw/knulli-linux/blob/knulli-main/knulli-Changelog.md)) | GPL / Batocera terms | Batocera's features, tuned for handhelds |
| **Batocera** | Buildroot, EmulationStation | Batocera 43 (May 2026) ships one image for SM8250 and SM8550 devices ([coverage](https://retrohandhelds.gg/batocera-43-adds-support-for-ayn-thor-retroid-pocket-6-and-is-packed-with-new-features/)); Mini V2 support not confirmed here | Batocera terms | Desktop and TV first, handhelds second |
| **muOS / MustardOS** | Own frontend on H700 and similar budget chips | No | Own | Fast, lightweight, many themes |
| **NextUI** | MinUI fork, rebuilt emulation engine | No: TrimUI Brick, Smart Pro, Smart Pro S only ([README](https://github.com/LoveRetro/NextUI)) | **PolyForm Noncommercial since 15 Aug 2026**, GPL before | MinUI's calm, plus the features people asked for |
| **Socket** (frontend only) | Android launcher | Android side only | Closed, Patreon builds | Cartridges and discs as objects; 0.8.0 (Oct 2026) added dual screen, play time, sorting ([coverage](https://retrohandhelds.gg/socket-releases-version-0-8-0-adding-dual-screen-support-and-better-performance/)) |

The realistic competition on this device is **stock ROCKNIX and KNULLI**. NextUI is a design reference, not an
alternative: it does not run on the Mini V2. Its licence change means we must keep using **no NextUI code or
assets**, which the README already promises.

## Feature by feature

Legend: ✔ has it, ◐ partly, ✘ no, – not applicable.

| Feature | pocket-ui | ROCKNIX (stock) | KNULLI | NextUI |
|---|---|---|---|---|
| Resume first: last game on Home, auto-resume | ✔ | ◐ (RetroArch auto-save state, no resume-first home) | ◐ | ✔ |
| Game switcher with play time | ✔ (last frame per game in progress) | ✘ | ◐ | ✔ |
| One pause menu across libretro **and** PS2/GameCube/Wii/3DS standalones | ✔ | ✘ (each emulator has its own menu and hotkeys) | ✘ | – (no standalones) |
| Save slots with previews | ✔ 8 slots | ✔ RetroArch | ✔ | ✔ |
| Per-game / per-console settings | ✔ | ✔ (ES advanced settings) | ✔ | ✔ |
| Picture effects | ◐ LCD grid, scanlines, CRT | ✔ full RetroArch shaders | ✔ shaders | ✔ shaders, overlays |
| Cheats, rewind, fast-forward | ✔ | ✔ | ✔ | ✔ cheats |
| Box art | ✔ libretro thumbnails, automatic | ✔ scrapers (ScreenScraper etc.), manual | ✔ scrapers | ✔ |
| Game details (year, developer, rating) | ◐ in progress (Wikidata, offline) | ✔ via scraper | ✔ via scraper | ✘ |
| Play status and notes | ◐ in progress | ✘ | ✘ | ✘ |
| System pictures | ✔ 23 pixel-art consoles (public-domain photos) | theme-dependent | theme-dependent | ✘ |
| First start: Wi-Fi, time zone, look, systems | ✔ | ◐ | ◐ | ◐ |
| Touch screen in the UI | ◐ built; not confirmed on the real panel | ◐ | ◐ | – |
| RetroAchievements | ✘ | ✔ | ✔ | ✘ |
| Netplay | ✘ | ✔ RetroArch | ✔ | ✘ |
| Save sync (Syncthing, cloud) | ✔ Settings → Sync, scan after each game (checked on a Mini V2, not with a second device; [user/sync.md](user/sync.md)) | ✔ ([Cloud Sync](https://rocknix.org/configure/cloud-sync/)) | ✔ sync on game exit | ✘ |
| PC game streaming (Moonlight) | ◐ Home entry "PC streaming"; pairing over SSH, real stream untested ([user/pc-streaming.md](user/pc-streaming.md)) | ✔ ([Moonlight](https://rocknix.org/systems/moonlight/)) | ✔ | ✘ |
| Ports (PortMaster) | ✔ | ✔ | ✔ | ◐ via Paks |
| Performance / power profiles | ◐ CPU governor per emulator, no user TDP setting | ✔ profiles | ✔ | ✔ dynamic CPU scaling |
| Sleep | ◐ wake from deep sleep measured on a Mini V2; battery drain over a night not measured ([STATUS.md](STATUS.md)) | ✔ | ✔ | ✔ light + deep sleep |
| Battery page | ✔ | ◐ | ✔ BatteryPlus | ✔ history graph, time left |
| Bluetooth | ◐ toggle in quick settings and a pairing page for controllers (no real controller tried yet); no audio support | ✔ | ✔ | ✔ audio |
| Menu sounds and haptics | ✔ | ✔ | ✔ | ✔ haptics |
| App updates | ✔ signed, live, automatic rollback | ✔ OTA (whole image) | ✔ OTA | ✔ |
| Add-on store | ◐ Store page built, no public catalog | ✘ | ✘ | ✔ Pak Store |
| Security defaults | ✔ SSH off, random password, Samba only shares games | ◐ SSH on with a known password | ◐ | – |
| Dual boot next to Android, Android untouched | ✔ | ◐ (can boot from SD; some guides overwrite the loader) | ◐ | – |
| Devices | **1** | dozens | dozens | 3 |
| Install image | ✘ by hand today | ✔ | ✔ | ✔ |
| Users and community | none yet | large | large | ~1k GitHub stars, Discord |

## Where pocket-ui is ahead

1. **One pause menu for everything.** PS2, GameCube, Wii and 3DS get Save, Load, Quit and settings from the same
   HOME menu as an NES game. No other system on this device does this; on ROCKNIX each standalone has its own
   menu and hotkey chord.
2. **Resume first, small library.** Home starts with the last game; the switcher shows where you stopped. Stock
   ROCKNIX and KNULLI start with a system carousel built for large collections.
3. **Safety by default.** Atomic saves, a crash-restarting launcher, a fallback to RetroArch when pocket-core
   cannot start a game, signed updates with rollback, SSH off.
4. **Android stays as it is.** The SD boot chain does not flash anything.

## Where pocket-ui is behind

Ordered by how much a player on this device would miss it:

1. **Install image.** Nobody can try pocket-ui without SSH. This blocks the public release more than any feature.
2. **RetroAchievements.** A standard feature on ROCKNIX and KNULLI. pocket-core would need rcheevos; the
   standalones (Dolphin, ARMSX2) bring their own.
3. ~~**Save sync.**~~ Built (22 Oct 2026, not merged yet): switch, device ID and QR code, status line, a rescan
   of the save folders when a game ends, and a protected web interface instead of ROCKNIX's open one. Needs a
   first pairing with a real second device.
4. **Shaders.** Three built-in effects cover most of it; people coming from RetroArch expect their CRT shaders.
5. **Netplay**, and the rest of **Moonlight**. Netplay is niche on a small handheld, but present everywhere else.
   Moonlight has its Home entry now; pairing still happens over SSH, so an on-screen PIN page is the next step.
6. **Bluetooth pairing screen** (controllers, headphones).
7. **Sleep confirmed on the device.**
8. **Device count.** One device is fine for a first release. The RP5 and Flip 2 share the SoC and would be the
   cheapest next targets.

## Worth taking over

| From | Idea | Effort |
|---|---|---|
| KNULLI | Sync saves on game exit, with a small notice | done: Settings → Sync (`src/sync.rs`), scan after each game |
| NextUI | Battery history graph with time left | done: Settings → Battery (48 h history, time left, cost per system) |
| NextUI | Light sleep, then deep sleep after two minutes | medium: our sleep exists, needs device confirmation |
| KNULLI | On-screen manual | done: Settings → Manual reads docs/user, built into the binary |
| ROCKNIX | Moonlight as a Home entry | done: PC streaming, ticked on in Systems on the shelf (`src/moonlight.rs`) |
| NextUI | Add-on store with community packages | large: needs a catalog and a review process |
| Socket | Cartridge/disc look for box art | medium, a fifth look |

## Sources

Checked on 22 Oct 2026:
- NextUI README and licence note: <https://github.com/LoveRetro/NextUI>
- KNULLI Scarab coverage, 11 May 2026: <https://retrohandhelds.gg/knulli-scarab-makes-one-of-the-best-handheld-firmwares-even-better/>
- KNULLI changelog (SM8250 support): <https://github.com/knulli-cfw/knulli-linux/blob/knulli-main/knulli-Changelog.md>
- ROCKNIX wiki and device page: <https://rocknix.org/>, <https://rocknix.org/devices/retroid/retroid-pocket-mini/>
- Batocera 43 coverage, 14 May 2026: <https://retrohandhelds.gg/batocera-43-adds-support-for-ayn-thor-retroid-pocket-6-and-is-packed-with-new-features/>
- Socket 0.8.0 coverage, Oct 2026: <https://retrohandhelds.gg/socket-releases-version-0-8-0-adding-dual-screen-support-and-better-performance/>
- Earlier research in this repo: [IDEAS-RESEARCH-2026-10.md](IDEAS-RESEARCH-2026-10.md), [EMULATOR-RESEARCH-2026-10.md](EMULATOR-RESEARCH-2026-10.md)

The ROCKNIX and KNULLI columns describe their documented features, not a test on this device. "In progress"
rows in the pocket-ui column are agent branches that are not merged yet.
