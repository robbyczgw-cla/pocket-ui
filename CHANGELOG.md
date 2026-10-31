# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may change anything).

## [Unreleased]

### Added

- SD card image: Settings → Help & tools → About names the system, "pocket-ui OS 0.1.8 · built on ROCKNIX 20261001
  (SM8250)" (on a ROCKNIX card with pocket-ui added, the ROCKNIX version). The device is called "pocket-ui" on the
  network, in Bluetooth and Tailscale, and EmulationStation's ROCKNIX update check is off.

### Changed

- SD card image: ROCKNIX updates are no longer installed on the card. At every start the boot hook moves any system
  update pocket-ui did not check (from EmulationStation's menu, the network share or copied by hand) to
  `/storage/.update-refused` and starts as usual; nothing there is deleted ([docs/SD-IMAGE.md](docs/SD-IMAGE.md)).

### Fixed

- On Android, back from Cozy extras lands on its row in Settings, no longer on the first.
- Tour and first-start cards on the Steam Deck: none for Haptics, Sync, Tailscale, Tools or Performance & fan.
- Steam Deck: no idle timers, suspend, power-key hold or low-battery power-off of ours any more; SteamOS owns sleep and the
  power button (the Settings rows were hidden, but the timers still ran and would have suspended after 4 minutes).

## [0.1.8] - 2026-10-31

### Added

- **Game check:** Settings → Games → Game check answers "will my games start?" after you copied games. It says in plain
  words what is wrong and what to do: a missing, misplaced or wrong BIOS (by checksum), a disc without its track file,
  empty files, zips without a game, the same game twice (A hides the copy), a game in the wrong folder or with the wrong
  file type, a system this handheld can't play. "All good: 214 games ready" when there's nothing. Adding games offers it.
  Works offline; never deletes a game, downloads anything or touches saves.
- **Frozen games:** your last five games with a saved moment stand a little open on the shelf, and A goes straight back to
  that moment, with no question and no waiting. It uses the moment Quit and sleep already keep: nothing keeps running,
  and nothing extra goes on the card. Games that can't go back stand closed and say why when started. On at the start;
  Settings → System → Frozen games switches it off.
- **Consoles that really take their game (Cozy, Playful cases):** the PlayStation's, GameCube's, Dreamcast's and Saturn's lid
  opens, the disc comes down onto the spindle and the lid shuts; the PS2's and Xbox's tray slides out and takes the disc in; a
  SNES, N64 or Mega Drive cartridge is pushed down behind the slot's lip and clicks; Game Boy, GBA and DS games go in at the back;
  the Wii pulls its disc in through the slot. Other systems keep their animation.

### Changed

- Cozy's start is slower, so you can see it: about 1.6 s until the game (it was about 1.1 s). Any button skips it at once, now
  also before the game's window is ready; Quick is unchanged, and Motion Reduced or Off start straight away.

### Fixed

- A game stuck in an error loop can't fill the memory with its log any more: a core writes at most 2 MB per game.

## [0.1.7] - 2026-10-31

### Added

- A **Cozy extras** card in the Tour (Settings → Help & tools → Tour) points to the page.

### Changed

- **Clear settings:** Settings has seven categories (Look, Sound & feel, Playing, Games, Connections, System, Help &
  tools), one line each with an icon, all seven on one screen at any text size; the line at the bottom names what is in
  the selected one. Every category is a short list of at most eight entries. What used to hide behind a shoulder button has an entry of its own: Appearance, Colours, Font, Motion,
  Systems on the shelf. The Theme page behind R2 is gone; Colours and Font are entries in Look and change right in the
  list. "Extras" is now **Pictures & extras**, and Tools and About moved to Help & tools. German names for all of it.
- **Quieter lists:** only the selected entry shows its marks (**>** on a page, **‹ ›** on a switch); the others show just
  their value.
- **Cozy extras, each where you see or hear it:** the page moved to Look and holds the eight you see (TV on the cabinet,
  stickers, printed discs, the memory card in the pause menu, light by time of day, decorations, game connections,
  Rediscover). **Playful cases** (was Playful animations), **Old-TV switch-on** and **Case sounds** are entries of Sound &
  feel, beside Animations, Motion and Sounds. The idle slideshow is **Slideshow before screen off** in System → Sleep &
  power. Your settings stay as they were.
- **Buttons page** (Settings → Playing → Buttons): button layout and button symbols in one place, with a picture of the
  four buttons and which one confirms and which goes back.
- **Systems on the shelf** says how systems appear when the list is empty.
- Pictures in the README and the manual show official games.
- Documentation: a new device list in the README and [docs/DEVICES.md](docs/DEVICES.md), grouped by chip with what each plays, the README's feature list brings 0.1.4 to 0.1.6, and the manual and screenshots match 0.1.6.

### Fixed

- A `.cue` written in Latin-1 with an umlaut in a file name (Windows tools) no longer hides its game from the shelf.

- Settings → Connections shows a connected Wi-Fi's name as it is: a network called "Off" or "Unknown" was shown as "Aus" or
  "Unbekannt" in German.
- Back from Report a problem lands on that row again (it landed on About).
- Tour and the first start's cards show only what the device has, like Settings: no Stick lights on handhelds without sticks
  (RG35XX Plus/2024, RG28XX).
- Leaving the idle slideshow or waking from sleep with a trigger pull no longer moves the shelf.
- The game's full page (X on a game in a system's list) shows the save slots row, "Save slots: 2 of 8" or "All empty", as the info page and the box's back do.
- The info page shows the save slots on the cartridge for SNES, GBA, GB, GBC and NES (the row was missing because
  these systems were not in the list of systems with saves).
- Cabinet: a tap on a case only counts when the Cabinet is in front, no longer under Settings, a card or setup.
- Colours and Font make the menu click when you change them, like the other Settings entries.
- Search: the text you type and Wi-Fi network names are shown as typed, not translated.
- Settings: each category's line names the entries this device has (no Store, no LEDs without sticks), in English and
  German.
- With light by time of day off, decorations are no longer re-scanned on every frame.
- About and backups name the release version.
- The shelf says hakuX and PortMaster, not their raw ids.
- At Large text, the explanation on Remote access and the first start's offline note wrap instead of running off the screen.

### Developer

- The gate runs its tests with nextest (one retry, flaky tests listed); the time-budget and rendering benchmarks run alone.
- The info page and hand-off tests no longer depend on the network.

## [0.1.6] - 2026-10-30

### Added

- **Theme** (Settings → Look & feel → Look, then R2): five colour sets for every page (Warm, the default; Indigo Cube,
  Grey Brick, Green X, Blue Station, each with a dark and a light version), three fonts (Barlow, Pixel, Rounded) and the
  button symbols in the hints: Nintendo letters, Xbox colours or PlayStation shapes. A live preview shows the choice
  ([docs/user/theme.md](docs/user/theme.md)).
- **Cozy extras** (Settings → Look & feel): each little touch on or off on its own: case sounds, old-TV switch-on, TV on
  the cabinet, stickers, memory card in the pause menu, printed discs, game connections, Rediscover, idle slideshow,
  light by time of day, decorations and playful animations.
- **Light by time of day** and **decorations that grow with your play** (Shelf and Cabinet): the room is cool and bright
  in the morning, warm with the lamp on in the evening and dim at night, and a plant, a lamp or a cactus stands beside the
  games where a shelf has room. The plant grows with your hours, a trophy appears after your first completed game, a framed
  "100 %" after your first 100 % sticker, and a system you played for 10 hours gets a small figurine.
- **Idle slideshow** (off until you switch it on in Cozy extras): after a quiet minute on the shelf your games' pictures
  play, slowly zooming. Any button ends it and does nothing else.
- **Playful animations** (Cozy extras, on with Cozy animations): cases spring out of the shelf and wiggle once, each game
  goes into its console its own way (a cartridge seats with a squash and a click, a disc tray slides out and shuts, the
  GameCube's and PlayStation's lid flips open and closed), the disc in an open box turns slowly, and the Shelf's spines
  lean when you scroll fast.
- **A new logo**: a pixel cartridge with the four system stripes on the boot screen, the first start, About and the README.

### Changed

- **A calmer pause menu:** Save and Load show the save slots as one slim row of small blocks in the colour of the
  game's memory card or cartridge, and the preview picture is as large as before. The memory card or cartridge with
  its blocks stays on the game's info page and box; **Memory card in the pause menu** (Cozy extras, off at first)
  brings it back to the pause menu.
- Names on spines get smaller or wrap in the wider fonts before anything is shortened with "…".
- README: a table of the devices pocket-ui was tested on and those it should run on.

## [0.1.5] - 2026-10-30

### Changed

- **Printed discs:** every disc and UMD shows its game's cover art and title in its system's look (PS1, PS2 with its blue
  rim and clear hub, GameCube mini disc, Wii, Dreamcast, Saturn, Xbox, a plain CD for the rest, the UMD with its window
  ring); a game without a cover gets a plain print with the title.
- **New cartridges:** the Game Boy Advance cartridge in dark grey and the Nintendo 64 one in light grey, redrawn from
  public-domain photos. The N64 box's spine takes its cover's colour, with the red block.
- README: a table of the devices pocket-ui was tested on and those it should run on.

## [0.1.4] - 2026-10-30

### Added

- **Stickers on your cases** (Shelf and Cabinet): Completed, 100 %, Favourite, Playing now and Game of the year with
  the year you pick. Put them on from the game's Stickers row; they sit a little crooked on the case's front and the
  game box, and one small one tops the spine on the shelf. Synced and backed up with your other game notes.
- **A TV on the cabinet** (Cabinet look): a small old CRT on the top board shows the selected game's screenshot (or
  your latest picture of it, or its cover), with scanlines and a warm glow, and crossfades as you move.
- **Connections**: the developer, publisher, series and genre on the info page and on the box's back lead to your other
  games with the same one ("More from Criterion Games", "Burnout series, in release order", "More racing games"). A small
  arrow marks a value that leads somewhere; Down picks it, A opens the list, B comes back. A value no other game of yours
  shares stays plain text. Where the data names no series, two different titles with the same name start can make one.
- **Rediscover** in Curator: games you played a year ago today, then those you have not played for a year, longest played
  first, each with the reason ("played a year ago today"). It also counts the "last played" of an EmulationStation
  gamelist; the row is only there while it has a game.
- **Old-TV power-on** at the end of Cozy's launch animation: the game comes on dark, with a bright line in the middle that
  opens into the picture and a glow that settles (a third of a second; Cozy with Motion on Full only).
- **Sounds of the cases** (Shelf and Cabinet, with menu sounds on): X opens a game's case with its own sound, a plastic
  case snapping open or a cartridge's cardboard box. A Cozy start plays the whole thing: the case opens, the cartridge
  slides in and clicks into place (a disc drops in and spins up, a game card clicks into its slot), then the old TV
  comes on with a thunk and a crackle of static. Quick keeps just a quiet click. At the level of the other menu sounds.
- **Saves on the save media of their era**: in the pause menu, Save and Load show what the game saved to back then,
  with its blocks beside it as that console's memory-card screen showed them: the PlayStation's or PS2's memory card,
  the GameCube's, the Dreamcast's VMU, the Xbox's memory unit, the PSP's Memory Stick Duo, or the game's own cartridge
  with its save label. A block with a save shows the game's cover. The same pictures, slots and buttons as before.
  The info page and the game box's back show the slots too ("Save slots: 3 of 8").

### Changed

- **Every game stands in its case** on the Shelf and in the Cabinet: the cartridge systems' games as their cardboard
  boxes (Super Nintendo, Nintendo 64, NES, Game Boy, Game Boy Advance), the rest in their cases as before. The cartridge,
  game card or disc is inside: X opens the box and it slides half out beside it; at the start (Cozy) the case opens and
  the game slides out of it into the console, the same way for every system.
- **Real sizes:** cases, boxes and what is inside them are drawn from the real things' millimetres at one scale (a Game
  Boy cartridge small beside a SNES one, a DS card tiny, a PS2 case tall, a jewel case wider than tall).
- Every system without a case of its own gets pocket-ui's own case: the cover under a band in the system's colour with
  its console and name, one size each for cartridge, disc and digital systems (Ports, PICO-8, TIC-80…).
- Long titles on spines are no longer cut while there is room: a little smaller, on two lines across a thick box (the
  subtitle on a line of its own), without the subtitle, and only then with "…".
- The cartridge systems' boxes (and the cartridges, cards and discs) come from photos of the real things, turned into
  pixel art; one system's boxes are all one size, and the front of a box is the cover edge to edge.
- The first start says "Step n of 7" (German: "Schritt n von 7") and, after a restart or a flat battery, opens again at
  the step it had reached. In the Wi-Fi step, A on the network you are connected to answers with a tick and the way on, and
  a network one search missed stays listed until three searches in a row miss it. The time zone a fresh image ships with
  does not count as "already set".
- Save slots say the real time of the save ("today 14:02", "yesterday 21:10", "23 Oct 2026, 14:02") instead of "just now".
- Menu sounds start at once: while pocket-ui is in front with the screen on and menu sounds on, the sound output stays
  open (on the RG35XX Plus the first sound after a quiet spell came 105 to 183 ms late; now 3 to 13 ms).
- The media sounds (cases, boxes, cartridges, discs, cards, the old TV) are recorded foley chosen by measurement, at the
  level of the other menu sounds.
- Add games shows the `smb://` address only while file sharing is on, and otherwise says how to turn it on.
- With only a few games on a shelf, the Shelf look stands them in from the end and the Cabinet shows a lone case in its
  middle. Settings categories get a scroll bar when larger text or German push some off the page. In the search keyboard,
  Up and Down land on the key right above or below.

### Fixed

- Quitting a PortMaster port closes only that launch and its helpers (also helpers in another process group), so a port
  started next keeps its controls.
- Port pictures and update notes are read in the background after a port is installed, updated or removed, so the shelf
  stays responsive while the card is read.
- An empty game file cannot start any more by accident: the card, the info page and the box say **Problem** with what to
  copy again, no cover is looked up for it, and the start message is clear.
- A sleep block (an app asking the system to stay awake) is respected by automatic sleep, the Sleep menu and lid sleep, not
  only by the power key; the screen stays dark and a button press wakes it.
- The shelf shows the proper title from a game's checksum (typos, glued articles) and a misspelled name no longer gets a
  wrong cover; the first list no longer waits for the checksum file. A name recognised during an interrupted background check
  reaches the shelf when the check resumes.
- Bluetooth headphones connected or disconnected in the menus adjust the sound output within two seconds; muted menu audio
  stops running in the background.

## [0.1.3] - 2026-10-29

### Added

- **Info page** in the Minimal, Cards and Split looks: X on a game shows everything on one screen (cover, your
  status, play time and rating, the facts, the description that scrolls in its place, a port's update note); SELECT
  shows the full card. Shelf and Cabinet keep the game box.
- Anbernic H700 handhelds can be installed, as **experimental**: the installers accept exactly the 11 models in the
  per-model table in [docs/DEVICES.md](docs/DEVICES.md#anbernic-h700-handhelds-2026-10-27-installable-2026-10-29-experimental)
  (RG28XX, five RG35XX, RG40XX H and V, RG34XX, RG34XX-SP, RG-SP), on ROCKNIX's H700 image. Computer tests for all models;
  the RG35XX Plus was also tried on a handheld, where games from Game Boy to PlayStation 1 run smoothly (Crash Bandicoot 2
  among them). The RG CubeXX (720x720) is refused with a sentence saying so.
- **PortMaster ports** are guests of the pause menu: HOME opens Continue, Options (Performance & fan), Quick settings and
  Quit over a running port. Quit closes the port and everything its start script launched. A port saves by itself, so
  the menu has no Save or Load.
- Ports show their PortMaster cover (else the screenshot) as the game box on the shelf, and the spine takes its colour. A
  port with a newer version in PortMaster says **Update available** in Extras and on its card and box; nothing updates by itself.
- **PS Vita** (Vita3K) as a guest of the pause menu like Wii U and PS3, with the Vita's own picture, a small blue-topped case on
  the shelf and covers from libretro's "Sony - PlayStation Vita" thumbnails. SM8550 handhelds only, experimental; the firmware and the
  games are your own.

### Changed

- **Animations: Cozy or Quick** (Settings → Look & feel → Animations, and on the look page, L1; it replaces the Launch
  animation switch). Cozy plays the launch animation long enough to see (about a second; a pocket-core game shows itself
  once it is over, any button skips it) and makes pages, the game box and the menus a little slower and softer; Quick is
  straight into the game. Until you choose, Cozy goes with the Shelf and Cabinet looks and Quick with the others.
- The Xbox core (hakuX) is updated to the current head of PR #9 (`2de4e9c`, rebased onto hakuX's own v1.2.0), which includes
  the pacing fix for the frame pickup and our audio relaunch fix; the build no longer needs a patch of ours. Measured on the
  Mini V2 with the usual settings it shows 59.9 new pictures a second, and Jade Empire's sound comes back after the relaunch
  into the game ([docs/XBOX.md](docs/XBOX.md)).

### Fixed

- A port that ends within seconds is a failed start: pocket-ui says "<name> didn't start. It may need files or libraries
  that aren't installed", with where to look, instead of counting a play session. Quit is not a failed start.
- A port's game box shows its cover (the cover is read while the card is open).
- H700: ROCKNIX's own volume box stays hidden from the first start, so only pocket-ui's shows when you change the volume.
- The pause menu no longer stays black on a screen where sway sends no resize (the H700's 640x480).
- After a game ends, its case on the Shelf is no longer left blank.
- The first-start note reads "an Anbernic ..." (the article follows the name).

## [0.1.2] - 2026-10-29

### Added

- **Xbox games**, played by hakuX (rfandango's Xbox emulator, Linux/libretro port by portare-ch) in RetroArch. Xbox is a
  system of its own on the shelf, with console picture, case and fact sheet. pocket-ui is the pause menu: Continue,
  Options (Resolution, Scaling), Quick settings and Quit. There are no save states yet; games save themselves, in the game.
  About 60 new pictures a second (was 47), sound after a relaunch (our audio fix, sent upstream), Quit in a blink.
  Runs on the Mini V2 and the SM8550 handhelds (Odin 2 family, Pocket 6, Nova), at 2x resolution by default on SM8550 only.
  All three downloads carry the hakuX core with its licence and source notes. Experimental ([docs/XBOX.md](docs/XBOX.md)).
- **Wii U** (Cemu) and **PS3** (RPCS3) as guests of the pause menu, with a performance profile and a clean Quit. SM8550
  handhelds only, the ones ROCKNIX ships them for.
- A launch animation while a game loads, and the game card shown as the game's box.
- A **This game** page: the game's picture, extras, controls, shortcuts (Fast-forward, Rewind, Earlier, Rumble), and where
  it runs. Open it from the card or the pause menu.
- PlayStation 1 and 2 games are recognised by the serial on the disc, not only by file name: `SLES_556.73.Something.iso`
  shows as "Final Fantasy VII", with its box art and facts ([docs/user/library.md](docs/user/library.md#playstation-1-and-2-discs)).
- A very quiet fan profile (off until 60 °C, stops below 55 °C).
- Report a problem, from Settings or with SELECT+START.
- START in the pause menu opens quick settings. The menu wraps around: Quit is one press Up from Continue.
- In preparation, not installable yet: profiles for the Anbernic RG28XX and RG35XX SP (H700) and a scaled layout for their small screens.

### Changed

- One look across the app: shared parts, warm neutrals, rows that lift off the page in Light, one scroll bar, one header
  for Settings pages, matching toasts and status panels, new emblems for Curator, Surprise me, PC streaming and stickers.
- The Cabinet look: every game stands in its system's real case on a shelf, the selected one large with its cover. Nothing
  is cut off and long titles stay readable.
- Big libraries start faster: covers load off the UI thread, generated libraries are reused after a save, and free memory
  goes back to the system every 20 s.
- Menu sounds follow the audio buffer's real size, so they no longer change under a sound.

### Fixed

- Menu sounds are heard again after a game on a big emulator.
- Quit takes one press. Xbox Quit no longer takes 18 s when its window is covered.
- A system with only one emulator is no longer relaunched for ever when a game fails; the error card says how to report it.
- Store lists and detail pages are never blank without a catalogue.
- A small screen (640x480) no longer crashes the interface.
- Volume-down on the Mini V2 shows the volume box every time.
- Smaller fixes to text fitting, German wording and the search field's caret.

## [0.1.1] - 2026-10-28

### Added

- **Two-file install** on the official ROCKNIX card: copy `mount-storage.sh` and `pocket-ui-seed.tar.gz` to the card's
  `ROCKNIX` partition on a PC and start twice. ROCKNIX stays untouched, and a failed install leaves a log on the card
  (`scripts/make-fat-install.sh`, `deploy/fat/`). Tested against a fake card only.
- **New menu sounds**: warm hardware samples (wooden tick, cartridge click, drawer, knob) instead of synthetic beeps, at a
  default level of 70 % (was 30 %) to match a game's loudness (`assets/ui-sounds/`).
- **A calm start**: no kernel text, a boot screen from systemd until the shelf takes over, and the shelf fading in from
  black ([docs/BOOT-SPLASH.md](docs/BOOT-SPLASH.md)). For a card set up earlier: `scripts/boot-quiet.sh`, with backup and undo.
- Pixel art in the interface: pictures above the words of empty pages, icons for the Settings categories, the handheld on
  first start and About, the Mini V2 with named controls in the manual, and stickers for milestones in the Year in review
  ([assets/ui-art/SOURCES.md](assets/ui-art/SOURCES.md)).
- Pictures and fact sheets for the SuperGrafx and the Pokémon mini, drawn from text prompts in the style of the others,
  not exact pictures of the hardware ([assets/systems/SOURCES.md](assets/systems/SOURCES.md)).
- Experimental support for the AYN Odin 2 family (Odin 2 Mini, Odin 2, Odin 2 Portal; ROCKNIX SM8550). Only the Odin 2 Mini
  was tried on a real device, over the command pipe with no button pressed: install checks, first start, shelf, a GBA game
  with the pause menu, screen off and wake. What is still open: [docs/DEVICES.md](docs/DEVICES.md#ayn-odin-2-family-2026-10-27-experimental).
- Licence texts travel with the programs: the package and the SD card image carry `LICENSE`, `NOTICE.md`, the Rust crates'
  and fonts' licences in a `licences/` folder. Settings › System › About has a Licences row naming it.
- `net.log` in the logs folder and in Report a problem: what NetworkManager, iwd and Tailscale said since the start, and
  the names (never passwords) of saved Wi-Fi networks.
- A Wi-Fi test bench (`scripts/wifi-bench/`, [docs/WIFI-BENCH.md](docs/WIFI-BENCH.md)): ROCKNIX's own network stack against
  simulated radios in eight scenarios (roaming, a vanishing hotspot, a hotel sign-in, a wrong password and more). It is
  not a test on a device.
- The update channel's address: pocketui.xyz for the signed list, GitHub Releases for the files, https only
  ([docs/UPDATE-CHANNEL.md](docs/UPDATE-CHANNEL.md)). Nothing is published there yet.

### Changed

- A newer ROCKNIX no longer blocks the install. The installers accept the tested ROCKNIX 20261001 and anything newer, say
  "tested with 20261001; your ROCKNIX is newer" and leave the decision to pocket-ui's own compatibility check
  (`pocket-ui --check-os-update /`), which still blocks when needed. An older ROCKNIX is asked to update first.
- The install guides are honest about the start: ROCKNIX on the Mini V2 needs its own one-time bootloader setup, and
  README, getting started and the card zip point to ROCKNIX's guide.
- One device list everywhere: the Mini V2 is the target; Pocket 5, Flip 2, first Mini, Pocket 6, Nova and the AYN Odin 2
  family are experimental.
- A hint how to copy files into `roms/ports` over ROCKNIX's file sharing (Samba) from Windows and macOS.
- **Switch off after sleeping is Off by default** (was 3 h): a sleeping Pocket stays asleep. If yours still switches off,
  set Settings › System › Sleep & power once.
- Power off and Restart clear the clock alarm and log it (`logs/shutdown.log`); the start clears a leftover one, so a
  Pocket never powers itself back on after Power off.
- The manual's longest pages are split in two; nothing was removed.
- English wording checked and made consistent (British spelling, "45%", Settings paths with "›"). Nothing works differently.

### Fixed

- **Bluetooth audio** no longer sounds broken. Games on Bluetooth headphones were heavily distorted on the Mini V2; the
  speaker was fine. The audio buffer was shorter than the Bluetooth sink's cycle and the pitch control wobbled. Measured
  off the device with `scripts/audio-bench`: 373 glitches in 8 s before, 0 in 10 minutes after
  ([docs/HOST.md](docs/HOST.md#bluetooth-audio-2026-10-25)). Not yet heard on a device with real headphones.
- **Wi-Fi after a boot**: quick settings and Settings › Wi-Fi share one picture of the radio and no longer disagree. A
  blocked radio shows "Wi-Fi is off" with A to turn it on, never "No networks found"; a failed scan says so; and if Wi-Fi
  is on in your settings but the system blocked the radio at start, pocket-ui switches it on once.
- **Wi-Fi in first-start setup**: after a wrong password the page kept offering the network as Saved and later showed
  "Connected" while nothing was. Now the key is dropped, the keyboard opens again with "Wrong password. Try again.", and
  after 60 s the page says the network isn't letting the Pocket in.
- **A hotspot that goes away and comes back** (phone, travel router): back in 4 to 21 s instead of 95 s, and a saved network
  heard again is joined at once.
- Sign in… on a hotel or café network said "No answer" the first time; it now waits 12 s instead of 5.
- ROCKNIX's own volume and brightness box no longer sits over pocket-ui while it is in front. Uninstalling puts it back.
- A small screen no longer crashes the interface: at 640x480 (the H700 handhelds' size) 62 of 620 snapshot states aborted,
  among them first start and every game card.
- Settings: the line under each category was cut with "…" at Medium and Large text; it now wraps whole.
- The Bluetooth page says "devices", not only "controllers": headphones and speakers are listed there too.
- Loading a missing or empty save state said "There is no the auto state" or "The the auto state is empty".

## [0.1.0] - 2026-10-28

The first public release, an early alpha: developed on one Retroid Pocket Mini V2 with ROCKNIX 20261001, plus a virtual
device. [docs/STATUS.md](docs/STATUS.md) says, feature by feature, how far each one has been proven.

### Added

- The shelf: Home with Continue, recent games, favourites, Curator and systems; five looks, dark and light; search as you
  type; cover art from the libretro thumbnail collection; play time.
- pocket-core, a libretro host for the handheld systems up to Nintendo DS, PSP, Dreamcast and Saturn, with resume, 8 save
  slots, the pause menu on HOME, per-console and per-game options, run-ahead and rewind.
- The same pause menu for PlayStation 2, GameCube, Wii and 3DS through ARMSX2, Dolphin and Azahar.
- Curator: a card per game with offline facts from Wikidata, filters, smart shelves, "more like this", history.
- First-start setup, Wi-Fi, Bluetooth controllers, sleep, battery, fan profiles, Syncthing, Tailscale, Moonlight, file
  sharing, a web companion, signed app updates with rollback.
- The manual on the device, in English, with menus in English or German ([docs/user/](docs/user/index.md)).

### Not included yet

- A published SD card image. It builds and passes its offline checks, but a complete first start as a new player (card,
  resize, setup, first game without help) has not been repeated with the fixed build.
- A public update server and Store catalogue.
