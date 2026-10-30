# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may change anything).

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
