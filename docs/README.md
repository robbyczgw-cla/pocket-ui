# Documentation index

Every document in this folder, by who it is for and how far you can rely on it.

- **Reference:** describes what the code does now. Kept in step with the code.
- **Design notes:** research and decisions from when a feature was planned. They explain why, but parts may be
  ahead of or behind the code. Each one starts with a status line saying so.
- **Internal:** checklists and measurements from the author's own device.

How far a feature has been proven (on the device, in the virtual device, in tests only) is in one table,
[STATUS.md](STATUS.md). Every Status note in the documents below agrees with it; whoever changes a feature's
verification state updates its row.

## For players

Start at [user/index.md](user/index.md).

| Document | Contents |
|---|---|
| [user/index.md](user/index.md) | The first page: what pocket-ui is and does, what it leaves alone, the list of pages |
| [user/getting-started.md](user/getting-started.md) | Preparing the card, first start, Wi-Fi, adding games |
| [user/other-handhelds.md](user/other-handhelds.md) | Other Retroid devices, SM8550 and Anbernic H700 |
| [user/games-drive.md](user/games-drive.md) | RG28XX card: copying games from a Mac or Windows PC |
| [user/copy-games.md](user/copy-games.md) | Copying games without Wi-Fi: the second card |
| [user/how-to.md](user/how-to.md) | Short steps: update over Wi-Fi, a game that won't start, pair a controller, back up saves, sync with a second device |
| [user/game-check.md](user/game-check.md) | Settings → Games → Game check: will your games start? |
| [user/coming-from.md](user/coming-from.md) | Coming from ROCKNIX or Android: what happens to saves, states, favourites and settings, what needs a copy, what does not come over |
| [user/playing.md](user/playing.md) | Home, finding a game, the looks, console pages, starting a game, the game switcher, continuing, menu sounds |
| [user/pause-menu.md](user/pause-menu.md) | The pause menu of each emulator, Quit and Settings → Playing → Continue where I quit, Pixel Path |
| [user/pause-menu-saves.md](user/pause-menu-saves.md) | Save slots, Where I quit and Before load, Earlier, where the state files are, when a save fails |
| [user/options.md](user/options.md) | Options in a game: picture, screen frames, emulator, controls, cheats, shortcuts, extras, run-ahead, saving them |
| [user/options-cheats.md](user/options-cheats.md) | Options → Cheats: the cheat list, the one-time download on TrimUI, offline use, cheat files pocket-core cannot apply |
| [user/options-big.md](user/options-big.md) | The Options of PS2, GameCube, Wii and 3DS games |
| [user/controls.md](user/controls.md) | Every button, screen by screen |
| [user/controls-card.md](user/controls-card.md) | The console button drawing, remaps and HOME shortcuts; first start, game card and pause menu |
| [user/easy-to-read.md](user/easy-to-read.md) | Easy to read (larger text and calmer menus in one switch), the pause menu's Zoom and Controls → Hold to toggle |
| [user/controls-settings.md](user/controls-settings.md) | The buttons in Settings, during setup, on the touch screen, and the power and volume keys |
| [user/settings.md](user/settings.md) | How Settings opens, and the entries of Look, Sound & feel, Playing and Games |
| [user/settings-sound.md](user/settings-sound.md) | The entries of Sound & feel: animations, Cozy touches, motion, sounds, haptics and LEDs |
| [user/settings-system.md](user/settings-system.md) | The entries of Connections and System: Wi-Fi, Tailscale, remote access, time, sleep, backup, the manual |
| [user/storage.md](user/storage.md) | Settings → System → Storage: what takes the room on the card, the clean-ups, the low-space notice, hidden games |
| [user/bluetooth.md](user/bluetooth.md) | Settings → Connections → Bluetooth: pairing controllers, headphones and speakers, reconnecting, forgetting |
| [user/continue.md](user/continue.md) | Continue: an open case means your moment is waiting and A goes straight back; how long each system takes, what can't continue |
| [user/quick-settings.md](user/quick-settings.md) | Quick settings (HOME), the power menu, sleep, power off and low battery |
| [user/language.md](user/language.md) | Settings → System → Language: English, German or Spanish, what stays English |
| [user/night-light.md](user/night-light.md) | Settings → Look → Night light: warmer, less blue colours in the evening, the schedule, the strength, how it fades and where it reaches |
| [user/theme.md](user/theme.md) | Settings → Look → Colours and Font: the five colour sets and the three fonts |
| [user/performance.md](user/performance.md) | Settings → Playing → Performance & fan: game defaults, the fan in the menus, Smooth fan, the readout |
| [user/tour.md](user/tour.md) | The Tour (the basics in 6 cards), the feature cards: what the first start's profiles set, "New in this version" |
| [user/library.md](user/library.md) | The game card, play status, ratings, notes, hiding games and matching your files |
| [user/curator.md](user/curator.md) | Curator: Lens, smart shelves, stickers, Echoes, Versions, play history and the Year in review |
| [user/achievements.md](user/achievements.md) | RetroAchievements: signing in, unlocks, the pause menu's list, offline play |
| [user/systems.md](user/systems.md) | Which systems run on which emulator |
| [user/arcade.md](user/arcade.md) | Arcade games: the cores, matching romsets, parents and clones, BIOS and CHD files, when a game does not start |
| [user/patches-msu1.md](user/patches-msu1.md) | Translations and hacks (IPS, BPS, UPS patches), prepatching, RetroArch softpatching, one MSU-1 recipe |
| [user/extras.md](user/extras.md) | Box art, game pictures (screenshots and title screens), PortMaster games, the Store |
| [user/sync.md](user/sync.md) | Syncing saves and states with Syncthing |
| [user/remote.md](user/remote.md) | Reaching the Pocket from anywhere with Tailscale: switch, QR sign-in, name, what it does to SSH |
| [RETRO-STYLE.md](RETRO-STYLE.md) | The Retro style: Settings → Look → Style → Retro, its palettes, frames, icons and blips |
| [user/easy.md](user/easy.md), [BIG-SHELF.md](BIG-SHELF.md) | The Easy look: one big game at a time, for a handful of games |
| [user/kids-mode.md](user/kids-mode.md), [KIDS-MODE.md](KIDS-MODE.md) | Kids mode: for a grown-up; what is closed and how, the clock, what is left open |
| [user/phone-upload.md](user/phone-upload.md) | Games from your phone: Add games → Y, the QR code, what it takes and refuses, what plain HTTP on a Wi-Fi means |
| [user/web-companion.md](user/web-companion.md) | The web companion: switching it on, pairing a browser, what it can do, how it is kept safe |
| [user/pc-streaming.md](user/pc-streaming.md) | PC streaming with Moonlight: pairing, starting, leaving |
| [user/updates.md](user/updates.md) | Updating pocket-ui and the system |
| [user/safe-mode.md](user/safe-mode.md) | When pocket-ui won't start: the way back, safe mode, opening EmulationStation safely |
| [user/hotel-wifi.md](user/hotel-wifi.md) | Hotel and train Wi-Fi: networks that want a sign-in first, signing in on the Pocket, the other ways online |
| [user/troubleshooting.md](user/troubleshooting.md) | Known limits, questions, where data is kept |

The regular pages of `user/` are also the manual on the device (Settings → Help & tools → Manual, `src/manual.rs`), in the order of
[user/index.md](user/index.md). The experimental branch note [user/android-spike.md](user/android-spike.md) describes the
separate Android prototype and is excluded from the ROCKNIX on-device manual (`manual::DESKTOP_ONLY`).

## Installing

| Document | Type | Contents |
|---|---|---|
| [INSTALL-DUAL-BOOT.md](INSTALL-DUAL-BOOT.md) | Reference | Starting pocket-ui from the SD card next to Android: the SD card image, going back to Android, and the manual way in phases that can each be undone |
| [BOOT-SPLASH.md](BOOT-SPLASH.md) | Reference | What shows from power-on to the shelf: the quiet start (and how to give a card set up before it, or take it back), the boot screen pocket-splash, its unit and how it is installed and removed, the fade into the shelf, the device test |
| [SD-IMAGE.md](SD-IMAGE.md) | Reference | The ready-to-flash SD card image: how it is built from ROCKNIX 20261001, how ROCKNIX's first start is handled, the offline check, licences, the open device test |

## For developers

Start at [DEVELOPING.md](DEVELOPING.md).

| Document | Type | Contents |
|---|---|---|
| [DEVELOPING.md](DEVELOPING.md) | Reference | Building, the dev loop, the virtual device, tests and the documentation checks, environment variables, repository layout |
| [BINARY-SIZE.md](BINARY-SIZE.md) | Internal | Release binary size by crate/function and embedded data, full-LTO savings, storage reserve and before/after runtime measurements |
| [ARCHITECTURE.md](ARCHITECTURE.md) | Reference | How pocket-ui fits into ROCKNIX, the processes, the launch flow, switching games, the pause menu, and where each feature lives |
| [i18n-glossary.md](i18n-glossary.md) | Reference | The German and Spanish interfaces: tone, the terms every page uses, what stays English and why |
| [HOST.md](HOST.md) | Reference | pocket-core, our libretro host: how it works, systems, measurements |
| [AUDIO-PATH.md](AUDIO-PATH.md) | Reference | Prepared sinc and NEON, H700 buffering, RetroArch comparisons and measurement limits |
| [XBOX.md](XBOX.md) | Reference | Xbox through the hakuX core in RetroArch: start, appendconfig, pause menu, licence, device test (experimental) |
| [BIG-EMULATORS.md](BIG-EMULATORS.md) | Reference | PlayStation 2, GameCube, Wii and 3DS through ARMSX2, Dolphin and Azahar: save slots, Options, measurements |
| [FRAMES.md](FRAMES.md) | Reference | Screen frames (bezels) and frame packs |
| [SYSTEM-CONTROLS.md](SYSTEM-CONTROLS.md) | Reference | Volume, brightness, colour temperature, quick settings, time zone |
| [H700-VOLUME.md](H700-VOLUME.md) | Reference | H616 speaker DAC/mute workaround, headphone routing, ownership, provisional ceiling and hardware-test limits |
| [ROCKNIX-H700-AUDIO-ISSUE-DRAFT.md](ROCKNIX-H700-AUDIO-ISSUE-DRAFT.md) | Draft, not submitted | Frozen-rootfs UCM finding, observed controls and untested upstream investigation proposal |
| [SLEEP.md](SLEEP.md) | Reference and design notes | Sleep and power: research, what is built, a device test plan |
| [DEVICES.md](DEVICES.md) | Design and reference | The Mini V2 and its experimental SM8250 siblings (Pocket 5, Flip 2, Mini V1): how pocket-ui tells them apart, what each gets, layouts at 1920x1080 and 1280x960, safety on untested hardware, installing |
| [TRIMUI-BRICK.md](TRIMUI-BRICK.md) | Design and reference | The TrimUI Brick (Beta) behind the `brick` feature: the first version under NextUI, the framebuffer bridge, the test kit |
| [TRIMUI-BRICK-CORES.md](TRIMUI-BRICK-CORES.md) | Design and reference | Additional standalone software cores: pinned builds, licences, routing, real-library checks and limits |
| [TRIMUI-BRICK-STANDALONE.md](TRIMUI-BRICK-STANDALONE.md) | Design and reference | The same Brick without NextUI: a card folder with its own start script, cores built for glibc 2.33, stand-ins for the firmware helpers, host tests |
| [TRIMUI-PORTMASTER.md](TRIMUI-PORTMASTER.md) | Design and reference | Optional TrimUI PortMaster setup, portable runtime archives, persistent games and files, cancellation and native launch |
| [TRIMUI-SMART-PRO.md](TRIMUI-SMART-PRO.md) | Design and reference | The TrimUI Smart Pro on the same card (experimental): how the card tells it from the Brick, what differs, the firmware floor, what a report carries |
| [TRIMUI-BRICK-PRO.md](TRIMUI-BRICK-PRO.md) | Design and reference | Experimental Brick Pro on the same TrimUI card: detection, separate stick clicks and F keys, suspend handshake, evidence and hardware limits |
| [TRIMUI-BRICK-REMOTE.md](TRIMUI-BRICK-REMOTE.md) | Design and reference | The Brick's own card on the network: Tailscale, Remote access with dropbear, updates of the card's program folder from a private channel, the way back |
| [TRIMUI-BRICK-MORE.md](TRIMUI-BRICK-MORE.md) | Design and reference | The Brick's own card with Kids mode, screenshots (and the `grim` stand-in), the web companion over userspace Tailscale, Over the game and a Tour that fits |
| [TRIMUI-BRICK-HW.md](TRIMUI-BRICK-HW.md) | Design and reference | The Brick's own hardware: rumble motor, the 23 LEDs, the F1/F2 keys and the mute slider, and the Settings page "Lights & buttons" |
| [TRIMUI-BRICK-SYNC.md](TRIMUI-BRICK-SYNC.md) | Design and reference | Save sync on the Brick's own card: the card's Syncthing, its service and what differs from ROCKNIX |
| [TRIMUI-BRICK-BLUETOOTH.md](TRIMUI-BRICK-BLUETOOTH.md) | Design and reference | Bluetooth on the Brick's own card: the BlueZ service, pairing kept on the card, controllers, and headphones over BlueALSA (A2DP) |
| [TRIMUI-BRICK-SMOKE.md](TRIMUI-BRICK-SMOKE.md) | Testing | The smoke test on the real Brick after an installation: version, services, lights in the dark, picture options and the FPS line in a test game |
| [BRICK-CHEAT-PROBE.md](BRICK-CHEAT-PROBE.md) | Testing | Cheat RAM effects, Continue, save states and disabling: core constraints, container checks and real Brick evidence |
| [BRICK-CFG.md](BRICK-CFG.md) | Design and verification | Compile-time ownership of Brick code and assets, the standard-build reference and comparison evidence |
| [BRICK-SHADERS.md](BRICK-SHADERS.md) | Design and reference | Picture options on the Brick through the GPU: the choices, their cost and the read-back proof |
| [STEAMDECK.md](STEAMDECK.md) | Design and reference | pocket-ui on a Steam Deck (first preparation, run on a PC only): the Deck profile, EmuDeck library, emulators, bundle and Flatpak plan, Add to Steam, what needs real hardware |
| [WEB-DEMO.md](WEB-DEMO.md) | Design and reference | pocket-ui in the browser (prototype): the emscripten build beside the device builds, the winit and softbuffer stand-ins, the stand-in game, building, measured size and speed |
| [PLATFORMS.md](PLATFORMS.md) | Reference | Which user-visible feature exists on ROCKNIX handhelds, Android and the Steam Deck, and what was seen on a device |
| [H700-SCALED-LAYOUT.md](H700-SCALED-LAYOUT.md) | Small-panel logical layouts, complete snapshot comparisons, JPEG readability and CPU measurement method |
| [DISPLAY-OUT.md](DISPLAY-OUT.md) | Design and reference | Playing on a TV through USB-C: what ROCKNIX does, TV mode, what is built, the device test plan |
| [DUAL-SCREEN.md](DUAL-SCREEN.md) | Design and reference | Two screens (the RG DS Plus; Retroid's add-on, prototype): the shelf on top, the game's box below, the panel table to confirm on the device |
| [FAN-CURVE.md](FAN-CURVE.md) | Design and reference | Smooth fan: why the mechanism, the curves, the safety net, the simulation, the device test |
| [UPDATES.md](UPDATES.md) | Reference | The signed update channel for the app and the system |
| [UPDATE-CHANNEL.md](UPDATE-CHANNEL.md) | Reference | Where the channel lives (pocketui.xyz, GitHub Releases), the signing key, the release procedure, rollback |
| [ROCKNIX-CONTRACT.md](ROCKNIX-CONTRACT.md) | Reference | Everything pocket-ui relies on in ROCKNIX (scripts, functions, settings, es_systems, cores, services) and how `scripts/rocknix-contract.sh` checks a system for it |
| [STORE.md](STORE.md) | Reference | The Store page, package kinds, the signed catalog, publishing |
| [STARTER-PACK.md](STARTER-PACK.md) | Reference | The starter pack: free games for an empty card, their licences and credits, how a download is checked and placed, how to add a game |
| [SYSTEM-PAGES.md](SYSTEM-PAGES.md) | Reference | Settings → Help & tools → Tools, Wi-Fi (saved networks, hidden network, country), PC streaming (pairing on the device), Backup & reset: what replaces EmulationStation's menus |
| [PORTAL-SIGNIN.md](PORTAL-SIGNIN.md) | Design and reference | Wi-Fi with a sign-in page: the row and sheet, the re-check, the sign-in helper, what it never does, limits, tests |
| [SECURITY-HARDENING.md](SECURITY-HARDENING.md) | Reference | File sharing, SSH and other network services |
| [WEB-COMPANION.md](WEB-COMPANION.md) | Reference | The web companion: threads and cost, where it listens, sign-in, CSRF, every endpoint, uploads, the threat model and its self-review |
| [PHONE-UPLOAD.md](PHONE-UPLOAD.md) | Reference | "Send from your phone": the upload server that exists only while its page is open, its routes, limits and threat model |
| [VIRTUAL-DEVICE.md](VIRTUAL-DEVICE.md) | Reference | The Docker-based virtual Mini V2 |
| [WIFI-BENCH.md](WIFI-BENCH.md) | Reference | The Wi-Fi bench: ROCKNIX's NetworkManager + iwd and the real pocket-ui against simulated radios and access points (mac80211_hwsim + hostapd), its scenarios and the first results |
| [IDLE.md](IDLE.md) | Reference | Still menus: finite animation passes, input and timer scheduling, two-window presentation traces and repeatable idle measurements |
| [LATENCY.md](LATENCY.md) | Reference | The speed guard: latency benchmark, baselines, comparing in CI |
| [MEASURED.md](MEASURED.md) | Reference | The homepage's former numbers page: speed guard of 25 Oct, waking, sleep, CPU, pocket-ui against EmulationStation |
| [OTHER-LAYOUTS.md](OTHER-LAYOUTS.md) | Reference | Cards from other systems: folder names, box art folders, how pocket-ui reads them in place, numbers for 5,000 and 13,000 games |
| [COMPARE-ROCKNIX.md](COMPARE-ROCKNIX.md) | Reference | pocket-ui against stock ROCKNIX (EmulationStation + RetroArch) on a Mini V2, measured from outside the same way for both: method, limits, the plan of presses, how to run it |
| [DESIGN.md](DESIGN.md) | Reference | Visual design of the shelf: colours, type, layout, motion |
| [UI-SCHEME.md](UI-SCHEME.md) | Reference | The one control scheme every page follows: keys, focus, hint bar, rows, help lines |
| [CURATOR.md](CURATOR.md) | Design notes and reference | Curator (Lens, smart shelves, stickers, Echoes, Versions) and Pixel Path: the design, then what was built ("Developer notes", "Pixel Path as built") |
| [HISTORY.md](HISTORY.md) | Reference | Play history: the session log and its file, what counts, Curator → History and the Year in review, the read API for the web companion |
| [SYNC-CURATION.md](SYNC-CURATION.md) | Reference | Settings → Sync → Library data: one folder per device, values with times, the field-by-field merge with tombstones, the play history union, what is left out |
| [SYNC-OWNERSHIP.md](SYNC-OWNERSHIP.md) | Reference and design notes | Save-sync pause ownership, failed-resume retry tests, the remaining interruption ambiguity and the manual-recovery follow-up |
| [CURATOR-DATA.md](CURATOR-DATA.md) | Reference | Curator's data: Wikidata facts and versions by work, the feel-tag vocabulary and labelling, ROM file-name facts, the reader APIs |
| [STATUS.md](STATUS.md) | Reference | The feature status ledger: one row per feature with how it was verified (device-measured, device smoke-checked, virtual device only, host tests only, not built), the evidence and what is still open |
| [LEDS.md](LEDS.md) | Reference | The lights around the sticks: ROCKNIX's scripts, what Settings → Sound & feel → LEDs writes, a device checklist |

## Design notes and history

| Document | Contents |
|---|---|
| [PER-CONSOLE.md](PER-CONSOLE.md) | Research for the pause menu and per-console options (largely built) |
| [ONBOARDING-STORE.md](ONBOARDING-STORE.md) | Research for the first start and the store (partly built) |
| [IMAGE-AND-BOOT.md](IMAGE-AND-BOOT.md) | Boot options and a plan for an own ROCKNIX-based image (the dual boot guide came out of it) |
| [BOOT-NOTES.md](BOOT-NOTES.md) | How the Mini V2 boots ROCKNIX, and the OpenABL route that pocket-ui does not use |
| [FASTBOOT-TEST.md](FASTBOOT-TEST.md) | The first test: ROCKNIX from memory with `fastboot boot` |
| [EMULATOR-RESEARCH-2026-10.md](EMULATOR-RESEARCH-2026-10.md) | Web research on emulators for ROCKNIX on the Snapdragon 865, with sources (October 2026) |
| [IDEAS-RESEARCH-2026-10.md](IDEAS-RESEARCH-2026-10.md) | Web research for feature ideas: other handheld systems, Roon, game metadata (October 2026; Curator came out of it) |
| [OS-COMPARISON-2026-10.md](OS-COMPARISON-2026-10.md) | pocket-ui compared with ROCKNIX, KNULLI, Batocera, muOS and NextUI (October 2026) |
| [NOVA-FEASIBILITY.md](NOVA-FEASIBILITY.md) | Could pocket-ui run on the Retroid Pocket Nova (SM8550, 4:3 1280x960)? Its ROCKNIX state, every Mini V2 assumption in the code, what can be checked without the device, a phased plan and the recommendation (not now) (October 2026) |

## Internal

| Document | Contents |
|---|---|
| [DEVICE-TESTS.md](DEVICE-TESTS.md) | Checklist of what only a person holding the device can check |
| `latency/` | The speed guard's baselines ([LATENCY.md](LATENCY.md)) |
| `bench/` | Raw benchmark data |
| [THIRD-PARTY-CRATES.md](THIRD-PARTY-CRATES.md) | Generated list of the Rust crates and their licences (`scripts/third-party.sh`); the rest of the credits are in [../NOTICE.md](../NOTICE.md) |

Android launcher foundation: [developer architecture and intent recipes](ANDROID-MVP.md), [preview controls](user/android-mvp.md).
