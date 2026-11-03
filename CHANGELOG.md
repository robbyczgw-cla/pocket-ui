# Changelog

All notable changes are listed here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/) (before 1.0, a minor version may change anything).

## [Unreleased]

## [0.1.14] - 2026-11-03

### Added

- Settings → Look → Screen colour: the screen's white from Cooler 10 to Warmer 10 as a row of its own, the same setting as SELECT + Vol. Left/Right change it at once, A goes back to Neutral.

- Two more **Screen effects** (Options → Picture): **Soft glow** puts a soft halo around bright parts and leaves the rest clear (no lines, no mask), and **Smooth pixels** rounds one-pixel stair-steps on slanted edges while flat pixels and pixel art stay sharp. Both are one pass in the existing picture shader, even at every size.

- Game facts (year, developer, genre, players, and the Wikipedia text once online) for 15 more systems: Neo Geo, Neo Geo CD, Amiga, C64, ZX Spectrum, Atari Jaguar, Atari 5200, Atari 800, 3DO, ColecoVision, Intellivision, Vectrex, PS3, PS Vita and Wii U.

- Console pages (X on a system, with Credits & links) for the Amiga, C64, ZX Spectrum, Atari 5200, Atari 8-bit family, PS3, PS Vita and Wii U, and short pages for PICO-8, TIC-80, ScummVM and DOS that say what they are: not consoles but programs. Each fact names its source (Wikipedia, Wikidata).

### Changed

- The game's full card says when a moment is waiting where you quit ("Continue: 2 h ago", under Save slots, or as its own row for games without slots), by the same checks as the open case on the shelf. Nothing is shown without one.
- Screen frames (Options → Picture → Screen frame) carry the names of the consoles they draw: Brick is **DMG**, Pocket slim **GB Pocket**, Shell **GBC**, Wide **GBA**, Clamshell **GBA SP**, Twin **DS**, Gloss **PSP**; their colour rows are renamed to match ("DMG colour", …). A frame shaped like one console is offered for that console's games only. Swan, Watch and Goggles are gone from the list (Auto still draws them for WonderSwan, Game & Watch and Virtual Boy; a game that had one chosen now uses Auto). Saved settings stay valid.
- Echoes (More like this), the Lens and Up next now also know the feel of about 840 games on one more system, which had no tags before.
- A game card without a description now says why: "No Wikipedia article for this game" (nothing to wait for), or "Offline: the description comes with Wi-Fi" (it comes with the next connection). English, German and Spanish; before, the card said nothing or an English error word.

### Fixed

- Cabinet: on a short shelf the empty strip under the shelf is used. The case and the TV beside it are shown bigger, and where room is still left the top of the next shelf looks in from below.
- Game info finds more of your games: "Mega Man X 2" and "Mega Man X 3" are no longer read as "Mega Man 10 2", and a Game Boy Color game filed in the Game Boy folder is found by its exact name first ("Link's Awakening DX" no longer shows the plain Link's Awakening).
- Dreamcast comes back to its moment and loads from Save slots again. Flycast's state is smaller at Quit (36 MB) than the size it reports at the next start (53 MB), so the file was refused as "the wrong size" and the game started fresh. Such a state now goes to the core padded to the full size. A save that fails because the state grew meanwhile is tried once more.

## [0.1.13] - 2026-11-03

### Added

- The Easy look (Settings → Look → Look): only your games, one at a time, for someone with a handful of them. Left and Right choose, A plays or resumes, X opens the game's card, HOME opens the quick settings, and in a game gives Continue, Save & quit and More. The first start offers it as its own row. It is calm: the case, name and buttons are smaller than in Kids Mode, More is a row like Continue and Quit, nothing is drawn over the game (no FPS or CPU numbers, whatever Over the game says), and there is no game switcher. An open case says once what open means.
- **Just Play** (prototype): one row on Home, right above Continue, and A starts a game picked for you: one you started and
  haven't finished first, then a starred one you never played, then any unplayed one; the game you played last is left
  out, hidden games never come up, and games on systems this handheld runs slowly come up less. No card, no question. It
  goes with Settings → Games → Curator → Lens & smart shelves and is not in Kids Mode.

### Changed

- Just Play's case on the Cabinet and the Shelf shows a picture: a game cartridge with a play triangle and two dice, in the pixel-art style of the other cases, instead of a plain grey case with its name.
- Console pages on a 1024 px screen: the Launch price and Hardware columns had 250 px, and "US$99.99" ran one character a line down the page. The second column now starts where the console's picture does, a label column gives way to a long value, and a value that still doesn't fit goes under its label. A short word torn across lines is now also reported by the text-size sweep (`W`).
- The game that stands out of an open case reads as meant: it comes out further beside the front (about half its width, and the name starts clear of it), and the strip above a spine is a little taller. The first time an open case is on screen one line says what it means, "Open means your moment is waiting" (English, German, Spanish; once, never in Kids Mode).
- Up next after a game (off by default, on with Enthusiast): **X** plays the suggestion, no longer A. A stays with the game the shelf is on, so a press of A right after a game continues that game and no longer starts another one.
- Help in German and Spanish says it is English: the Manual row reads "Handbuch (Englisch)" and "Manual (en inglés)"; the way back from the manual is named Help & tools (the manual said System).
- The first start's profile **Simple** is **The shelf** ("Your games on a shelf, as it comes."): in the first start, in Settings → Help & tools → How you like to play and in the manual, en/de/es. From Help & tools the page now says honestly what it does later: it puts every extra back as pocket-ui ships it, also On start and Screen frames (it is not the same as skipping then).
- Pause menu: the **Quick settings** row is gone (START opens them from any row, as its hint says); Options pages have a title ("Options", "Options › Picture" …) so you know where you are after two levels; Settings → Look no longer shows a key without its word at the end of its hint bar (R1 and L1 still work there; their rows are in Sound & feel).
- "Stickers" meant two things. Your own tags for games (Couch co-op) are now **Labels** in Curator, the Lens, the tour, the Settings row, the phone page and the manual; **Stickers** are the ones on the case. The card's row that holds both reads **Stickers & labels**. Saved labels and smart shelves are unchanged.
- "Extras" meant four things: the first start's page is **Pictures & extras** like its Settings row, its first row **Box art** (it said Cover art, and Box art under it), the pictures note no longer names the libretro collection or counts KB, and the pause menu's Options group is **Speed, rumble, fan** (Extras). English, German, Spanish and the manual.
- The list of recent games has one name, **Recent games**: the shelf's SELECT hint, the screen's title (it said Recently played, and Switch game from a game), the pause menu's SELECT hint (it said Games) and the manual (it said game switcher).
- HOME in Settings goes back to Home (it did nothing). Pages with something under way (an update, a download, a sign-in, a pairing, a clean-up, a button being remapped) keep HOME to themselves. On Home and the shelf HOME still opens the quick settings, in a game the pause menu.
- Pause menu: the Quit row says **Save & quit** while it keeps the moment, with "Next time, A continues from this moment." beside it. With Continue where I quit off (or nothing to save) it stays **Quit** with its own line.
- Game switcher: A says **Continue** only where a moment is waiting (the same check as the shelf's open cases) and **Play** otherwise, instead of promising Resume for every game. X **Remove** is gone: it took the game off the list and with it the play time it had counted.
- One word for going back to where you left off: **Continue**. A on a game with a waiting moment says Continue (it said Resume), the setting is **Continue where I quit** (Remember where I quit), Load's first stop is **Where I quit** (Auto), and the tour card is "Continue your last game". English, German and Spanish; the manual says the same.
- Frozen games is gone as a switch and as a word: Settings → System has no Frozen games row, the word on the cases is **Continue**, and the manual page is now **Continue**. An open case always means a moment is waiting. An old setting that switched it off is ignored (treated as on); the save rules did not change. Android has no row either.
- Pause menu → Load has a last stop, **From the start**, on pocket-ui's own host: the game begins again and nothing else changes; its own saves and the moment on the card stay until the next Save & quit replaces it.
- Remember where I quit off: the older moment is no longer the next start (the game starts as it would without one); it stays under Load as Auto until a Quit saves a new one. Applies to pocket-ui's own host; PlayStation 2, GameCube, Wii and 3DS are unchanged.

### Fixed

- Cabinet shows the selected shelf’s position and up/down arrows, so systems outside the visible shelves are easier to find.
- Box art: a picture server that did not answer (offline, a timeout, an error page, a broken file) no longer marks games as "has no cover" for good; they are asked for again at the next start. A run gives up after three failed requests in a row instead of waiting a minute per game, and Settings → Extras → Cover art says "Couldn't reach the picture server. Try again later."
- On a 1024x768 screen the first start's language line no longer ends in "Settin…": the sentence goes under the row, whole.

## [0.1.12] - 2026-11-02

### Added

- Commodore 64, Amiga, ZX Spectrum, Atari 800 and 5200 have their own picture on the shelf card
  and front, and the game goes into the right place when it starts, instead of the crate of games.
- SD card image: Settings → Help & tools → About names the system, "pocket-ui OS 0.1.9 · built on ROCKNIX 20261001
  (SM8250)" (on a ROCKNIX card with pocket-ui added, the ROCKNIX version). The device is called "pocket-ui" on the
  network, in Bluetooth and Tailscale, and EmulationStation's ROCKNIX update check is off.

### Changed

- USB file transfer (MTP) is no longer offered: no tile on Add games, no row in Settings → Connections, and the manual's "With a USB cable" part is gone. Nobody had completed a real transfer with it, and a Mac depends on OpenMTP, which was never shown to see the device. One that was switched on before this version is switched off at the start. The code stays, unused. Add games still offers File sharing and the phone page, plus the second card and the GAMES drive where they exist.
- RG28XX card: the first start's games step says the GAMES drive appears after this first start (switch off, card in a computer), and the README and docs/user/games-drive.md say that a computer shows no GAMES drive right after flashing, so there is no reason to flash again.
- SD card images: the big ROCKNIX source bundles are no longer uploaded next to the image (ROCKNIX, Knulli and pocknix publish none either). The card's SOURCE-OFFER.txt points to ROCKNIX's repository at the exact tag and commit and to the pocket-ui source (`pocket-ui-<version>-source.tar.gz`), and offers the complete source of the GPL and LGPL software on the card for at least three years from the image's release, on request through GitHub issues, for no more than the cost of providing it.
- Home with no games has a second button, **Add games** (X; also with no network), next to the starter pack, and B (**Look around**) puts the hint away:
  the shelf in your look stays empty with "No games yet", A brings the hint back. The hint returns at the next start while the card is empty and
  is gone once games appear.
- First start: once a Wi-Fi connection works, the setup shows "Connected to …" for a moment and goes on to the time zone by
  itself (one press fewer). A failed connection stays on the page; B from the time zone comes back and stays; Settings is unchanged.
- Mini V2 card README and INSTALL-DUAL-BOOT.md: the steps are for a Mini V2 with Retroid's own bootloader (firmware RPMiniV2_V1.0.0.275_20260407); with ROCKNIX's bootloader the card is untried, install the package instead.
- Home with no games, offline: the one button reads "Connect to Wi-Fi" and A opens the Wi-Fi page (one press instead of twelve to find it);
  the starter pack waits on Home when B comes back.
- Kids Mode's "No games to pick yet" page offers the starter pack with X, and comes back to the picker with the games.
- Settings → Help & tools → How you like to play: the first start's profile question again in one page, instead of 27 Tour cards.
- First start on a handheld with no Wi-Fi chip (RG28XX, RG35XX 2024): the Wi-Fi and Extras pages, which had nothing to choose, are
  left out. Five steps instead of seven; B and a restart in the middle follow the shorter way.
- SD card image: ROCKNIX updates are no longer installed on the card. At every start the boot hook moves any system
  update pocket-ui did not check (from EmulationStation's menu, the network share or copied by hand) to
  `/storage/.update-refused` and starts as usual; nothing there is deleted ([docs/SD-IMAGE.md](docs/SD-IMAGE.md)).
- On-device Manual: a link to another page of the guide opens that page (A on the framed link, X for the next link, or a tap; B goes back to where it came from). Links to a web address already showed the address.

### Fixed

- Manual (pause menu): the one-time note that HOME opens the menu is said only for the first game on pocket-ui's own host, not in RetroArch, standalone emulators or Android; the page now says so.
- Game check: when a folder had more than 50,000 entries and the check stopped at the first 50,000, it no longer says "All good"; it says "First 50,000 files checked".
- First start, Extras: at 1280x800 with Large text the note under the rows no longer touches the hint bar (the rows stand a little closer). Large text at 1024x768 is still tight there.
- Manual: a link of several words is one stop for X (it moved word by word before).
- First start, "For a child" on a handheld without Wi-Fi: after Kids Mode's setup it goes on to the time zone like the other play styles (no Wi-Fi page, steps numbered 4 and 5 of 5).
- Wi-Fi password: after a refused password the keyboard keeps what was typed (B fixes a letter); L1 now empties the field in one press, so
  leaving for the network list is L1, B instead of one B per letter.
- Android: a GBA or NES game comes back to its moment after Quit. The state it wrote was refused as the wrong size ("430144 of 528448 bytes") and the next start began fresh; the player now reads mGBA's and Nestopia's shorter states, as the handhelds do.
- Starting a game on a C64, Amiga, ZX Spectrum, Atari 800, MSX, Atari ST, Amstrad CPC or an arcade system (MAME, FinalBurn Neo, CPS, NAOMI) no
  longer shows a disc going in (those games come on floppies, tape or boards, which have no picture here): the case fades and the machine lights up.
  Amiga OCS/ECS and AGA use the Amiga's picture.
- 1024x768 screens: the Cards look keeps its game name and facts above the hints (the cards are lower there), and the on-screen keyboard's rows and its last row fit the screen instead of running over the hints and off the right edge.
- Settings → System → Report a problem only opens About with the row selected; the file is written when A is pressed there (before, merely landing on the row wrote a file into the games share).
- Android (0.1.11-android.1): the shared fixes of 0.1.10 and 0.1.11 come along (inserts on the console's middle, Light's case shadow, wrapped sentences, German and Spanish wording, patch-aware Frozen marks); a test pins that the phone's marks and older ones still stand open.

- Quick launch animation: a handheld's cartridge sinks behind the console's own outline, so on a tilted picture its corners no
  longer overlap the top edge; the slot bar is the console's own surface darkened, not black (it showed as a black stripe on a PlayStation's lid).
- The first game on a card says how to get out: "HOME opens the menu: save, load, quit." Nothing did before.
- First start: the last page ("You're all set") says "In a game, HOME opens the menu: save, load, quit." too, in English, German and Spanish (not in Kids Mode); a first game in RetroArch or another emulator has no line over it.
- For a child, with no games on the card: the first start ends on the page that adds games, not on the child's "ask a
  grown-up", and the Ready page no longer says it can be handed over.
- A handheld the project has played on (Odin 2 Mini, RG35XX Plus and SP) is no longer called untested at the first start.
- A device without Wi-Fi: the first start no longer promises Wi-Fi or a clock that sets itself.
- Extras and "You're all set" count the games on the card: "no games yet" stood there with six games on it.
- Add games with more than two wrong folders says how many and where to look, not a line of names cut off before the advice.
- A game's name on two lines stays its name: "Sonic Adventure" was "Sonic Aventura" on its case in Spanish.
- "1 Spiele", "Alle 1 Spiele holen" and their English and Spanish twins.
- Add games and the phone page say a computer's browser works too.
- A refused Wi-Fi password stays in the field, so one wrong letter doesn't mean typing it all again.
- Large text: Pixel Path shows each stage's grade under its name, so the values have the whole line, and the last seconds of play lose whole figures from the end instead of half of one; the Look step's lines about Animations, Motion and Appearance fit in German and Spanish; the selected row on Sync keeps its pill at 480 lines.
- A copied game the shelf leaves out because its file type belongs to another system, or because it lies more than one folder deep, now counts like a disc missing a file: the "can't be read" note after a rescan and Home's "No games yet" mention it, and point to Game check.
- SD card images: a build without a model (Mini V2) no longer gets GAMES drive files.
- SD card image: a card over about 36 GB (the 64 GB card an RG35XX SP comes with, for one) no longer ends in "Filesystem
  corruption has been detected" and a black screen after the first start's resize. Cards flashed with an earlier image
  have to be flashed again.
- Android: marking a game as a favourite keeps it selected, so Play starts that game.
- Android: the Games page with several shelves opened scrolled down to the shelf of the alphabetically first title after a fresh start. It now opens at the top (Recent games first); a remembered game still opens on its own shelf.
- On Android, a moment set aside while all eight slots are full is now under Load, after the last slot.
- On Android, Home during a slow game copy stops the start: no game comes up in the background later.
- On Android, Play works right after Game check found a disc's missing track copied in.
- On Android, back from Cozy extras lands on its row in Settings, no longer on the first.
- Tour and first-start cards on the Steam Deck: none for Haptics, Sync, Tailscale, Tools or Performance & fan.
- Steam Deck: no idle timers, suspend, power-key hold or low-battery power-off of ours any more; SteamOS owns sleep and the
  power button (the Settings rows were hidden, but the timers still ran and would have suspended after 4 minutes).
- 1024x768 screens: About keeps its two rows and their help above the hints, the first start's look step fits its rows above the hints (smaller preview), and Settings → Curator's rows leave room for their explanation.
- 1024x768 screens, more pages: Extras, "How do you like to play?", the feature cards, Search (reason over the keys), Tailscale, Web companion, RetroAchievements, Time, Battery, per-port Performance & fan, Pair a PC and History keep their lines above the hints and off each other; the pause menu's slot name no longer runs under the preview.

## [0.1.11] - 2026-11-02

### Added

- A second card that still holds a flashed system image (its small STORAGE partition would be the whole shelf) is said plainly on
  Add games and on Home's "No games yet": take it out, or format it for games. Nothing is reformatted.
- PlayStation 3 and Wii U have their own console picture on their shelf card and front (public-domain photos by Evan Amos, pixel-art
  conversion, lettering painted over), instead of the bare title card.
- A copy the shelf can't read no longer vanishes without a word: when a rescan finds unread game files (a disc
  without its file, a folder no system reads) and no new game, a note points to Game check, and an empty Home's "No
  games yet" says the files are there.
- Kids Mode: the PIN page names where the recovery file goes from the first look; the screens that stop the child say
  "Grown-ups: hold SELECT and START."; the grown-ups' page says when picked games are no longer on the card.
- The Manual shows the address after an outside link, to type on a phone. The phone upload page shows refusals in
  German and Spanish. After a skipped time zone with nothing to go on, the last first-start page says the clock is on UTC.
- The page you open on your phone for "From your phone" speaks Spanish too (after German and English), by the phone's
  language like the device.
- SD card image: Settings → Help & tools → About names the system, "pocket-ui OS 0.1.9 · built on ROCKNIX 20261001
  (SM8250)" (on a ROCKNIX card with pocket-ui added, the ROCKNIX version). The device is called "pocket-ui" on the
  network, in Bluetooth and Tailscale, and EmulationStation's ROCKNIX update check is off.

### Changed

- Updates: with no update list published yet, the page says "No updates published yet. A new version installs like
  the first one; your games and settings stay." Sleep says what it costs ("a few percent of the battery an hour").
- Words: second card ("only its games show while it's in"), empty Home's ways to add games, the remote access
  sentence, the manual's H700 and "START skips a step" lines, getting started leads with the installs that exist.
- Release scripts: the Ports scripts are uploaded and listed in `SHA256SUMS` under the names GitHub gives them
  (`Install.pocket-ui.sh`), so `sha256sum -c SHA256SUMS` passes for them too.

### Fixed

- Quit no longer kills a game still writing its saved moment. On a busy card the save could pass the 7 seconds after which the game was stopped, and the moment was lost; now nothing stops it while it writes, and a wait over a second shows it is working.
- Starting a game: the Game Boy Advance's cartridge goes into the middle of the console (it stood 35 pixels to the left on
  a 1240x1080 screen) and the DS's card comes straight down behind its upper screen. Every other system's plain animation, and
  Quick everywhere, aims at where the console's slot is on its picture, not at the middle of a picture that also holds the pad,
  at a size that fits the console.
- Starting a game in Light: no dark block stays where the case stood.
- The consoles at a game's start have clean outlines: no white fringe or grey patches left from their photos, and no white
  blade under the GameCube's open lid.
- Starting a game on a system without a console picture (arcade, home computers): its game goes into the crate of games, not
  into an empty grey box. The Saturn has a console picture everywhere now, in Quick too.
- Uploading games from your phone no longer moves the selection to another game in the shelf: the same game stays
  selected, as when the list is read again.
- Settings: the Kids mode row shows ">" when selected, like every row that opens a page.
- Tailscale, Sync, Bluetooth, the web companion and RetroAchievements: the line about the selected row wraps next to the
  hint pill instead of ending in "…" (Spanish "…en un navegad…").
- Explanations at the foot of a page no longer end on a single word ("ist."); a per-game Extras explanation takes the
  free lines above it before it is cut. German: "Ganzes Bild" over the small preview, "Behalten" in the first start's
  hint bar (the last key lost its label at 640x480), a shorter fan line so the note under it shows whole.
- Performance & fan on a 640x480 screen: the readout's long values (the CPU's clocks, the fan's line) have the room the
  short ones (temperature, power draw) don't need, instead of ending in "…". Starter pack: the explanation under the
  list stands at the left like every page's, no longer centred. German: the picture page's second line fits.
- First start / Settings → Systems on the shelf: a long line under the heading (German, Spanish, 640x480) takes a second
  line instead of ending in "…". Sentences in the Kids Mode pages, the game page, search and the report page no longer
  end on a single word.
- Look step (first start, Settings → Look): A "Keep this look" and B "Back" come right after Left/Right, so at Large text
  the keys that lose their label are the shortcuts (X, L1), no longer A.
- Performance & fan: the note about saved choices is shorter and shows whole on a 640x480 screen. First start, "How do you
  like to play?": the "Off:" line names two features and "and more" instead of running off the page.
- A fresh ROCKNIX card no longer starts the shelf with an empty "Splore" game that doesn't start: ROCKNIX's own PICO-8
  entry is hidden everywhere (shelf, counts, Recent, Add games, Kids Mode) until the official PICO-8 is installed, and a
  card holding only it says "No games yet".
- The install package no longer says pocket-ui "hasn't been tested" on an Odin 2 Mini, an RG35XX Plus or an RG35XX SP: it says the
  project has played on that model; the others still say they are untested.
- Two games with the same file name in different folders of one system share save states, as they always did, but no longer
  silently: a game only continues on its own from a moment it wrote last, the start says when they share and that saving can
  overwrite the other's, and deleting one leaves the shared states and pictures on the card. Rename one to keep them apart.
- Back up saves now includes ScummVM's saves, so a backup before flashing keeps your adventures.
- Adding, replacing or removing a soft patch no longer resumes the moment from the game's previous version: the game
  starts fresh and the moment stays under Load until your next Quit. This also holds with Frozen games switched off.
- Kids Mode: the grown-ups' page no longer offers "30 more minutes today" and "Start today's time again" while the time
  per day is "No limit".

## [0.1.10] - 2026-11-01

### Added

- Settings → System → Backup & reset backs up your saves and states: battery saves, save states, PS2 and GameCube memory
  cards, Wii and 3DS save data, no games and no BIOS files. It shows the size and the free space first and keeps the page
  moving while it copies. A restore never overwrites what is newer on the card. Flashing a card erases the folder, so copy it
  to a computer first; a second card keeps it ([docs/user/settings-system.md](docs/user/settings-system.md#backup--reset)).
- **Kids mode** (Settings → System, or "For a child" at the first start): a big, simple shelf with the games you pick, behind your PIN. Blue or Pink, a
  time per day with a warning five minutes before, a loudest volume. PS2, GameCube, Wii, 3DS and Xbox games too, on handhelds that play them; the emulators' and
  ROCKNIX's own shortcuts are closed meanwhile ([docs/user/kids-mode.md](docs/user/kids-mode.md)).
- Add games: a system the handheld runs but not every game of it smoothly (heavy ones such as PS2 and GameCube on a Snapdragon 865, Nintendo 64, PSP, Dreamcast and DS on an H700) says so in one plain sentence under its Files and BIOS lines.
- Add games: **From your phone** (a tile in the row of ways, also Y, also on the first start). Scan the code with your
  phone's camera, pick game files on the page it opens, and they land in the right system's folder; the shelf has them
  when you leave. Disc images and anything several systems take (`.iso`, `.chd`, `.cue`, `.zip`) ask which system. The
  Pocket stays awake while the page is open; it closes by itself after 10 minutes idle or when the screen goes dark
  ([docs/user/phone-upload.md](docs/user/phone-upload.md)).

### Fixed

- Kids mode: "Pick the games" shows the cover of the game under the cursor (it stayed an empty box on a device), and the case on the child's shelf fills as soon as its cover is read.
- Kids mode: the child's shelf reads the covers that arrive after it is shown (a case could stay blank), and stops repainting once they are in.
- Kids mode: a game that can't start stops on a picture asking for a grown-up. It no longer falls back to RetroArch, whose menu leaves the mode.
- Kids mode: a game paused by the screen going dark no longer uses up the day's time.
- Kids mode: a keyboard's Escape with the power menu, and a keyboard on the crash-loop door, no longer get past the PIN.
- A restore keeps a save that arrived or got newer after the comparison, also during the copy; it counts as kept.
- From your phone: a disc's tracks (`.bin`, `.raw`, `.img`, `.wav`) can be sent for Dreamcast, Saturn, Sega CD, PC Engine CD and the others, and go where the `.cue` or `.gdi` in their own folder goes.
- From your phone: choosing a system for one file no longer sends other files there; a button offers the same system for the rest, if you want it.
- The first saves backup after an update no longer goes to your other devices: the sync's ignore list is brought up to date first.
- Kids mode: a game hosted by pocket-core fills the screen as large as fits in its shape (a Game Boy Advance game on a 640x480 handheld is 640x427, no longer 480x320), whatever picture settings the grown-up saved; they come back when Kids mode is off.
- A large saves backup or restore keeps the handheld awake until it is done, instead of going to sleep halfway.

## [0.1.9] - 2026-11-01

### Added

- File sharing: nothing under `roms` is hidden any more. BIOS files, save states, `ports` and `PortMaster` can be copied over Wi-Fi file sharing (a BIOS file no longer needs a computer that writes ext4). Anyone with the share password can then also change them, and put programs into `ports` that run as root when started. Game check says where to copy a missing BIOS on your model.
- File sharing stays on across restarts until you switch it off, and comes back by itself once the Pocket is on Wi-Fi.
- Add games (Settings → Games → Add games, and the first start's last step): a row of tiles under the grid for the ways your
  handheld can take games, **File sharing** and **USB file transfer**. Reach it with Down from the grid, choose with Left
  and Right, A switches it, or tap it. USB file transfer is the same switch as Settings → Connections and both show
  the same state. Only the ways your handheld has appear; X still switches file sharing.
- Spanish: pocket-ui now speaks Español, in neutral Spanish for Spain and Latin America. Choose it on the Welcome page or
  in Settings → System → Language; a system set to any Spanish locale starts in it. The manual's pages stay in English.
- A second card on Anbernic H700 handhelds: put a card with a `roms` folder in the second slot and the shelf shows its games within a few seconds, also while the handheld is on. While it's in, only its games are on the shelf; its saves and states stay on that card. Add games says when the games come from it, and the shelf says when it's been pulled.
- USB file transfer: Settings → Connections → USB file transfer lets a computer copy games over a USB cable (MTP, ROCKNIX's own). Off by default, it stays on across restarts until you turn it off. Switched on and off on a Mini V2 and an Odin 2 Mini; copying to a computer has not been tried yet, and a Mac needs the free app OpenMTP, also untried.

### Changed

- PortMaster games that need your own files: the `port-files` folder is gone. Copy the files straight into the game's folder, `roms/ports/<game>/<folder>`, over file sharing or the web companion; "Copy your game files" shows the address. Files left in `port-files` stay on the card and are no longer moved.
- Remote access (SSH) no longer turns itself off after 30 idle minutes: it stays on until you switch it off, and is off after a restart.
- Enthusiast leaves the line over the game off: it's a choice of its own (Settings → Playing → Over the game), not part of the profile.

### Fixed

- File sharing writes as root: turning it on is instant and no longer interrupts a running copy, saves and port folders stay writable over the share, and a root-mounted exFAT or NTFS games card can be written. Add games says so when the card is read-only, and follows the real state while it is open. Safe mode's reset switches file sharing off; its password stays.
- Game check reports a game that is zeros all through (a copy that stopped early) as an empty file, no longer as ready; disc images are left alone.
- A game pocket-core couldn't start and that went to RetroArch goes back to pocket-core when you copy a new file over it (or delete it); a choice made under Runs on stays.
- X and Y are the right way round on Anbernic H700 handhelds. If you had swapped X and Y yourself on an H700, set them back.
- Game check no longer tells you to rename a save like `Game.nds.sav` to `Game.nds`: saves are left alone.
- Scaling preview on small screens: on 640x480 the Picture page's preview keeps at least a quarter of the height.

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
